// Runs on an Android device or emulator: PDFium in an isolated service
// process (#59, ADR-0007).
//
//   python scripts/fetch_pdfium.py android-x64   # or the device's ABI
//   flutter test integration_test/render_service_test.dart -d <device>
//
// The corpus group runs only when the corpus has been copied to the device
// and its path passed in (docs/feasibility/PW-035-isolated-pdfium.md):
//   adb push tests/corpus/... /data/local/tmp/pw-corpus/...
//   --dart-define=PW_CORPUS=/data/local/tmp/pw-corpus

// Each direct call starts and discards a process, and the corpus is 16 jobs.
@Timeout(Duration(minutes: 5))
library;

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:paperwarden/documents/render_service.dart';
import 'package:paperwarden/src/rust/api/document.dart';
import 'package:paperwarden/src/rust/api/probe.dart';
import 'package:paperwarden/src/rust/api/render.dart';
import 'package:paperwarden/src/rust/frb_generated.dart';

const corpusRoot = String.fromEnvironment('PW_CORPUS');

/// `ServiceStatus` codes (core/crates/pdf/src/service.rs).
const rendered = 0;
const pageFailed = 10;
const timedOutBetweenPages = 13;

/// What `render_pdf` says when the service process died.
const engineStopped = 'the PDF engine stopped';

/// A budget measured on the Android 15 emulator (PW-035): 96 MiB above its
/// starting size is enough for PDFium to load and render a 512 px page, but
/// not a 4096 px one (a 64 MiB bitmap, then its conversion). Where the
/// address-space ceiling is hit decides whether the render fails cleanly
/// (`PageFailed`) or the process dies; both stop only the service. (64 MiB
/// is too close to what loading PDFium itself takes.)
const tightBudget = 96 << 20;

/// The two ways the ceiling stops a render.
final stoppedByCeiling = anyOf(
  const EngineOutcome(end: EngineEnd.finished, status: pageFailed),
  const EngineOutcome(end: EngineEnd.died, status: -1),
);

/// A one-page PDF with a correct cross-reference table.
List<int> onePagePdf() {
  const content = '0 0 1 rg 20 20 160 160 re f';
  final objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>',
    '<< /Length ${content.length} >>\nstream\n$content\nendstream',
  ];
  final out = StringBuffer('%PDF-1.7\n');
  final offsets = <int>[];
  for (final (index, body) in objects.indexed) {
    offsets.add(out.length);
    out.write('${index + 1} 0 obj\n$body\nendobj\n');
  }
  final xref = out.length;
  out.write('xref\n0 ${objects.length + 1}\n0000000000 65535 f \n');
  for (final offset in offsets) {
    out.write('${offset.toString().padLeft(10, '0')} 00000 n \n');
  }
  out.write('trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\n');
  out.write('startxref\n$xref\n%%EOF\n');
  return latin1.encode(out.toString());
}

