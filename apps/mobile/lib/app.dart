import 'package:flutter/material.dart';

import 'shell/app_shell.dart';
import 'theme/app_theme.dart';
import 'theme/theme_controller.dart';

/// Root widget. Follows the system light or dark setting unless the user
/// picks one in Settings.
class PaperWardenApp extends StatefulWidget {
  const PaperWardenApp({super.key});

  @override
  State<PaperWardenApp> createState() => _PaperWardenAppState();
}

class _PaperWardenAppState extends State<PaperWardenApp> {
  final _theme = ThemeController();

  @override
  void dispose() {
    _theme.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ThemeScope(
      notifier: _theme,
      child: ValueListenableBuilder<ThemeMode>(
        valueListenable: _theme,
        builder: (context, mode, _) => MaterialApp(
          debugShowCheckedModeBanner: false,
          title: 'PaperWarden',
          theme: AppTheme.light(),
          darkTheme: AppTheme.dark(),
          themeMode: mode,
          home: const AppShell(),
        ),
      ),
    );
  }
}
