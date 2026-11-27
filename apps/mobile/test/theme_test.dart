import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/theme/app_theme.dart';
import 'package:paperwarden/theme/palette.dart';

/// WCAG 2.x contrast ratio between two opaque colors.
double contrast(Color a, Color b) {
  final la = a.computeLuminance();
  final lb = b.computeLuminance();
  final lighter = la > lb ? la : lb;
  final darker = la > lb ? lb : la;
  return (lighter + 0.05) / (darker + 0.05);
}

const _aa = 4.5;

void main() {
  for (final palette in [Palette.light, Palette.dark]) {
    group('${palette.brightness.name} palette', () {
      test('text meets WCAG AA on background and surface', () {
        for (final ground in [palette.background, palette.surface]) {
          expect(contrast(palette.text, ground), greaterThanOrEqualTo(_aa));
          expect(
            contrast(palette.textSecondary, ground),
            greaterThanOrEqualTo(_aa),
          );
        }
      });

      test('brand accent meets WCAG AA as text', () {
        for (final ground in [palette.background, palette.surface]) {
          expect(contrast(palette.accent, ground), greaterThanOrEqualTo(_aa));
        }
      });

      test('the warning color meets WCAG AA as text and icon', () {
        for (final ground in [palette.background, palette.surface]) {
          expect(contrast(palette.warning, ground), greaterThanOrEqualTo(_aa));
        }
      });

      test('text on the brand fill meets WCAG AA', () {
        expect(
          contrast(palette.onBrand, palette.brandFill),
          greaterThanOrEqualTo(_aa),
        );
      });

      test('file-type badge labels meet WCAG AA', () {
        for (final fill in [
          palette.pdf,
          palette.document,
          palette.spreadsheet,
          palette.image,
          palette.other,
        ]) {
          expect(contrast(palette.onBrand, fill), greaterThanOrEqualTo(_aa));
        }
      });
    });
  }

  test('themes carry their palette and brightness', () {
    final light = AppTheme.light();
    final dark = AppTheme.dark();

    expect(light.brightness, Brightness.light);
    expect(dark.brightness, Brightness.dark);
    expect(light.extension<Palette>(), Palette.light);
    expect(dark.extension<Palette>(), Palette.dark);
    expect(light.scaffoldBackgroundColor, Palette.light.background);
    expect(dark.scaffoldBackgroundColor, Palette.dark.background);
  });

  test('selected navigation destination uses the brand accent', () {
    for (final theme in [AppTheme.light(), AppTheme.dark()]) {
      final accent = theme.extension<Palette>()!.accent;
      final icon =
          theme.navigationBarTheme.iconTheme!.resolve({WidgetState.selected});
      final label = theme.navigationBarTheme.labelTextStyle!
          .resolve({WidgetState.selected});
      expect(icon?.color, accent);
      expect(label?.color, accent);
    }
  });
}
