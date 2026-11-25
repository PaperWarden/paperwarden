import 'package:flutter/material.dart';

import '../shell/app_shell.dart';
import '../theme/palette.dart';
import '../theme/tokens.dart';

/// The layout shared by every tab: brand bar, large title, content, footer.
class PageScaffold extends StatelessWidget {
  const PageScaffold({
    super.key,
    required this.title,
    this.subtitle,
    this.showBrandBar = true,
    this.children = const [],
    this.footer,
  });

  final String title;
  final String? subtitle;

  /// Shows the "PaperWarden" wordmark and the settings shortcut.
  final bool showBrandBar;
  final List<Widget> children;

  /// Usually a [TrustFooter], placed after the content.
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    final palette = context.palette;
    return SafeArea(
      bottom: false,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(
          Space.xl,
          Space.md,
          Space.xl,
          Space.xxl,
        ),
        children: [
          if (showBrandBar) const _BrandBar(),
          Semantics(
            header: true,
            child: Text(title, style: text.headlineLarge),
          ),
          if (subtitle != null) ...[
            const SizedBox(height: Space.sm),
            Text(
              subtitle!,
              style: text.bodyLarge?.copyWith(color: palette.textSecondary),
            ),
          ],
          const SizedBox(height: Space.xl),
          ...children,
          if (footer != null) ...[
            const SizedBox(height: Space.xxl),
            footer!,
          ],
        ],
      ),
    );
  }
}

class _BrandBar extends StatelessWidget {
  const _BrandBar();

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.md),
      child: Row(
        children: [
          Expanded(
            child: Text(
              'PaperWarden',
              style: Theme.of(context).textTheme.titleSmall,
            ),
          ),
          IconButton(
            tooltip: 'Settings',
            icon: const Icon(Icons.settings_outlined),
            style: IconButton.styleFrom(
              backgroundColor: palette.surface,
              foregroundColor: palette.text,
              side: BorderSide(color: palette.outline),
            ),
            onPressed: () => ShellScope.select(context, AppTab.settings),
          ),
        ],
      ),
    );
  }
}
