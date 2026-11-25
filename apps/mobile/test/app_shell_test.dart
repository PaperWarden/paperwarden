import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/app.dart';
import 'package:paperwarden/screens/files_screen.dart';
import 'package:paperwarden/screens/home_screen.dart';
import 'package:paperwarden/screens/settings_screen.dart';
import 'package:paperwarden/screens/tools_screen.dart';
import 'package:paperwarden/widgets/page_scaffold.dart';

Finder _destination(String label) => find.descendant(
      of: find.byType(NavigationBar),
      matching: find.text(label),
    );

void main() {
  testWidgets('opens on Home with four destinations', (tester) async {
    await tester.pumpWidget(const PaperWardenApp());

    expect(find.byType(NavigationDestination), findsNWidgets(4));
    for (final label in ['Home', 'Files', 'Tools', 'Settings']) {
      expect(_destination(label), findsOneWidget);
    }
    expect(find.byType(HomeScreen), findsOneWidget);
    expect(find.text('Your documents, under control.'), findsOneWidget);
  });

  testWidgets('each destination shows its screen', (tester) async {
    await tester.pumpWidget(const PaperWardenApp());
    const screens = <String, Type>{
      'Files': FilesScreen,
      'Tools': ToolsScreen,
      'Settings': SettingsScreen,
      'Home': HomeScreen,
    };

    for (final MapEntry(key: label, value: screen) in screens.entries) {
      await tester.tap(_destination(label));
      await tester.pumpAndSettle();
      expect(find.byType(screen), findsOneWidget, reason: label);
    }
  });

  testWidgets('tabs keep their state when switching', (tester) async {
    await tester.pumpWidget(const PaperWardenApp());
    final homeBefore = tester.element(find.byType(HomeScreen));

    await tester.tap(_destination('Tools'));
    await tester.pumpAndSettle();
    await tester.tap(_destination('Home'));
    await tester.pumpAndSettle();

    expect(
      identical(tester.element(find.byType(HomeScreen)), homeBefore),
      isTrue,
    );
  });

  testWidgets('settings shortcut opens the Settings tab', (tester) async {
    await tester.pumpWidget(const PaperWardenApp());
    // The bottom-navigation destination carries the same tooltip, so look
    // only inside the visible page.
    Finder shortcut() => find.descendant(
          of: find.byType(PageScaffold),
          matching: find.byTooltip('Settings'),
        );

    await tester.tap(shortcut());
    await tester.pumpAndSettle();

    expect(find.byType(SettingsScreen), findsOneWidget);
    expect(shortcut(), findsNothing);
  });

  testWidgets('follows the system dark theme', (tester) async {
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
    addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);

    await tester.pumpWidget(const PaperWardenApp());

    final context = tester.element(find.byType(HomeScreen));
    expect(Theme.of(context).brightness, Brightness.dark);
  });

  testWidgets('shows the on-device promise', (tester) async {
    await tester.pumpWidget(const PaperWardenApp());
    final footer = find.text('Processed on this device');
    await tester.scrollUntilVisible(
      footer,
      200,
      scrollable: find
          .descendant(
            of: find.byType(HomeScreen),
            matching: find.byType(Scrollable),
          )
          .first,
    );

    expect(footer, findsOneWidget);
  });
}
