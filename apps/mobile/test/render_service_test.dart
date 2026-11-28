import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/documents/render_service.dart';
import 'package:paperwarden/src/rust/api/render.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final request = EngineRequest(
    input: '/data/jobs/render-1/input/staged',
    output: '/data/jobs/render-1/output/result',
    maxWidth: 1080,
    maxHeight: 1920,
    maxInputBytes: BigInt.from(256 << 20),
    maxOutputBytes: BigInt.from(64 << 20),
    timeLimitMs: BigInt.from(30000),
    memoryBudgetBytes: BigInt.from(512 << 20),
  );

  void answerWith(Object? Function(MethodCall call) handler) {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(renderChannel, (call) async => handler(call));
  }

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(renderChannel, null);
  });

  test('the request reaches the service unchanged', () async {
    MethodCall? seen;
    answerWith((call) {
      seen = call;
      return {'end': 'finished', 'status': 0};
    });

    await renderInService(request);

    expect(seen?.method, 'render');
    expect(seen?.arguments, {
      'input': '/data/jobs/render-1/input/staged',
      'output': '/data/jobs/render-1/output/result',
      'maxWidth': 1080,
      'maxHeight': 1920,
      'maxInputBytes': 256 << 20,
      'maxOutputBytes': 64 << 20,
      'timeLimitMs': 30000,
      'memoryBudgetBytes': 512 << 20,
    });
  });

  test('each way the service can end becomes a typed outcome', () async {
    const ends = {
      'finished': EngineEnd.finished,
      'timedOut': EngineEnd.timedOut,
      'died': EngineEnd.died,
      'unavailable': EngineEnd.unavailable,
    };
    for (final MapEntry(key: end, value: expected) in ends.entries) {
      answerWith((_) => {'end': end, 'status': 7});
      expect(
        await renderInService(request),
        EngineOutcome(end: expected, status: 7),
        reason: end,
      );
    }
  });

  test('the evidence about the service process is kept', () async {
    answerWith((_) => {
          'end': 'finished',
          'status': 0,
          'pid': 20,
          'uid': 99001,
          'appPid': 10,
          'appUid': 10123,
          'isolated': true,
          'internetGranted': false,
          'apkMappings': ['7f00-7f10 r-xp 00002000 fe:05 7 /data/app/base.apk'],
        });

    final answer = await callRenderService(request);

    expect(answer.pid, 20);
    expect(answer.uid, 99001);
    expect(answer.appPid, 10);
    expect(answer.appUid, 10123);
    expect(answer.isolated, isTrue);
    expect(answer.internetGranted, isFalse);
    expect(answer.apkMappings, [
      '7f00-7f10 r-xp 00002000 fe:05 7 /data/app/base.apk',
    ]);
  });

  test('a malformed answer or a platform error is never a success', () async {
    const unavailable = EngineOutcome(end: EngineEnd.unavailable, status: -1);
    for (final answer in <Object?>[
      null,
      {'end': 'rendered', 'status': 0},
      {'end': 'finished'},
      {'end': 'finished', 'status': '0'},
    ]) {
      answerWith((_) => answer);
      expect(await renderInService(request), unavailable, reason: '$answer');
    }

    answerWith((_) => throw PlatformException(code: 'invalid'));
    expect(await renderInService(request), unavailable);
  });

  test('a platform without the service is unavailable', () async {
    // No handler registered: the platform has no render service.
    expect(
      await renderInService(request),
      const EngineOutcome(end: EngineEnd.unavailable, status: -1),
    );
  });
}
