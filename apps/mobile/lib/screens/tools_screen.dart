import 'package:flutter/material.dart';

import '../widgets/page_scaffold.dart';
import '../widgets/trust_footer.dart';

/// Tools tab. Content arrives with PW-032 (#42).
class ToolsScreen extends StatelessWidget {
  const ToolsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return const PageScaffold(
      title: 'Tools',
      subtitle: 'Everything you need for documents.',
      footer: TrustFooter('Processed on this device'),
    );
  }
}
