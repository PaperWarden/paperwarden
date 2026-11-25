import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/app.dart';
import 'package:paperwarden/app_info.dart';
import 'package:paperwarden/screens/privacy_policy_screen.dart';
import 'package:paperwarden/screens/settings_screen.dart';
import 'package:paperwarden/widgets/grouped_list.dart';

Future<void> _openSettings(WidgetTester tester) async {
  await tester.pumpWidget(const PaperWardenApp());
  await tester.tap(find.descendant(
    of: find.byType(NavigationBar),
    matching: find.text('Settings'),
  ));
  await tester.pumpAndSettle();
}

Finder _inSettings(Finder finder) =>
    find.descendant(of: find.byType(SettingsScreen), matching: finder);

/// Scrolls the lazily built Settings list until the row exists and shows.
Future<Finder> _reveal(WidgetTester tester, String title) async {
  final row = _inSettings(find.widgetWithText(ListRow, title));
  await tester.scrollUntilVisible(
    row,
    200,
    scrollable: _inSettings(find.byType(Scrollable)).first,
  );
  await tester.pumpAndSettle();
  return row;
}

Future<void> _tapRow(WidgetTester tester, String title) async {
  final row = await _reveal(tester, title);
  await tester.tap(row);
  await tester.pumpAndSettle();
}

Brightness _brightness(WidgetTester tester) =>
    Theme.of(tester.element(find.byType(SettingsScreen))).brightness;

void main() {
  testWidgets('states that documents are processed locally', (tester) async {
    await _openSettings(tester);

    expect(find.text('Local-only processing'), findsOneWidget);
    expect(
      find.text('PaperWarden does not upload your documents.'),
      findsOneWidget,
    );
  });

  testWidgets('the theme choice applies immediately', (tester) async {
    await _openSettings(tester);
    expect(_brightness(tester), Brightness.light);

    await _tapRow(tester, 'Theme');
    await tester.tap(find.text('Dark'));
    await tester.pumpAndSettle();

    expect(_brightness(tester), Brightness.dark);
    expect(_inSettings(find.text('Dark')), findsOneWidget);

    await _tapRow(tester, 'Theme');
    await tester.tap(find.text('System'));
    await tester.pumpAndSettle();

    expect(_brightness(tester), Brightness.light);
    expect(_inSettings(find.text('System')), findsOneWidget);
  });

  test('the shown version matches pubspec.yaml', () {
    final pubspec = File('pubspec.yaml').readAsStringSync();
    final declared = RegExp(r'^version:\s*([^+\s]+)', multiLine: true)
        .firstMatch(pubspec)
        ?.group(1);

    expect(appVersion, declared);
  });

  testWidgets('shows the app version', (tester) async {
    await _openSettings(tester);
    final row = await _reveal(tester, 'Version');

    expect(tester.widget<ListRow>(row).value, appVersion);
  });

  test('the bundled privacy policy matches PRIVACY.md', () {
    final bundled = File(PrivacyPolicyScreen.asset).readAsStringSync();
    final source = File('../../PRIVACY.md').readAsStringSync();

    expect(bundled, source);
  });

  testWidgets('opens the privacy policy offline', (tester) async {
    await _openSettings(tester);

    await _tapRow(tester, 'Privacy policy');

    expect(find.byType(PrivacyPolicyScreen), findsOneWidget);
    expect(find.text('Commitments'), findsOneWidget);
    expect(
      find.textContaining('Documents and derived content are processed'),
      findsOneWidget,
    );
  });

  testWidgets('unbuilt settings are coming soon', (tester) async {
    await _openSettings(tester);
    const unbuilt = [
      'App Lock',
      'Biometric Unlock',
      'Default export format',
      'Storage used',
      'Clear cache',
    ];

    for (final title in unbuilt) {
      final row = await _reveal(tester, title);
      expect(tester.widget<ListRow>(row).comingSoon, isTrue, reason: title);
    }
  });
}
