import 'package:flutter/material.dart';

import '../theme/palette.dart';
import '../theme/tokens.dart';

/// Broad document families shown in file lists.
enum FileKind { pdf, document, spreadsheet, image, other }

/// A small colored document badge labeled with the file family.
///
/// Deliberately generic: it uses plain labels and colors, never third-party
/// product logos.
class FileTypeBadge extends StatelessWidget {
  const FileTypeBadge(this.kind, {super.key});

  final FileKind kind;

  static String labelFor(FileKind kind) => switch (kind) {
        FileKind.pdf => 'PDF',
        FileKind.document => 'DOC',
        FileKind.spreadsheet => 'XLS',
        FileKind.image => 'IMG',
        FileKind.other => 'FILE',
      };

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    final fill = switch (kind) {
      FileKind.pdf => palette.pdf,
      FileKind.document => palette.document,
      FileKind.spreadsheet => palette.spreadsheet,
      FileKind.image => palette.image,
      FileKind.other => palette.other,
    };
    return SizedBox(
      width: 32,
      height: 40,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: fill,
          borderRadius: BorderRadius.circular(Radii.badge),
        ),
        child: Center(
          child: Text(
            labelFor(kind),
            style: Theme.of(context).textTheme.labelSmall?.copyWith(
                  color: palette.onBrand,
                  fontSize: 9,
                  fontWeight: FontWeight.w800,
                ),
          ),
        ),
      ),
    );
  }
}
