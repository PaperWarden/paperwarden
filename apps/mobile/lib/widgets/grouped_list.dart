import 'package:flutter/material.dart';

import '../theme/palette.dart';
import '../theme/tokens.dart';

/// A rounded card holding [ListRow]s separated by hairline dividers.
class GroupedList extends StatelessWidget {
  const GroupedList({super.key, required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    return Material(
      color: palette.surface,
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Radii.card),
        side: BorderSide(color: palette.outline),
      ),
      child: Column(
        children: [
          for (var i = 0; i < children.length; i++) ...[
            if (i > 0) const Divider(indent: Space.lg, endIndent: Space.lg),
            children[i],
          ],
        ],
      ),
    );
  }
}

/// One row in a [GroupedList] or a list section.
///
/// A [comingSoon] row is shown dimmed with a "Coming soon" label and cannot be
/// opened. Use it for any capability whose engine has not passed feasibility,
/// so the UI never advertises an unproven feature (`PRODUCT_INVARIANTS.md` 12).
class ListRow extends StatelessWidget {
  const ListRow({
    super.key,
    required this.title,
    this.subtitle,
    this.leading,
    this.value,
    this.trailing,
    this.onTap,
    this.comingSoon = false,
  });

  final String title;
  final String? subtitle;
  final Widget? leading;

  /// Short current value shown before the chevron, such as "System".
  final String? value;

  /// Replaces the chevron, for example with a switch.
  final Widget? trailing;
  final VoidCallback? onTap;
  final bool comingSoon;

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    final text = Theme.of(context).textTheme;
    final enabled = !comingSoon && onTap != null;

    final details = Row(
      children: [
        if (leading != null) ...[leading!, const SizedBox(width: Space.md)],
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(title, style: text.titleMedium),
              if (subtitle != null) ...[
                const SizedBox(height: 2),
                Text(
                  subtitle!,
                  style: text.bodySmall?.copyWith(color: palette.textSecondary),
                ),
              ],
            ],
          ),
        ),
      ],
    );

    return MergeSemantics(
      child: Semantics(
        button: true,
        enabled: enabled,
        child: InkWell(
          onTap: enabled ? onTap : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: Space.lg,
              vertical: Space.md + 2,
            ),
            child: Row(
              children: [
                Expanded(
                  child: comingSoon
                      ? Opacity(opacity: 0.55, child: details)
                      : details,
                ),
                const SizedBox(width: Space.sm),
                if (comingSoon)
                  const ComingSoonBadge()
                else ...[
                  if (value != null)
                    Text(
                      value!,
                      style: text.bodyMedium
                          ?.copyWith(color: palette.textSecondary),
                    ),
                  if (trailing != null)
                    trailing!
                  else if (onTap != null)
                    Icon(Icons.chevron_right, color: palette.textSecondary),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Pill marking a capability that is not available yet.
class ComingSoonBadge extends StatelessWidget {
  const ComingSoonBadge({super.key});

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.background,
        borderRadius: BorderRadius.circular(Radii.pill),
        border: Border.all(color: palette.outline),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Space.sm,
          vertical: 3,
        ),
        child: Text(
          'Coming soon',
          style: Theme.of(context)
              .textTheme
              .labelSmall
              ?.copyWith(color: palette.textSecondary),
        ),
      ),
    );
  }
}
