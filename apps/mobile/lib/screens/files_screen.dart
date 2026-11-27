import 'package:flutter/material.dart';

import '../theme/palette.dart';
import '../theme/tokens.dart';
import '../widgets/empty_state.dart';
import '../widgets/page_scaffold.dart';
import '../widgets/section_heading.dart';
import '../widgets/trust_footer.dart';

/// The filters offered above the file list.
enum FileFilter {
  all('All'),
  pdf('PDF'),
  docs('Docs'),
  sheets('Sheets'),
  scans('Scans');

  const FileFilter(this.label);

  final String label;
}

/// Files tab: search, filters, folders and recent files.
///
/// Layout only. It reads no files and keeps no library: how PaperWarden
/// remembers documents without holding hidden copies of originals
/// (`ARCHITECTURE.md`) is decided by ADR under #9. Until then both sections
/// show empty states, and search and filters have nothing to act on.
class FilesScreen extends StatefulWidget {
  const FilesScreen({super.key});

  @override
  State<FilesScreen> createState() => _FilesScreenState();
}

class _FilesScreenState extends State<FilesScreen> {
  FileFilter _filter = FileFilter.all;

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    return PageScaffold(
      title: 'Files',
      footer: const TrustFooter('Stored on this device'),
      children: [
        TextField(
          textInputAction: TextInputAction.search,
          decoration: InputDecoration(
            hintText: 'Search files',
            prefixIcon: const Icon(Icons.search),
            filled: true,
            fillColor: palette.surface,
            contentPadding: const EdgeInsets.symmetric(vertical: Space.md),
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(Radii.control),
              borderSide: BorderSide(color: palette.outline),
            ),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(Radii.control),
              borderSide: BorderSide(color: palette.outline),
            ),
          ),
        ),
        const SizedBox(height: Space.md),
        Wrap(
          spacing: Space.sm,
          runSpacing: Space.sm,
          children: [
            for (final filter in FileFilter.values)
              _FilterChip(
                filter: filter,
                selected: filter == _filter,
                onSelected: () => setState(() => _filter = filter),
              ),
          ],
        ),
        const SectionHeading('Folders'),
        const EmptyState(
          icon: Icons.folder_outlined,
          title: 'No folders yet',
          message: 'Folders you make in PaperWarden appear here.',
        ),
        const SectionHeading('Recent files'),
        const EmptyState(
          icon: Icons.description_outlined,
          title: 'No files yet',
          message: 'Files you open in PaperWarden appear here. '
              'They stay on this device.',
        ),
      ],
    );
  }
}

class _FilterChip extends StatelessWidget {
  const _FilterChip({
    required this.filter,
    required this.selected,
    required this.onSelected,
  });

  final FileFilter filter;
  final bool selected;
  final VoidCallback onSelected;

  @override
  Widget build(BuildContext context) {
    final palette = context.palette;
    return ChoiceChip(
      label: Text(filter.label),
      selected: selected,
      onSelected: (_) => onSelected(),
      showCheckmark: false,
      selectedColor: palette.brandFill,
      backgroundColor: palette.surface,
      side: BorderSide(color: selected ? palette.brandFill : palette.outline),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Radii.control),
      ),
      labelStyle: Theme.of(context).textTheme.labelLarge?.copyWith(
            color: selected ? palette.onBrand : palette.text,
          ),
    );
  }
}
