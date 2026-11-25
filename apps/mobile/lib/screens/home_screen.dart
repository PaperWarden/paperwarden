import 'package:flutter/material.dart';

import '../shell/app_shell.dart';
import '../widgets/empty_state.dart';
import '../widgets/hero_card.dart';
import '../widgets/page_scaffold.dart';
import '../widgets/quick_action.dart';
import '../widgets/section_heading.dart';
import '../widgets/trust_footer.dart';

/// Home tab: quick actions and recently opened documents.
///
/// Scan and Convert stay "Coming soon" until their engines pass feasibility.
/// Recent documents are empty until file import lands.
class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return PageScaffold(
      title: 'Your documents, under control.',
      footer: const TrustFooter('Processed on this device'),
      children: [
        HeroCard(
          title: 'Quick actions',
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Expanded(
                child: QuickAction(
                  icon: Icons.document_scanner_outlined,
                  label: 'Scan',
                  comingSoon: true,
                ),
              ),
              Expanded(
                child: QuickAction(
                  icon: Icons.description_outlined,
                  label: 'Files',
                  onTap: () => ShellScope.select(context, AppTab.files),
                ),
              ),
              const Expanded(
                child: QuickAction(
                  icon: Icons.sync,
                  label: 'Convert',
                  comingSoon: true,
                ),
              ),
            ],
          ),
        ),
        const SectionHeading('Recent documents'),
        const EmptyState(
          icon: Icons.history,
          title: 'No recent documents',
          message: 'Documents you open in PaperWarden appear here. '
              'They stay on this device.',
        ),
      ],
    );
  }
}
