import 'package:flutter/material.dart';

import '../theme/palette.dart';
import '../theme/tokens.dart';

/// An icon-and-label shortcut placed on a brand-red [HeroCard].
///
/// A [comingSoon] action keeps its full-contrast label, dims its icon, adds a
/// "Coming soon" caption and cannot be opened (`PRODUCT_INVARIANTS.md` 12).
class QuickAction extends StatelessWidget {
  const QuickAction({
    super.key,
    required this.icon,
    required this.label,
    this.onTap,
    this.comingSoon = false,
  });

  final IconData icon;
  final String label;
  final VoidCallback? onTap;
  final bool comingSoon;

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    final text = Theme.of(context).textTheme;
    final enabled = !comingSoon && onTap != null;

    return MergeSemantics(
      child: Semantics(
        button: true,
        enabled: enabled,
        child: InkWell(
          onTap: enabled ? onTap : null,
          borderRadius: BorderRadius.circular(Radii.control),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: Space.sm),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Opacity(
                  opacity: comingSoon ? 0.6 : 1,
                  child: Icon(icon, color: palette.onBrand, size: 30),
                ),
                const SizedBox(height: Space.sm),
                Text(
                  label,
                  style: text.labelLarge?.copyWith(color: palette.onBrand),
                ),
                if (comingSoon)
                  Text(
                    'Coming soon',
                    style: text.labelSmall?.copyWith(color: palette.onBrand),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
