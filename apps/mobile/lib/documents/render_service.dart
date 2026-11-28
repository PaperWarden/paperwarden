// The platform's isolated render service, as the Rust job runner's engine
// (#59, ADR-0007).
//
// `renderPdf` in the bridge runs the job: it stages the input, runs the
// pre-check and verifies the output. For the PDFium step it calls
// [renderInService], which asks the platform to render in a process of its
// own. On Android that is `RenderService`, an `android:isolatedProcess`, so
// the app process never loads PDFium. This file only carries the request and
// the answer; it never reads the document.

import 'package:flutter/services.dart';

import '../src/rust/api/render.dart';

/// The channel to the platform render service (`MainActivity.kt`).
const renderChannel = MethodChannel('dev.paperwarden/render');

/// The service's full answer. Everything beyond [outcome] is evidence for
/// tests and diagnostics: which process rendered, and what it could do.
class ServiceAnswer {
  const ServiceAnswer({
    required this.outcome,
    this.pid,
    this.uid,
    this.appPid,
    this.appUid,
    this.isolated,
    this.internetGranted,
    this.apkMappings,
  });

  /// An answer for a service that could not be reached.
  static const unavailable = ServiceAnswer(
    outcome: EngineOutcome(end: EngineEnd.unavailable, status: -1),
  );

  /// What the job runner is told.
  final EngineOutcome outcome;

  /// The service process; `null` unless it answered.
  final int? pid;
  final int? uid;

  /// The app process that asked.
  final int? appPid;
  final int? appUid;

  /// Whether the service ran in an isolated process.
  final bool? isolated;

  /// Whether the service process held the Internet permission.
  final bool? internetGranted;

  /// The service process's mappings of the APK (`/proc/self/maps` lines).
  /// Native libraries load from the APK, so the file offsets show which
  /// libraries the service loaded.
  final List<String>? apkMappings;

  static const _ends = {
    'finished': EngineEnd.finished,
    'timedOut': EngineEnd.timedOut,
    'died': EngineEnd.died,
    'unavailable': EngineEnd.unavailable,
  };

  /// Reads the platform's answer; anything malformed is [unavailable].
  static ServiceAnswer fromMap(Map<String, Object?>? map) {
    final end = _ends[map?['end']];
    final status = map?['status'];
    if (map == null || end == null || status is! int) return unavailable;
    return ServiceAnswer(
      outcome: EngineOutcome(end: end, status: status),
      pid: map['pid'] as int?,
      uid: map['uid'] as int?,
      appPid: map['appPid'] as int?,
      appUid: map['appUid'] as int?,
      isolated: map['isolated'] as bool?,
      internetGranted: map['internetGranted'] as bool?,
      apkMappings: (map['apkMappings'] as List?)?.cast<String>(),
    );
  }
}

/// Sends [request] to the platform render service and returns its answer.
///
/// Never throws: a platform without the service, or an error on the way,
/// is [ServiceAnswer.unavailable], so the job fails with a typed error.
Future<ServiceAnswer> callRenderService(EngineRequest request) async {
  try {
    final answer = await renderChannel.invokeMapMethod<String, Object?>(
      'render',
      {
        'input': request.input,
        'output': request.output,
        'maxWidth': request.maxWidth,
        'maxHeight': request.maxHeight,
        'maxInputBytes': request.maxInputBytes.toInt(),
        'maxOutputBytes': request.maxOutputBytes.toInt(),
        'timeLimitMs': request.timeLimitMs.toInt(),
        'memoryBudgetBytes': request.memoryBudgetBytes.toInt(),
      },
    );
    return ServiceAnswer.fromMap(answer);
  } on MissingPluginException {
    return ServiceAnswer.unavailable;
  } on PlatformException {
    return ServiceAnswer.unavailable;
  }
}

/// The engine callback for `renderPdf`.
Future<EngineOutcome> renderInService(EngineRequest request) async =>
    (await callRenderService(request)).outcome;
