import 'package:flutter/material.dart';

import '../theme/palette.dart';
import '../theme/tokens.dart';

/// The filled brand-red card used for quick actions and privacy banners.
class HeroCard extends StatelessWidget {
  const HeroCard({
    super.key,
    required this.title,
    this.icon,
    this.body,
    this.child,
  });

  final String title;
  final IconData? icon;
  final String? body;

  /// Extra content below the heading, such as quick-action buttons.
  final Widget? child;

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    final text = Theme.of(context).textTheme;
    return Material(
      color: palette.brandFill,
      borderRadius: BorderRadius.circular(Radii.hero),
      child: Padding(
        padding: const EdgeInsets.all(Space.xl),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                if (icon != null) ...[
                  Icon(icon, color: palette.onBrand, size: 32),
                  const SizedBox(width: Space.md),
                ],
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        title,
                        style:
                            text.titleLarge?.copyWith(color: palette.onBrand),
                      ),
                      if (body != null) ...[
                        const SizedBox(height: Space.xs),
                        Text(
                          body!,
                          style:
                              text.bodyMedium?.copyWith(color: palette.onBrand),
                        ),
                      ],
                    ],
                  ),
                ),
              ],
            ),
            if (child != null) ...[
              const SizedBox(height: Space.lg),
              child!,
            ],
          ],
        ),
      ),
    );
  }
}
