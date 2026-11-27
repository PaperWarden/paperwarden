import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'shell/app_shell.dart';
import 'theme/app_theme.dart';
import 'theme/theme_mode_provider.dart';

/// Root widget. Follows the system light or dark setting unless the user
/// picks one in Settings. Needs a [ProviderScope] above it (see `main.dart`).
class PaperWardenApp extends ConsumerWidget {
  const PaperWardenApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'PaperWarden',
      theme: AppTheme.light(),
      darkTheme: AppTheme.dark(),
      themeMode: ref.watch(themeModeProvider),
      home: const AppShell(),
    );
  }
}
