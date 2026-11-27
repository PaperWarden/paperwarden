import 'package:flutter/material.dart';

import '../documents/document_copy.dart';
import '../src/rust/api/document.dart';
import '../theme/palette.dart';
import '../theme/tokens.dart';

/// What the user must be told about a document that opened: a repair, and
/// content that was not run. Shows nothing when there is nothing to tell.
///
/// Each notice is one card that a screen reader reads as a whole and
/// announces when it appears.
class DocumentNotices extends StatelessWidget {
  const DocumentNotices(this.notices, {super.key});

  final List<DocumentNotice> notices;

  @override
  Widget build(BuildContext context) {
    final notRun = [
      for (final notice in notices)
        if (notice != DocumentNotice.repaired) notice,
    ];
    final cards = [
      if (notices.contains(DocumentNotice.repaired))
        const _NoticeCard(
          icon: Icons.healing_outlined,
          title: repairedTitle,
          lines: [repairedBody],
        ),
      if (notRun.isNotEmpty)
        _NoticeCard(
          icon: Icons.block_outlined,
          title: notRunTitle,
          lines: [notRunIntro],
          bullets: [for (final notice in notRun) notRunItem(notice)],
        ),
    ];
    if (cards.isEmpty) {
      return const SizedBox.shrink();
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final (index, card) in cards.indexed) ...[
          if (index > 0) const SizedBox(height: Space.md),
          card,
        ],
      ],
    );
  }
}

/// Why a document was refused, in plain words.
class DocumentRefusal extends StatelessWidget {
  const DocumentRefusal(this.reason, {super.key});

  final RefusalReason reason;

  @override
  Widget build(BuildContext context) {
    return _NoticeCard(
      icon: Icons.gpp_bad_outlined,
      title: refusedTitle,
      lines: [refusalReason(reason), refusedFooter],
      refusal: true,
    );
  }
}

class _NoticeCard extends StatelessWidget {
  const _NoticeCard({
    required this.icon,
    required this.title,
    required this.lines,
    this.bullets = const [],
    this.refusal = false,
  });

  final IconData icon;
  final String title;
  final List<String> lines;
  final List<String> bullets;
  final bool refusal;

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    final text = Theme.of(context).textTheme;
    final tone = refusal ? palette.accent : palette.warning;
    final body = text.bodyMedium?.copyWith(color: palette.textSecondary);
    return MergeSemantics(
      child: Semantics(
        liveRegion: true,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(Radii.card),
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: palette.surface,
              border: Border(left: BorderSide(color: tone, width: 4)),
            ),
            child: Padding(
              padding: const EdgeInsets.all(Space.lg),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  ExcludeSemantics(child: Icon(icon, color: tone, size: 24)),
                  const SizedBox(width: Space.md),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Semantics(
                          header: true,
                          child: Text(
                            title,
                            style:
                                text.titleMedium?.copyWith(color: palette.text),
                          ),
                        ),
                        for (final line in lines) ...[
                          const SizedBox(height: Space.xs),
                          Text(line, style: body),
                        ],
                        for (final bullet in bullets) ...[
                          const SizedBox(height: Space.xs),
                          Text('•  $bullet', style: body),
                        ],
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
