import 'package:flutter/material.dart';

import '../widgets/page_scaffold.dart';

/// Settings tab. Content arrives with PW-034 (#45).
class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return const PageScaffold(title: 'Settings', showBrandBar: false);
  }
}
