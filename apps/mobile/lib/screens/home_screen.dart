import 'package:flutter/material.dart';

import '../widgets/page_scaffold.dart';
import '../widgets/trust_footer.dart';

/// Home tab. Content arrives with PW-032 (#42).
class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return const PageScaffold(
      title: 'Your documents, under control.',
      footer: TrustFooter('Processed on this device'),
    );
  }
}
