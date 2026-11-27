import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/theme/app_theme.dart';
import 'package:paperwarden/widgets/file_type_badge.dart';
import 'package:paperwarden/widgets/grouped_list.dart';
import 'package:paperwarden/widgets/trust_footer.dart';

Widget _host(Widget child) => MaterialApp(
      theme: AppTheme.light(),
      home: Scaffold(body: child),
    );

void main() {
  testWidgets('an available row responds to taps', (tester) async {
    var taps = 0;
    await tester.pumpWidget(_host(
      ListRow(title: 'Convert', onTap: () => taps++),
    ));

    await tester.tap(find.text('Convert'));

    expect(taps, 1);
    expect(find.byIcon(Icons.chevron_right), findsOneWidget);
  });

  testWidgets('a coming-soon row is labeled and cannot be opened',
      (tester) async {
    var taps = 0;
    await tester.pumpWidget(_host(
      ListRow(title: 'Sign', onTap: () => taps++, comingSoon: true),
    ));

    await tester.tap(find.text('Sign'), warnIfMissed: false);

    expect(taps, 0);
    expect(find.text('Coming soon'), findsOneWidget);
    expect(find.byIcon(Icons.chevron_right), findsNothing);
    expect(
      tester.getSemantics(find.byType(ListRow)),
      matchesSemantics(
        label: 'Sign\nComing soon',
        isButton: true,
        hasEnabledState: true,
      ),
    );
  });

  testWidgets('file-type badges use generic labels', (tester) async {
    const expected = {
      FileKind.pdf: 'PDF',
      FileKind.document: 'DOC',
      FileKind.spreadsheet: 'XLS',
      FileKind.image: 'IMG',
      FileKind.other: 'FILE',
    };
    await tester.pumpWidget(_host(Column(children: [
      for (final kind in expected.keys) FileTypeBadge(kind),
    ])));

    for (final label in expected.values) {
      expect(find.text(label), findsOneWidget);
    }
  });

  testWidgets('trust footer pairs a lock with its message', (tester) async {
    await tester.pumpWidget(_host(const TrustFooter('Stored on this device')));

    expect(find.byIcon(Icons.lock_outline), findsOneWidget);
    expect(find.text('Stored on this device'), findsOneWidget);
  });
}
