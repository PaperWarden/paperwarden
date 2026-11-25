import 'package:flutter/material.dart';

import '../app_info.dart';
import '../theme/palette.dart';
import '../theme/theme_controller.dart';
import '../theme/tokens.dart';
import '../widgets/grouped_list.dart';
import '../widgets/page_scaffold.dart';
import '../widgets/section_heading.dart';
import 'privacy_policy_screen.dart';

/// Settings tab.
///
/// Only the theme, version and privacy policy work today. App Lock, export
/// and storage settings are "Coming soon" until those features exist; Clear
/// cache will call the core's workspace recovery once the bridge (#12) lands.
class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = ThemeScope.of(context);
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
        const SectionHeading('Appearance'),
        GroupedList(children: [
          ListRow(
            title: 'Theme',
            value: theme.value.label,
            onTap: () => _chooseTheme(context, theme),
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
    ThemeController theme,
  ) {
    return showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (sheetContext) => SafeArea(
        child: RadioGroup<ThemeMode>(
          groupValue: theme.value,
          onChanged: (mode) {
            if (mode != null) theme.value = mode;
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
