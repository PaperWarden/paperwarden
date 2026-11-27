import 'package:flutter/material.dart';

import 'palette.dart';

/// Builds PaperWarden's light and dark [ThemeData] from the [Palette]s.
///
/// Uses the platform font (Roboto on Android, SF on iOS). A bundled font
/// would need its own license review, and fetching fonts at runtime would
/// break the offline-only rule.
abstract final class AppTheme {
  static ThemeData light() => _build(Palette.light);

  static ThemeData dark() => _build(Palette.dark);

  static ThemeData _build(Palette p) {
    final scheme = ColorScheme.fromSeed(
      seedColor: p.brandFill,
      brightness: p.brightness,
    ).copyWith(
      primary: p.brandFill,
      onPrimary: p.onBrand,
      secondary: p.accent,
      surface: p.surface,
      // Sheets, dialogs and menus use these; keep them neutral rather than
      // tinted by the red seed.
      surfaceContainerLowest: p.surface,
      surfaceContainerLow: p.surface,
      surfaceContainer: p.surface,
      surfaceContainerHigh: p.surface,
      surfaceContainerHighest: p.surface,
      onSurface: p.text,
      onSurfaceVariant: p.textSecondary,
      outline: p.outline,
      outlineVariant: p.outline,
    );

    final base = p.brightness == Brightness.dark
        ? Typography.material2021().white
        : Typography.material2021().black;
    final textTheme = base
        .copyWith(
          headlineLarge: const TextStyle(
            fontSize: 32,
            fontWeight: FontWeight.w700,
            height: 1.12,
            letterSpacing: -0.8,
          ),
          titleLarge:
              const TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
          titleMedium:
              const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
          titleSmall:
              const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
          bodyLarge: const TextStyle(fontSize: 16),
          bodyMedium: const TextStyle(fontSize: 14),
          bodySmall: const TextStyle(fontSize: 13),
          labelLarge:
              const TextStyle(fontSize: 14, fontWeight: FontWeight.w700),
          labelSmall: const TextStyle(
            fontSize: 11,
            fontWeight: FontWeight.w600,
            letterSpacing: 0.2,
          ),
        )
        .apply(bodyColor: p.text, displayColor: p.text);

    Color selectable(Set<WidgetState> states) =>
        states.contains(WidgetState.selected) ? p.accent : p.textSecondary;

    return ThemeData(
      useMaterial3: true,
      brightness: p.brightness,
      colorScheme: scheme,
      scaffoldBackgroundColor: p.background,
      textTheme: textTheme,
      extensions: [p],
      dividerTheme: DividerThemeData(color: p.outline, thickness: 1, space: 1),
      navigationBarTheme: NavigationBarThemeData(
        backgroundColor: p.surface,
        surfaceTintColor: Colors.transparent,
        indicatorColor: Colors.transparent,
        elevation: 0,
        labelBehavior: NavigationDestinationLabelBehavior.alwaysShow,
        iconTheme: WidgetStateProperty.resolveWith(
          (states) => IconThemeData(color: selectable(states), size: 24),
        ),
        labelTextStyle: WidgetStateProperty.resolveWith(
          (states) => TextStyle(
            fontSize: 12,
            fontWeight: states.contains(WidgetState.selected)
                ? FontWeight.w700
                : FontWeight.w500,
            color: selectable(states),
          ),
        ),
      ),
    );
  }
}
