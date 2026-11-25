import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/screens/files_screen.dart';
import 'package:paperwarden/theme/app_theme.dart';

Widget _host() => MaterialApp(
      theme: AppTheme.light(),
      home: const Scaffold(body: FilesScreen()),
    );

ChoiceChip _chip(WidgetTester tester, String label) =>
    tester.widget<ChoiceChip>(find.widgetWithText(ChoiceChip, label));

void main() {
  const filters = ['All', 'PDF', 'Docs', 'Sheets', 'Scans'];

  testWidgets('shows search, filters and both sections', (tester) async {
    await tester.pumpWidget(_host());

    expect(find.widgetWithText(TextField, 'Search files'), findsOneWidget);
    for (final label in filters) {
      expect(find.widgetWithText(ChoiceChip, label), findsOneWidget);
    }
    expect(find.text('Folders'), findsOneWidget);
    expect(find.text('Recent files'), findsOneWidget);
  });

  testWidgets('starts with empty folders and files', (tester) async {
    await tester.pumpWidget(_host());

    expect(find.text('No folders yet'), findsOneWidget);
    expect(find.text('No files yet'), findsOneWidget);
  });

  testWidgets('All is selected first and exactly one filter is selected',
      (tester) async {
    await tester.pumpWidget(_host());

    expect(_chip(tester, 'All').selected, isTrue);

    for (final label in filters.reversed) {
      await tester.tap(find.widgetWithText(ChoiceChip, label));
      await tester.pump();

      for (final other in filters) {
        expect(_chip(tester, other).selected, other == label, reason: other);
      }
    }
  });

  testWidgets('ends with the on-device footer', (tester) async {
    await tester.pumpWidget(_host());
    final footer = find.text('Stored on this device');
    // The page list, not the search field's own scrollable.
    await tester.scrollUntilVisible(
      footer,
      200,
      scrollable: find.byType(Scrollable).first,
    );

    expect(footer, findsOneWidget);
  });
}
