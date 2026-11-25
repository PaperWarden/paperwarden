import 'package:flutter/material.dart';

import '../theme/palette.dart';
import '../theme/tokens.dart';
import '../widgets/grouped_list.dart';
import '../widgets/hero_card.dart';
import '../widgets/page_scaffold.dart';
import '../widgets/trust_footer.dart';

/// Tools tab: the document capabilities PaperWarden is building.
///
/// Every tool is "Coming soon" until its engine passes feasibility and its
/// corpus lands (`PRODUCT_INVARIANTS.md` 12). Enable a row by giving it an
/// `onTap` and removing `comingSoon` in the PR that proves it.
class ToolsScreen extends StatelessWidget {
  const ToolsScreen({super.key});

  static const _tools = [
    (Icons.sync, 'Convert', 'Export between formats'),
    (Icons.picture_as_pdf_outlined, 'PDF Tools', 'Merge, split, compress'),
    (Icons.document_scanner_outlined, 'Scan & OCR', 'Scan and recognize text'),
    (Icons.privacy_tip_outlined, 'Privacy', 'Sanitize, redact, inspect'),
    (Icons.draw_outlined, 'Sign', 'Add and place signatures'),
  ];

  @override
  Widget build(BuildContext context) {
    final accent = context.palette.accent;
    return PageScaffold(
      title: 'Tools',
      subtitle: 'Everything you need for documents.',
      footer: const TrustFooter('Processed on this device'),
      children: [
        const HeroCard(
          icon: Icons.shield_outlined,
          title: 'Private by design.',
          body: 'Offline tools for viewing, converting and protecting files.',
        ),
        const SizedBox(height: Space.lg),
        GroupedList(
          children: [
            for (final (icon, title, subtitle) in _tools)
              ListRow(
                leading: Icon(icon, color: accent),
                title: title,
                subtitle: subtitle,
                comingSoon: true,
              ),
          ],
        ),
      ],
    );
  }
}
