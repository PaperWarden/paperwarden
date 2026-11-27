import 'package:flutter/material.dart';

import '../theme/palette.dart';
import '../theme/tokens.dart';

/// A bordered card explaining why a section has nothing to show yet.
class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.icon,
    required this.title,
    required this.message,
  });

  final IconData icon;
  final String title;
  final String message;

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    final text = Theme.of(context).textTheme;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.surface,
        borderRadius: BorderRadius.circular(Radii.card),
        border: Border.all(color: palette.outline),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Space.xl,
          vertical: Space.xxl,
        ),
        child: MergeSemantics(
          child: Column(
            children: [
              Icon(icon, size: 32, color: palette.textSecondary),
              const SizedBox(height: Space.md),
              Text(title, style: text.titleMedium, textAlign: TextAlign.center),
              const SizedBox(height: Space.xs),
              Text(
                message,
                style: text.bodySmall?.copyWith(color: palette.textSecondary),
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ),
      ),
    );
  }
}
