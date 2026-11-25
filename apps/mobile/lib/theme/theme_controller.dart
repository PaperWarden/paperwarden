import 'package:flutter/material.dart';

/// Holds the user's light, dark or system theme choice.
///
/// Kept in memory for now; persisting it waits for local settings storage
/// (#20).
class ThemeController extends ValueNotifier<ThemeMode> {
  ThemeController([super.value = ThemeMode.system]);
}

/// Makes the app's [ThemeController] available to the widgets below it.
class ThemeScope extends InheritedNotifier<ThemeController> {
  const ThemeScope({
    super.key,
    required ThemeController super.notifier,
    required super.child,
  });

  static ThemeController of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<ThemeScope>();
    assert(scope != null, 'No ThemeScope above this context.');
    return scope!.notifier!;
  }
}

/// The label shown for each [ThemeMode].
extension ThemeModeLabel on ThemeMode {
  String get label => switch (this) {
        ThemeMode.system => 'System',
        ThemeMode.light => 'Light',
        ThemeMode.dark => 'Dark',
      };
}
