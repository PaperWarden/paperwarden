import 'package:flutter/material.dart';

/// Color roles for one brightness, taken from the approved mockups.
///
/// Every text/background pair is checked against WCAG AA in
/// `test/theme_test.dart`, so a palette change that hurts legibility fails CI.
@immutable
class Palette extends ThemeExtension<Palette> {
  const Palette({
    required this.brightness,
    required this.background,
    required this.surface,
    required this.outline,
    required this.text,
    required this.textSecondary,
    required this.accent,
    required this.warning,
    required this.brandFill,
    required this.onBrand,
    required this.pdf,
    required this.document,
    required this.spreadsheet,
    required this.image,
    required this.other,
  });

  static const light = Palette(
    brightness: Brightness.light,
    background: Color(0xFFF4F4F2),
    surface: Color(0xFFFFFFFF),
    outline: Color(0xFFE5E5E3),
    text: Color(0xFF111111),
    textSecondary: Color(0xFF6B6B6B),
    accent: Color(0xFFD0151F),
    warning: Color(0xFF9A5B00),
    brandFill: Color(0xFFD0151F),
    onBrand: Color(0xFFFFFFFF),
    pdf: Color(0xFFD0151F),
    document: Color(0xFF2B5797),
    spreadsheet: Color(0xFF1E7145),
    image: Color(0xFF5B4B8A),
    other: Color(0xFF5C5C5C),
  );

  // On dark grounds the accent is lightened for legibility, while filled
  // brand surfaces are darkened so white text on them stays above AA.
  static const dark = Palette(
    brightness: Brightness.dark,
    background: Color(0xFF0F0F10),
    surface: Color(0xFF1B1B1D),
    outline: Color(0xFF2C2C2F),
    text: Color(0xFFF2F2F2),
    textSecondary: Color(0xFFA3A3A8),
    accent: Color(0xFFFF5A5F),
    warning: Color(0xFFF0B23E),
    brandFill: Color(0xFFB3121A),
    onBrand: Color(0xFFFFFFFF),
    pdf: Color(0xFFB3121A),
    document: Color(0xFF2B5797),
    spreadsheet: Color(0xFF1E7145),
    image: Color(0xFF5B4B8A),
    other: Color(0xFF5C5C5C),
  );

  final Brightness brightness;

  /// Page background behind cards.
  final Color background;

  /// Cards, rows and navigation.
  final Color surface;

  /// Hairline borders and dividers.
  final Color outline;

  /// Primary text and icons.
  final Color text;

  /// Supporting text such as subtitles, sizes and dates.
  final Color textSecondary;

  /// Brand red used as text or icon color on [background] or [surface].
  final Color accent;

  /// Amber for warnings about a document, such as a repair or content that
  /// was not run, used as icon, border or text color on [background] or
  /// [surface].
  final Color warning;

  /// Brand red used as a filled surface, as in the hero cards.
  final Color brandFill;

  /// Text and icons placed on [brandFill] or a file-type badge.
  final Color onBrand;

  /// File-type badge fills.
  final Color pdf;
  final Color document;
  final Color spreadsheet;
  final Color image;
  final Color other;

  @override
  Palette copyWith({
    Brightness? brightness,
    Color? background,
    Color? surface,
    Color? outline,
    Color? text,
    Color? textSecondary,
    Color? accent,
    Color? warning,
    Color? brandFill,
    Color? onBrand,
    Color? pdf,
    Color? document,
    Color? spreadsheet,
    Color? image,
    Color? other,
  }) {
    return Palette(
      brightness: brightness ?? this.brightness,
      background: background ?? this.background,
      surface: surface ?? this.surface,
      outline: outline ?? this.outline,
      text: text ?? this.text,
      textSecondary: textSecondary ?? this.textSecondary,
      accent: accent ?? this.accent,
      warning: warning ?? this.warning,
      brandFill: brandFill ?? this.brandFill,
      onBrand: onBrand ?? this.onBrand,
      pdf: pdf ?? this.pdf,
      document: document ?? this.document,
      spreadsheet: spreadsheet ?? this.spreadsheet,
      image: image ?? this.image,
      other: other ?? this.other,
    );
  }

  @override
  Palette lerp(Palette? other, double t) {
    if (other == null) {
      return this;
    }
    Color mix(Color a, Color b) => Color.lerp(a, b, t) ?? b;
    return Palette(
      brightness: t < 0.5 ? brightness : other.brightness,
      background: mix(background, other.background),
      surface: mix(surface, other.surface),
      outline: mix(outline, other.outline),
      text: mix(text, other.text),
      textSecondary: mix(textSecondary, other.textSecondary),
      accent: mix(accent, other.accent),
      warning: mix(warning, other.warning),
      brandFill: mix(brandFill, other.brandFill),
      onBrand: mix(onBrand, other.onBrand),
      pdf: mix(pdf, other.pdf),
      document: mix(document, other.document),
      spreadsheet: mix(spreadsheet, other.spreadsheet),
      image: mix(image, other.image),
      other: mix(this.other, other.other),
    );
  }
}

/// Reads the active [Palette] from the theme.
extension PaletteContext on BuildContext {
  Palette get palette =>
      Theme.of(this).extension<Palette>() ??
      (Theme.of(this).brightness == Brightness.dark
          ? Palette.dark
          : Palette.light);
}
