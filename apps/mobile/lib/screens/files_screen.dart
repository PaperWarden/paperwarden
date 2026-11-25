import 'package:flutter/material.dart';

import '../widgets/page_scaffold.dart';
import '../widgets/trust_footer.dart';

/// Files tab. Content arrives with PW-033 (#43).
class FilesScreen extends StatelessWidget {
  const FilesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return const PageScaffold(
      title: 'Files',
      footer: TrustFooter('Stored on this device'),
    );
  }
}
