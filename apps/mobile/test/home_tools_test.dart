import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/app.dart';
import 'package:paperwarden/screens/files_screen.dart';
import 'package:paperwarden/screens/home_screen.dart';
import 'package:paperwarden/screens/tools_screen.dart';
import 'package:paperwarden/widgets/grouped_list.dart';
import 'package:paperwarden/widgets/quick_action.dart';

Finder _quickAction(String label) => find.ancestor(
      of: find.text(label),
      matching: find.byType(QuickAction),
    );

Future<void> _openTools(WidgetTester tester) async {
  await tester.tap(find.descendant(
    of: find.byType(NavigationBar),
    matching: find.text('Tools'),
  ));
  await tester.pumpAndSettle();
}

void main() {
  group('Home', () {
    testWidgets('offers Scan, Files and Convert quick actions', (tester) async {
      await tester.pumpWidget(const PaperWardenApp());

      expect(find.text('Quick actions'), findsOneWidget);
      for (final label in ['Scan', 'Files', 'Convert']) {
        expect(_quickAction(label), findsOneWidget, reason: label);
      }
    });

    testWidgets('the Files quick action opens the Files tab', (tester) async {
      await tester.pumpWidget(const PaperWardenApp());

      await tester.tap(_quickAction('Files'));
      await tester.pumpAndSettle();

      expect(find.byType(FilesScreen), findsOneWidget);
      expect(find.byType(HomeScreen), findsNothing);
    });

    testWidgets('Scan and Convert are coming soon and cannot be opened',
        (tester) async {
      await tester.pumpWidget(const PaperWardenApp());
      final navigator = tester.state<NavigatorState>(find.byType(Navigator));

      for (final label in ['Scan', 'Convert']) {
        await tester.tap(_quickAction(label), warnIfMissed: false);
        await tester.pumpAndSettle();

        expect(find.byType(HomeScreen), findsOneWidget, reason: label);
        expect(navigator.canPop(), isFalse, reason: label);
        expect(
          tester.getSemantics(_quickAction(label)),
          matchesSemantics(
            label: '$label\nComing soon',
            isButton: true,
            hasEnabledState: true,
          ),
          reason: label,
        );
      }
    });

    testWidgets('Recent documents starts empty', (tester) async {
      await tester.pumpWidget(const PaperWardenApp());

      expect(find.text('Recent documents'), findsOneWidget);
      expect(find.text('No recent documents'), findsOneWidget);
    });
  });

  group('Tools', () {
    const tools = ['Convert', 'PDF Tools', 'Scan & OCR', 'Privacy', 'Sign'];

    testWidgets('shows the privacy banner and every tool', (tester) async {
      await tester.pumpWidget(const PaperWardenApp());
      await _openTools(tester);

      expect(find.text('Everything you need for documents.'), findsOneWidget);
      expect(find.text('Private by design.'), findsOneWidget);
      for (final tool in tools) {
        expect(
          find.descendant(
            of: find.byType(ToolsScreen),
            matching: find.text(tool),
          ),
          findsOneWidget,
          reason: tool,
        );
      }
    });

    testWidgets('no unbuilt tool can be opened', (tester) async {
      await tester.pumpWidget(const PaperWardenApp());
      await _openTools(tester);
      final navigator = tester.state<NavigatorState>(find.byType(Navigator));

      final rows = tester.widgetList<ListRow>(find.descendant(
        of: find.byType(ToolsScreen),
        matching: find.byType(ListRow),
      ));
      expect(rows.map((row) => row.title), tools);
      expect(rows.every((row) => row.comingSoon), isTrue);

      for (final tool in tools) {
        final row = find.descendant(
          of: find.byType(ToolsScreen),
          matching: find.text(tool),
        );
        await tester.ensureVisible(row);
        await tester.tap(row, warnIfMissed: false);
        await tester.pumpAndSettle();

        expect(find.byType(ToolsScreen), findsOneWidget, reason: tool);
        expect(navigator.canPop(), isFalse, reason: tool);
      }
      expect(
        find.descendant(
          of: find.byType(ToolsScreen),
          matching: find.text('Coming soon'),
        ),
        findsNWidgets(tools.length),
      );
    });
  });
}
