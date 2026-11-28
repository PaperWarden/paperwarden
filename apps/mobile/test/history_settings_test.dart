import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/history/history.dart';
import 'package:paperwarden/screens/settings_screen.dart';
import 'package:paperwarden/theme/app_theme.dart';
import 'package:paperwarden/widgets/grouped_list.dart';

/// An in-memory stand-in for the core's database, recording every change.
class FakeHistory implements HistoryRepository {
  FakeHistory({this.enabled = true, this.fileNames = true, this.count = 3});

  bool enabled;
  bool fileNames;
  int count;
  final calls = <String>[];

  @override
  Future<HistoryState> load() async =>
      HistoryState(enabled: enabled, fileNames: fileNames, count: count);

  @override
  Future<void> setEnabled(bool value) async {
    calls.add('enabled=$value');
    enabled = value;
  }

  @override
  Future<void> setFileNames(bool keep) async {
    calls.add('fileNames=$keep');
    fileNames = keep;
  }

  @override
  Future<void> clear() async {
    calls.add('clear');
    count = 0;
  }
}

class BrokenHistory implements HistoryRepository {
  @override
  Future<HistoryState> load() => Future.error(StateError('newer database'));
  @override
  Future<void> setEnabled(bool enabled) async {}
  @override
  Future<void> setFileNames(bool keep) async {}
  @override
  Future<void> clear() async {}
}

Future<void> _pump(
  WidgetTester tester,
  HistoryRepository repository, {
  ThemeData? theme,
}) async {
  await tester.pumpWidget(ProviderScope(
    overrides: [historyRepositoryProvider.overrideWithValue(repository)],
    child: MaterialApp(
      theme: theme ?? AppTheme.light(),
      home: const Scaffold(body: SettingsScreen()),
    ),
  ));
  await tester.pumpAndSettle();
}

Finder _row(String title) => find.widgetWithText(ListRow, title);

/// Scrolls the row into view, then taps it.
Future<void> _tapRow(WidgetTester tester, String title) async {
  await tester.ensureVisible(_row(title));
  await tester.pumpAndSettle();
  await tester.tap(_row(title));
  await tester.pumpAndSettle();
}

Switch _switchIn(WidgetTester tester, String title) => tester.widget<Switch>(
      find.descendant(of: _row(title), matching: find.byType(Switch)),
    );

void main() {
  testWidgets('shows the current controls and entry count', (tester) async {
    await _pump(tester, FakeHistory(fileNames: false));

    expect(_switchIn(tester, 'Keep job history').value, isTrue);
    expect(_switchIn(tester, 'Include file names').value, isFalse);
    expect(find.text('3 entries'), findsOneWidget);
  });

  testWidgets('turning history off stores it and locks the names switch',
      (tester) async {
    final history = FakeHistory();
    await _pump(tester, history);

    await _tapRow(tester, 'Keep job history');

    expect(history.calls, ['enabled=false']);
    expect(_switchIn(tester, 'Keep job history').value, isFalse);
    expect(_switchIn(tester, 'Include file names').onChanged, isNull);
    await _tapRow(tester, 'Include file names');
    expect(history.calls, ['enabled=false'], reason: 'locked while off');
  });

  testWidgets('hiding file names is stored', (tester) async {
    final history = FakeHistory();
    await _pump(tester, history);

    await tester.tap(find.descendant(
      of: _row('Include file names'),
      matching: find.byType(Switch),
    ));
    await tester.pumpAndSettle();

    expect(history.calls, ['fileNames=false']);
    expect(_switchIn(tester, 'Include file names').value, isFalse);
  });

  testWidgets('clearing asks first and can be cancelled', (tester) async {
    final history = FakeHistory();
    await _pump(tester, history);

    await _tapRow(tester, 'Clear history');
    expect(find.text('Clear job history?'), findsOneWidget);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();

    expect(history.calls, isEmpty);
    expect(find.text('3 entries'), findsOneWidget);
  });

  testWidgets('clearing empties the history and says so', (tester) async {
    final history = FakeHistory(count: 1);
    await _pump(tester, history);
    expect(find.text('1 entry'), findsOneWidget);

    await _tapRow(tester, 'Clear history');
    await tester.tap(find.text('Clear'));
    await tester.pumpAndSettle();

    expect(history.calls, ['clear']);
    expect(find.text('No entries'), findsOneWidget);
    expect(find.text('Job history cleared'), findsOneWidget);
  });

  testWidgets('an empty history cannot be cleared', (tester) async {
    final history = FakeHistory(count: 0);
    await _pump(tester, history);

    await _tapRow(tester, 'Clear history');

    expect(find.text('Clear job history?'), findsNothing);
    expect(history.calls, isEmpty);
  });

  testWidgets('an unavailable database is shown, not hidden', (tester) async {
    await _pump(tester, BrokenHistory());

    expect(
      find.text('History settings are unavailable right now.'),
      findsOneWidget,
    );
    expect(find.byType(Switch), findsNothing);
  });

  testWidgets('switch rows announce their state to screen readers',
      (tester) async {
    final semantics = tester.ensureSemantics();
    await _pump(tester, FakeHistory(fileNames: false));

    expect(
      tester.getSemantics(_row('Keep job history')),
      isSemantics(
        isToggled: true,
        hasToggledState: true,
        isEnabled: true,
        hasEnabledState: true,
        isButton: true,
        isFocusable: true,
        hasTapAction: true,
        hasFocusAction: true,
        label: 'Keep job history\nRemember which tools you used and when, on '
            'this device only.',
      ),
    );
    expect(
      tester.getSemantics(_row('Include file names')).label,
      contains('removes names already saved'),
    );
    semantics.dispose();
  });

  for (final (name, theme) in [
    ('light', AppTheme.light()),
    ('dark', AppTheme.dark()),
  ]) {
    testWidgets('the history controls meet text contrast in $name theme',
        (tester) async {
      await _pump(tester, FakeHistory(), theme: theme);
      await expectLater(tester, meetsGuideline(textContrastGuideline));
    });
  }
}
