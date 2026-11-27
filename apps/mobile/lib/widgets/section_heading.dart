import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// A bold label introducing a group of rows, such as "Security".
class SectionHeading extends StatelessWidget {
  const SectionHeading(this.label, {super.key, this.action});

  final String label;

  /// Optional trailing control, such as a "View all" button.
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: Space.xl, bottom: Space.sm),
      child: Row(
        children: [
          Expanded(
            child: Semantics(
              header: true,
              child: Text(label, style: Theme.of(context).textTheme.labelLarge),
            ),
          ),
          if (action != null) action!,
        ],
      ),
    );
  }
}
