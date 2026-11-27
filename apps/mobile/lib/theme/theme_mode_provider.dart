import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// The user's light, dark or system theme choice.
///
/// Kept in memory for now; persisting it waits for local settings storage
/// (#20).
final themeModeProvider =
    NotifierProvider<ThemeModeNotifier, ThemeMode>(ThemeModeNotifier.new);

/// Holds the selected [ThemeMode]. Starts by following the system.
class ThemeModeNotifier extends Notifier<ThemeMode> {
  @override
  ThemeMode build() => ThemeMode.system;

  /// Switches the app to [mode] immediately.
  void select(ThemeMode mode) => state = mode;
}

/// The label shown for each [ThemeMode].
extension ThemeModeLabel on ThemeMode {
  String get label => switch (this) {
        ThemeMode.system => 'System',
        ThemeMode.light => 'Light',
        ThemeMode.dark => 'Dark',
      };
}
