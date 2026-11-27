import 'package:flutter/material.dart';

import '../theme/palette.dart';
import '../theme/tokens.dart';

/// Shows the privacy policy bundled with the app, so it reads offline.
///
/// [asset] is a copy of the repository's `PRIVACY.md`, kept identical by
/// `test/settings_screen_test.dart`.
class PrivacyPolicyScreen extends StatefulWidget {
  const PrivacyPolicyScreen({super.key});

  static const asset = 'assets/legal/PRIVACY.md';

  @override
  State<PrivacyPolicyScreen> createState() => _PrivacyPolicyScreenState();
}

class _PrivacyPolicyScreenState extends State<PrivacyPolicyScreen> {
  Future<String>? _markdown;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _markdown ??=
        DefaultAssetBundle.of(context).loadString(PrivacyPolicyScreen.asset);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Privacy policy')),
      body: FutureBuilder<String>(
        future: _markdown,
        builder: (context, snapshot) {
          final markdown = snapshot.data;
          if (markdown == null) {
            return const Center(child: CircularProgressIndicator());
          }
          return SafeArea(
            top: false,
            child: ListView(
              padding: const EdgeInsets.all(Space.xl),
              children: _render(context, markdown),
            ),
          );
        },
      ),
    );
  }

  /// Renders the headings, bullets and paragraphs `PRIVACY.md` uses.
  static List<Widget> _render(BuildContext context, String markdown) {
    final text = Theme.of(context).textTheme;
    final secondary = context.palette.textSecondary;
    final blocks = markdown.trim().split(RegExp(r'\n\s*\n'));
    final widgets = <Widget>[];

    for (final block in blocks) {
      if (block.startsWith('#')) {
        final level = block.indexOf(' ');
        widgets.add(Padding(
          padding: EdgeInsets.only(
            top: widgets.isEmpty ? 0 : Space.xl,
            bottom: Space.sm,
          ),
          child: Semantics(
            header: true,
            child: Text(
              block.substring(level + 1).trim(),
              style: level == 1 ? text.headlineLarge : text.titleLarge,
            ),
          ),
        ));
      } else if (block.startsWith('- ')) {
        for (final item in block.split(RegExp(r'\n(?=- )'))) {
          widgets.add(Padding(
            padding: const EdgeInsets.only(bottom: Space.sm),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('•  ', style: text.bodyLarge?.copyWith(color: secondary)),
                Expanded(
                  child:
                      Text(_unwrap(item.substring(2)), style: text.bodyLarge),
                ),
              ],
            ),
          ));
        }
      } else {
        widgets.add(Padding(
          padding: const EdgeInsets.only(bottom: Space.sm),
          child: Text(_unwrap(block), style: text.bodyLarge),
        ));
      }
    }
    return widgets;
  }

  /// Joins hard-wrapped source lines into one line.
  static String _unwrap(String block) =>
      block.split('\n').map((line) => line.trim()).join(' ');
}
