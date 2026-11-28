/// Job history controls (ADR-0006, #63): keep history or not, keep file names
/// or not, and clear it. The Rust core enforces the choices; this layer only
/// reads and changes them.
library;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../src/rust/api/store.dart';

/// The history controls and how many entries there are.
@immutable
class HistoryState {
  const HistoryState({
    required this.enabled,
    required this.fileNames,
    required this.count,
  });

  /// Whether finished jobs are recorded.
  final bool enabled;

  /// Whether entries keep the file name.
  final bool fileNames;

  /// Entries in the history.
  final int count;

  @override
  bool operator ==(Object other) =>
      other is HistoryState &&
      other.enabled == enabled &&
      other.fileNames == fileNames &&
      other.count == count;

  @override
  int get hashCode => Object.hash(enabled, fileNames, count);
}

/// Where the history controls are stored.
abstract interface class HistoryRepository {
  Future<HistoryState> load();
  Future<void> setEnabled(bool enabled);

  /// Turning names off also removes the names already saved.
  Future<void> setFileNames(bool keep);
  Future<void> clear();
}

/// The repository backed by the Rust core's database.
class StoreHistoryRepository implements HistoryRepository {
  StoreHistoryRepository(this._store);

  final AppStore _store;

  @override
  Future<HistoryState> load() async {
    final prefs = await _store.historyPrefs();
    return HistoryState(
      enabled: prefs.enabled,
      fileNames: prefs.fileNames,
      count: await _store.historyCount(),
    );
  }

  @override
  Future<void> setEnabled(bool enabled) =>
      _store.setHistoryEnabled(enabled: enabled);

  @override
  Future<void> setFileNames(bool keep) =>
      _store.setHistoryFileNames(keep: keep);

  @override
  Future<void> clear() => _store.clearHistory();
}

/// The platform adapters' channel for app-private storage.
const storageChannel = MethodChannel('dev.paperwarden/storage');

/// Opens the core's database in app-private storage, excluded from backups.
Future<AppStore> openAppStore() async {
  final directory =
      await storageChannel.invokeMethod<String>('databaseDirectory');
  if (directory == null || directory.isEmpty) {
    throw StateError('no app-private storage directory');
  }
  return AppStore.open(path: '$directory/paperwarden.db');
}

/// The history repository. `main` overrides it with the core's database;
/// without that, the history controls show as unavailable.
final historyRepositoryProvider = Provider<HistoryRepository>(
  (ref) => throw StateError('the local database is not open'),
);

/// The current history controls.
final historyProvider =
    AsyncNotifierProvider<HistoryNotifier, HistoryState>(HistoryNotifier.new);

/// Reads and changes the history controls, reloading after each change so
/// the screen shows what the core stored.
class HistoryNotifier extends AsyncNotifier<HistoryState> {
  HistoryRepository get _repository => ref.read(historyRepositoryProvider);

  @override
  Future<HistoryState> build() => ref.watch(historyRepositoryProvider).load();

  Future<void> _change(Future<void> Function() change) async {
    state = await AsyncValue.guard(() async {
      await change();
      return _repository.load();
    });
  }

  Future<void> setEnabled(bool enabled) =>
      _change(() => _repository.setEnabled(enabled));

  Future<void> setFileNames(bool keep) =>
      _change(() => _repository.setFileNames(keep));

  Future<void> clear() => _change(_repository.clear);
}
