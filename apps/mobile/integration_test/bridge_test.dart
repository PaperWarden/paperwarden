// Runs on a device or emulator against the real Rust library:
//   flutter test integration_test -d <device>
// Proves the bridge feasibility criteria from #12: sync and async calls,
// cancellation and typed errors; the document check from #61; and the
// local database through the platform storage adapter (#63).

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:paperwarden/history/history.dart';
import 'package:paperwarden/src/rust/api/document.dart';
import 'package:paperwarden/src/rust/api/probe.dart';
import 'package:paperwarden/src/rust/frb_generated.dart';

Matcher _bridgeError(BridgeErrorKind kind) =>
    isA<BridgeError>().having((error) => error.kind, 'kind', kind);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(RustLib.init);

  test('a synchronous call returns the core version', () {
    expect(coreVersion(), '0.0.1');
  });

  test('an asynchronous call completes without blocking Dart', () async {
    var ticks = 0;
    final timer = Timer.periodic(
      const Duration(milliseconds: 10),
      (_) => ticks++,
    );
    addTearDown(timer.cancel);

    final steps = await runProbeJob(
      steps: 20,
      stepMillis: 20,
      cancel: CancelHandle(),
    );

    expect(steps, 20);
    // The Rust work took ~400 ms on a worker thread; Dart kept running.
    expect(ticks, greaterThan(10));
  });

  test('cancelling stops a running call with a typed error', () async {
    final handle = CancelHandle();
    final stopwatch = Stopwatch()..start();
    final job = runProbeJob(steps: 1000, stepMillis: 10, cancel: handle);

    await Future<void>.delayed(const Duration(milliseconds: 50));
    handle.cancel();

    await expectLater(job, throwsA(_bridgeError(BridgeErrorKind.cancelled)));
    expect(handle.isCancelled(), isTrue);
    // Uncancelled, the job would take 10 s.
    expect(stopwatch.elapsed, lessThan(const Duration(seconds: 2)));
  });

  test('invalid arguments are rejected with a typed error', () async {
    await expectLater(
      runProbeJob(steps: 0, stepMillis: 0, cancel: CancelHandle()),
      throwsA(_bridgeError(BridgeErrorKind.invalidInput)),
    );
  });

  test('core validation errors reach Dart', () {
    expect(validateJobId(id: 'job-1'), 'job-1');
    expect(
      () => validateJobId(id: '../escape'),
      throwsA(_bridgeError(BridgeErrorKind.invalidInput)),
    );
  });

  test(
    'the database opens in private storage and keeps history controls',
    () async {
      final directory = await storageChannel.invokeMethod<String>(
        'databaseDirectory',
      );
      expect(directory, isNotNull);
      // App-private: inside the app's own sandbox, never shared storage.
      expect(
        directory,
        Platform.isIOS
            ? contains('Library/Application Support/PaperWarden')
            : contains('dev.paperwarden.paperwarden'),
      );

      final history = StoreHistoryRepository(await openAppStore());
      await history.setEnabled(false);
      await history.setFileNames(false);
      final reopened = StoreHistoryRepository(await openAppStore());
      expect(
        await reopened.load(),
        const HistoryState(enabled: false, fileNames: false, count: 0),
      );
      await reopened.setEnabled(true);
      await reopened.setFileNames(true);
      await reopened.clear();
      expect(
        await reopened.load(),
        const HistoryState(enabled: true, fileNames: true, count: 0),
      );
    },
  );

  group('document check', () {
    late Directory dir;
    setUpAll(() async {
      dir = await Directory.systemTemp.createTemp('pw-check');
    });
    tearDownAll(() => dir.delete(recursive: true));

    Future<String> write(String name, String content) async {
      final file = File('${dir.path}/$name');
      await file.writeAsBytes(latin1.encode(content));
      return file.path;
    }

    test('a damaged PDF with a script opens with typed notices', () async {
      // No cross-reference table or %%EOF, and JavaScript on open.
      final path = await write(
        'script.pdf',
        '%PDF-1.7\n1 0 obj\n<< /Type /Catalog /OpenAction '
            '<< /S /JavaScript /JS (app.alert(1)) >> >>\nendobj\n',
      );

      final check = await checkPdf(path: path, cancel: CancelHandle());

      expect(check.refusal, isNull);
      expect(check.notices, [DocumentNotice.repaired, DocumentNotice.scripts]);
    });

    test('a file that is not a PDF is refused with a typed reason', () async {
      final path = await write('invoice.pdf', 'MZ not a pdf at all');

      final check = await checkPdf(path: path, cancel: CancelHandle());

      expect(check.refusal, RefusalReason.notPdf);
      expect(check.notices, isEmpty);
    });

    test('a missing file is a typed error', () async {
      await expectLater(
        checkPdf(path: '${dir.path}/missing.pdf', cancel: CancelHandle()),
        throwsA(_bridgeError(BridgeErrorKind.unreadable)),
      );
    });
  });
}
