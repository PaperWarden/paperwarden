import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app_info.dart';
import '../history/history.dart';
import '../theme/palette.dart';
import '../theme/theme_mode_provider.dart';
import '../theme/tokens.dart';
import '../widgets/grouped_list.dart';
import '../widgets/page_scaffold.dart';
import '../widgets/section_heading.dart';
import 'privacy_policy_screen.dart';

/// Settings tab.
///
/// The theme, the job history controls, the version and the privacy policy
/// work today. App Lock, export and storage settings are "Coming soon" until
/// those features exist; Clear cache will call the core's workspace recovery
/// once that is exposed through the bridge.
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final themeMode = ref.watch(themeModeProvider);
    return PageScaffold(
      title: 'Settings',
      showBrandBar: false,
      children: [
        const _LocalOnlyCard(),
        const SectionHeading('Security'),
        const GroupedList(children: [
          ListRow(
            title: 'App Lock',
            subtitle: 'Require a PIN to open the app',
            comingSoon: true,
          ),
          ListRow(
            title: 'Biometric Unlock',
            subtitle: 'Use fingerprint or face unlock',
            comingSoon: true,
          ),
        ]),
        const SectionHeading('History'),
        const _HistorySettings(),
        const SectionHeading('Appearance'),
        GroupedList(children: [
          ListRow(
            title: 'Theme',
            value: themeMode.label,
            onTap: () => _chooseTheme(context, ref, themeMode),
          ),
        ]),
        const SectionHeading('Export'),
        const GroupedList(children: [
          ListRow(title: 'Default export format', comingSoon: true),
        ]),
        const SectionHeading('Storage'),
        const GroupedList(children: [
          ListRow(title: 'Storage used', comingSoon: true),
          ListRow(
            title: 'Clear cache',
            subtitle: 'Remove temporary files',
            comingSoon: true,
          ),
        ]),
        const SectionHeading('About'),
        GroupedList(children: [
          ListRow(
            title: 'Version',
            value: appVersion,
            onTap: () => showAboutDialog(
              context: context,
              applicationName: 'PaperWarden',
              applicationVersion: appVersion,
            ),
          ),
          ListRow(
            title: 'Privacy policy',
            onTap: () => Navigator.of(context).push(MaterialPageRoute<void>(
              builder: (_) => const PrivacyPolicyScreen(),
            )),
          ),
        ]),
      ],
    );
  }

  static Future<void> _chooseTheme(
    BuildContext context,
    WidgetRef ref,
    ThemeMode current,
  ) {
    return showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (sheetContext) => SafeArea(
        child: RadioGroup<ThemeMode>(
          groupValue: current,
          onChanged: (mode) {
            if (mode != null) {
              ref.read(themeModeProvider.notifier).select(mode);
            }
            Navigator.of(sheetContext).pop();
          },
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final mode in ThemeMode.values)
                RadioListTile<ThemeMode>(value: mode, title: Text(mode.label)),
            ],
          ),
        ),
      ),
    );
  }
}

/// The job history controls (ADR-0006). The core enforces them: with history
/// off nothing is recorded, and with names off none is stored.
class _HistorySettings extends ConsumerWidget {
  const _HistorySettings();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final history = ref.watch(historyProvider);
    final notifier = ref.read(historyProvider.notifier);
    return switch (history) {
      AsyncData(:final value) => GroupedList(children: [
          ListRow(
            title: 'Keep job history',
            subtitle: 'Remember which tools you used and when, on this '
                'device only.',
            trailing: Switch(
              value: value.enabled,
              onChanged: notifier.setEnabled,
            ),
            onTap: () => notifier.setEnabled(!value.enabled),
          ),
          ListRow(
            title: 'Include file names',
            subtitle: 'Show which file each job used. Turning this off also '
                'removes names already saved.',
            trailing: Switch(
              value: value.fileNames,
              onChanged: value.enabled ? notifier.setFileNames : null,
            ),
            onTap: value.enabled
                ? () => notifier.setFileNames(!value.fileNames)
                : null,
          ),
          ListRow(
            title: 'Clear history',
            subtitle: switch (value.count) {
              0 => 'No entries',
              1 => '1 entry',
              final count => '$count entries',
            },
            onTap: value.count == 0
                ? null
                : () => _confirmClear(context, notifier),
          ),
        ]),
      AsyncError() => const GroupedList(children: [
          ListRow(
            title: 'Job history',
            subtitle: 'History settings are unavailable right now.',
          ),
        ]),
      _ => const GroupedList(children: [
          ListRow(title: 'Job history', subtitle: 'Loading…'),
        ]),
    };
  }

  static Future<void> _confirmClear(
    BuildContext context,
    HistoryNotifier notifier,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Clear job history?'),
        content: const Text(
          'This removes every entry from this device for good.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('Clear'),
          ),
        ],
      ),
    );
    if (confirmed != true) {
      return;
    }
    await notifier.clear();
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Job history cleared')),
      );
    }
  }
}

class _LocalOnlyCard extends StatelessWidget {
  const _LocalOnlyCard();

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
        padding: const EdgeInsets.all(Space.lg),
        child: MergeSemantics(
          child: Row(
            children: [
              DecoratedBox(
                decoration: BoxDecoration(
                  color: palette.brandFill,
                  borderRadius: BorderRadius.circular(Radii.control),
                ),
                child: Padding(
                  padding: const EdgeInsets.all(Space.md),
                  child: Icon(
                    Icons.verified_user_outlined,
                    color: palette.onBrand,
                    size: 28,
                  ),
                ),
              ),
              const SizedBox(width: Space.lg),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Local-only processing', style: text.titleMedium),
                    const SizedBox(height: Space.xs),
                    Text(
                      'PaperWarden does not upload your documents.',
                      style: text.bodySmall
                          ?.copyWith(color: palette.textSecondary),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