/// Where PaperWarden's PDFium lies inside the installed APK, per ABI:
/// `[start, end)` byte offsets of each stored `libpaperwarden_pdfium.so`.
///
/// Native libraries load straight from the APK (`extractNativeLibs` is
/// false), so `/proc/self/maps` names `base.apk`, never the library. A
/// mapping of the APK whose file offset falls in one of these ranges is
/// PaperWarden's PDFium. (Every process also maps Android's own
/// `/system/lib64/libpdfium.so`, inherited from zygote; that is not ours.)
List<(int, int)> pdfiumRanges(String apk) {
  final file = File(apk).openSync();
  try {
    int u16(List<int> b, int at) => b[at] | b[at + 1] << 8;
    int u32(List<int> b, int at) => u16(b, at) | u16(b, at + 2) << 16;
    List<int> read(int at, int length) {
      file.setPositionSync(at);
      return file.readSync(length);
    }

    // End of central directory: the last 0x06054b50 in the final 64 KiB.
    final length = file.lengthSync();
    final tailStart = length > 65557 ? length - 65557 : 0;
    final tail = read(tailStart, length - tailStart);
    var eocd = tail.length - 22;
    while (eocd >= 0 && u32(tail, eocd) != 0x06054b50) {
      eocd--;
    }
    expect(eocd, greaterThanOrEqualTo(0), reason: 'APK has no zip directory');
    final count = u16(tail, eocd + 10);
    final directory = read(u32(tail, eocd + 16), u32(tail, eocd + 12));

    final ranges = <(int, int)>[];
    var at = 0;
    for (var entry = 0; entry < count; entry++) {
      expect(u32(directory, at), 0x02014b50);
      final method = u16(directory, at + 10);
      final size = u32(directory, at + 24);
      final nameLength = u16(directory, at + 28);
      final skip =
          nameLength + u16(directory, at + 30) + u16(directory, at + 32);
      final name =
          latin1.decode(directory.sublist(at + 46, at + 46 + nameLength));
      if (name.endsWith('/libpaperwarden_pdfium.so')) {
        expect(method, 0, reason: '$name must be stored to load from the APK');
        final local = read(u32(directory, at + 42), 30);
        final data =
            u32(directory, at + 42) + 30 + u16(local, 26) + u16(local, 28);
        ranges.add((data, data + size));
      }
      at += 46 + skip;
    }
    return ranges;
  } finally {
    file.closeSync();
  }
}

/// This process's installed APK, from its own mappings.
String appApk() => File('/proc/self/maps')
    .readAsLinesSync()
    .map((line) => line.split(RegExp(r'\s+')).last)
    .firstWhere((path) => path.endsWith('/base.apk'));

/// The `/proc/self/maps` lines among [mappings] that map PaperWarden's
/// PDFium.
List<String> pdfiumMappings(
    Iterable<String> mappings, List<(int, int)> ranges) {
  return [
    for (final line in mappings)
      if (line.endsWith('/base.apk'))
        if (line.split(RegExp(r'\s+')) case [final span, _, final offset, ...])
          if (span.split('-') case [final from, final to])
            if (ranges.any((range) {
              final start = int.parse(offset, radix: 16);
              final end =
                  start + int.parse(to, radix: 16) - int.parse(from, radix: 16);
              return start < range.$2 && end > range.$1;
            }))
              line,
  ];
}

late List<(int, int)> ours;

