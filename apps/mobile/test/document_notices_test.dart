import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/documents/document_copy.dart';
import 'package:paperwarden/src/rust/api/document.dart';
import 'package:paperwarden/theme/app_theme.dart';
import 'package:paperwarden/widgets/document_notices.dart';

Widget _host(Widget child, {ThemeData? theme}) => MaterialApp(
      theme: theme ?? AppTheme.light(),
      home: Scaffold(body: ListView(children: [child])),
    );

void main() {
  testWidgets('nothing is shown when there is nothing to tell', (tester) async {
    await tester.pumpWidget(_host(const DocumentNotices([])));

    expect(find.byType(Text), findsNothing);
  });

  testWidgets('a repaired file says so and that the original is untouched',
      (tester) async {
    await tester.pumpWidget(_host(
      const DocumentNotices([DocumentNotice.repaired]),
    ));

    expect(find.text(repairedTitle), findsOneWidget);
    expect(
        find.textContaining('original file was not changed'), findsOneWidget);
    expect(find.text(notRunTitle), findsNothing);
  });

  testWidgets('content that was not run is named, after the repair',
      (tester) async {
    await tester.pumpWidget(_host(const DocumentNotices([
      DocumentNotice.webLinks,
      DocumentNotice.repaired,
      DocumentNotice.launchActions,
    ])));

    expect(find.text(notRunTitle), findsOneWidget);
    expect(find.text('•  Links to websites'), findsOneWidget);
    expect(
      find.text('•  Actions that would open other apps or files'),
      findsOneWidget,
    );
    expect(
      tester.getTopLeft(find.text(repairedTitle)).dy,
      lessThan(tester.getTopLeft(find.text(notRunTitle)).dy),
    );
  });

  test('every finding and every refusal has words', () {
    for (final notice in DocumentNotice.values) {
      if (notice != DocumentNotice.repaired) {
        expect(notRunItem(notice), isNotEmpty, reason: notice.name);
      }
    }
    for (final reason in RefusalReason.values) {
      expect(refusalReason(reason), isNotEmpty, reason: reason.name);
    }
  });

  testWidgets('a refusal explains why in plain words', (tester) async {
    await tester.pumpWidget(_host(
      const DocumentRefusal(RefusalReason.expandsTooMuch),
    ));

    expect(find.text(refusedTitle), findsOneWidget);
    expect(
        find.text(refusalReason(RefusalReason.expandsTooMuch)), findsOneWidget);
    expect(find.text(refusedFooter), findsOneWidget);
  });

  testWidgets('each notice is read as one announced block with a heading',
      (tester) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(_host(const DocumentNotices([
      DocumentNotice.repaired,
      DocumentNotice.scripts,
    ])));

    final repaired = find.bySemanticsLabel(RegExp('^$repairedTitle'));
    expect(repaired, findsOneWidget);
    expect(
      tester.getSemantics(repaired),
      isSemantics(isLiveRegion: true, isHeader: true),
    );
    expect(
      tester.getSemantics(repaired).label,
      contains('Some pages or content may be missing'),
    );
    final notRun = find.bySemanticsLabel(RegExp('^$notRunTitle'));
    expect(tester.getSemantics(notRun).label, contains('Scripts'));
    semantics.dispose();
  });

  for (final (name, theme) in [
    ('light', AppTheme.light()),
    ('dark', AppTheme.dark()),
  ]) {
    testWidgets('notices and refusals meet text contrast in $name theme',
        (tester) async {
      await tester.pumpWidget(_host(
        const Column(children: [
          DocumentNotices([
            DocumentNotice.repaired,
            DocumentNotice.scripts,
            DocumentNotice.webLinks,
          ]),
          DocumentRefusal(RefusalReason.encrypted),
        ]),
        theme: theme,
      ));

      await expectLater(tester, meetsGuideline(textContrastGuideline));
    });
  }
}
