import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  // Loads the Rust core. A missing or mismatched library fails here, at
  // launch, rather than at the first document operation.
  await RustLib.init();
  runApp(const ProviderScope(child: PaperWardenApp()));
}