/// The app process's mappings of PaperWarden's PDFium; empty when the app
/// never loaded it.
List<String> appPdfiumMappings() =>
    pdfiumMappings(File('/proc/self/maps').readAsLinesSync(), ours);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async {
    await RustLib.init();
    ours = pdfiumRanges(appApk());
    expect(ours, isNotEmpty, reason: 'the APK carries no PaperWarden PDFium');
  });

  late Directory root;
  late String pdf;
  var sequence = 0;

  setUp(() async {
    root = await Directory.systemTemp.createTemp('pw-render');
    pdf = '${root.path}/one-page.pdf';
    await File(pdf).writeAsBytes(onePagePdf());
  });
  tearDown(() => root.delete(recursive: true));

  String jobs() => '${root.path}/jobs';
  String destination() => '${root.path}/page-${sequence++}.ppm';

  void expectNoWorkspace() {
    final dir = Directory(jobs());
    final left = dir.existsSync() ? dir.listSync() : const <FileSystemEntity>[];
    expect(left, isEmpty, reason: 'workspaces left behind');
  }

  /// A request straight to the service, for limits the job runner never
  /// asks for. The input is staged by hand, like the runner does.
  Future<ServiceAnswer> direct({
    required String input,
    int box = 512,
    int timeLimitMs = 30000,
    int memoryBudgetBytes = 512 << 20,
  }) async {
    final output = File('${root.path}/direct-${sequence++}.ppm')..createSync();
    return callRenderService(
      EngineRequest(
        input: input,
        output: output.path,
        maxWidth: box,
        maxHeight: box,
        maxInputBytes: BigInt.from(256 << 20),
        maxOutputBytes: BigInt.from(64 << 20),
        timeLimitMs: BigInt.from(timeLimitMs),
        memoryBudgetBytes: BigInt.from(memoryBudgetBytes),
      ),
    );
  }

  test('a job renders through the service and its page is verified', () async {
    final out = destination();
    final result = await renderPdf(
      path: pdf,
      jobsDir: jobs(),
      destination: out,
      maxWidth: 200,
      maxHeight: 200,
      cancel: CancelHandle(),
      engine: renderInService,
    );

    final bytes = File(out).readAsBytesSync();
    expect(result.bytes, BigInt.from(bytes.length));
    expect(latin1.decode(bytes.sublist(0, 15)), 'P6\n200 200\n255\n');
    expect(bytes.length, 15 + 200 * 200 * 3);
    expect(result.notices, isEmpty);
    expectNoWorkspace();
  });

  test('PDFium is loaded only in an isolated process without Internet',
      () async {
    final answer = await direct(input: pdf);

    expect(answer.outcome,
        const EngineOutcome(end: EngineEnd.finished, status: rendered));
    expect(answer.isolated, isTrue);
    expect(answer.internetGranted, isFalse);
    // The service rendered with PaperWarden's own PDFium build.
    expect(pdfiumMappings(answer.apkMappings!, ours), isNotEmpty);
    expect(answer.pid, isNot(answer.appPid));
    expect(answer.uid, isNot(answer.appUid));
    expect(answer.appPid, pid);
    expect(appPdfiumMappings(), isEmpty);
  });

  test('every job gets a fresh process', () async {
    final first = await direct(input: pdf);
    final second = await direct(input: pdf);

    expect(first.outcome.status, rendered);
    expect(second.outcome.status, rendered);
    expect(second.pid, isNot(first.pid));
  });

  test('a render over its memory ceiling stops only the service', () async {
    // Within the same budget a small page renders, so the ceiling, not
    // PDFium failing to load, is what stops the large one.
    final small = await direct(input: pdf, memoryBudgetBytes: tightBudget);
    expect(small.outcome.status, rendered);

    // A 4096 x 4096 render needs a 64 MiB bitmap.
    final large = await direct(
      input: pdf,
      box: 4096,
      memoryBudgetBytes: tightBudget,
    );

    // ignore: avoid_print
    print('4096 px page at 96 MiB: '
        '${large.outcome.end.name} ${large.outcome.status}');
    expect(large.outcome, stoppedByCeiling);
    // The app is alive and the next job, in a new process, renders.
    final next = await direct(input: pdf);
    expect(next.outcome.status, rendered);
    expect(next.pid, isNot(small.pid));
    expect(appPdfiumMappings(), isEmpty);
  });

  test('a render past its time limit is stopped', () async {
    final started = DateTime.now();
    final answer = await direct(input: pdf, timeLimitMs: 1);

    expect(
      answer.outcome,
      anyOf(
        const EngineOutcome(end: EngineEnd.timedOut, status: -1),
        const EngineOutcome(
            end: EngineEnd.finished, status: timedOutBetweenPages),
      ),
    );
    expect(DateTime.now().difference(started),
        lessThan(const Duration(seconds: 10)));
    expect((await direct(input: pdf)).outcome.status, rendered);
  });

  test('a killed render is a typed error and leaves no workspace', () async {
    Future<EngineOutcome> starved(EngineRequest request) async {
      final answer = await callRenderService(
        EngineRequest(
          input: request.input,
          output: request.output,
          maxWidth: 4096,
          maxHeight: 4096,
          maxInputBytes: request.maxInputBytes,
          maxOutputBytes: request.maxOutputBytes,
          timeLimitMs: request.timeLimitMs,
          memoryBudgetBytes: BigInt.from(tightBudget),
        ),
      );
      return answer.outcome;
    }

    Future<EngineOutcome> overdue(EngineRequest request) async {
      final answer = await callRenderService(
        EngineRequest(
          input: request.input,
          output: request.output,
          maxWidth: request.maxWidth,
          maxHeight: request.maxHeight,
          maxInputBytes: request.maxInputBytes,
          maxOutputBytes: request.maxOutputBytes,
          timeLimitMs: BigInt.one,
          memoryBudgetBytes: request.memoryBudgetBytes,
        ),
      );
      return answer.outcome;
    }

    for (final (engine, kind) in [
      (starved, BridgeErrorKind.engineFailed),
      (overdue, BridgeErrorKind.timedOut),
    ]) {
      // ignore: avoid_print
      print('killed job: expecting ${kind.name}');
      final out = destination();
      await expectLater(
        renderPdf(
          path: pdf,
          jobsDir: jobs(),
          destination: out,
          maxWidth: 200,
          maxHeight: 200,
          cancel: CancelHandle(),
          engine: engine,
        ),
        throwsA(isA<BridgeError>().having((e) => e.kind, 'kind', kind).having(
              (e) => e.message,
              'message',
              kind == BridgeErrorKind.engineFailed
                  ? anyOf(engineStopped, 'a page could not be rendered')
                  : isNotEmpty,
            )),
      );
      expect(File(out).existsSync(), isFalse);
      expectNoWorkspace();
    }
  });

  group('corpus', skip: corpusRoot.isEmpty ? 'PW_CORPUS not set' : null, () {
    test('every PDF fixture meets its outcome through the service', () async {
      final manifest = jsonDecode(
        File('$corpusRoot/manifest.json').readAsStringSync(),
      ) as Map<String, Object?>;
      final fixtures = (manifest['fixtures']! as List)
          .cast<Map<String, Object?>>()
          .where((fixture) => (fixture['path']! as String).endsWith('.pdf'));
      final failures = <String>[];
      for (final fixture in fixtures) {
        final path = fixture['path']! as String;
        final outcome = fixture['outcome']! as String;
        final started = DateTime.now();
        String result;
        String? message;
        var notices = <DocumentNotice>[];
        try {
          final rendered = await renderPdf(
            path: '$corpusRoot/$path',
            jobsDir: jobs(),
            destination: destination(),
            maxWidth: 1080,
            maxHeight: 1920,
            cancel: CancelHandle(),
            engine: renderInService,
          );
          result = 'rendered';
          notices = rendered.notices;
        } on BridgeError catch (error) {
          result = error.kind.name;
          message = error.message;
        }
        final ms = DateTime.now().difference(started).inMilliseconds;
        // ignore: avoid_print
        print(jsonEncode({
          'fixture': path,
          'outcome': outcome,
          'result': result,
          'notices': [for (final notice in notices) notice.name],
          if (message != null) 'message': message,
          'ms': ms,
        }));
        final ok = switch (outcome) {
          'accept' => result == 'rendered' && notices.isEmpty,
          'repair' =>
            result == 'rendered' && notices.contains(DocumentNotice.repaired),
          'neutralize' => result == 'rendered' && notices.isNotEmpty,
          // A typed refusal, by the pre-check or by PaperWarden's render
          // bound, is fine; a service that died is not.
          'bound' || 'reject' => result == 'refused' ||
              (result == 'engineFailed' && message != engineStopped) ||
              (outcome == 'bound' && result == 'rendered'),
          _ => false,
        };
        if (!ok) failures.add('$path: $outcome but $result $message $notices');
        expectNoWorkspace();
      }
      expect(failures, isEmpty);
      expect(appPdfiumMappings(), isEmpty);
    });

    test('the flate bomb, sent past the pre-check, is contained', () async {
      // PW-010 finding 1: in-process PDFium inflated this in full. The
      // pre-check refuses it, so it never reaches the service in a job; sent
      // straight there, the ceiling stops it while a normal page renders
      // within the same budget.
      final bomb = await direct(
        input: '$corpusRoot/security/pdf/flate-bomb.pdf',
        memoryBudgetBytes: tightBudget,
      );
      // ignore: avoid_print
      print('flate bomb at 96 MiB: ${bomb.outcome.end.name}');
      expect(
          bomb.outcome,
          isNot(
              const EngineOutcome(end: EngineEnd.finished, status: rendered)));
      final page = await direct(input: pdf, memoryBudgetBytes: tightBudget);
      expect(page.outcome.status, rendered);
      expect(appPdfiumMappings(), isEmpty);
    });
  });
}
