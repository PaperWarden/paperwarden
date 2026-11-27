import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app.dart';
import 'history/history.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  // Loads the Rust core. A missing or mismatched library fails here, at
  // launch, rather than at the first document operation.
  await RustLib.init();
  // The local database. If it cannot open (for example, one written by a
  // newer version), the app still runs and the history controls say so.
  HistoryRepository? history;
  try {
    history = StoreHistoryRepository(await openAppStore());
  } on Object {
    history = null;
  }
  runApp(ProviderScope(
    overrides: [
      if (history != null) historyRepositoryProvider.overrideWithValue(history),
    ],
    child: const PaperWardenApp(),
  ));
}
