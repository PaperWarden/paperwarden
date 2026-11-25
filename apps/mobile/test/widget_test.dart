import 'package:flutter_test/flutter_test.dart';
import 'package:paperwarden/main.dart';

void main() {
  testWidgets('shows the product promise', (tester) async {
    await tester.pumpWidget(const PaperWardenApp());
    expect(find.text('PaperWarden'), findsOneWidget);
    expect(
      find.text('Private document tools. On your device.'),
      findsOneWidget,
    );
  });
}
