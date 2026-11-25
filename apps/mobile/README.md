# PaperWarden mobile shell

Flutter owns PaperWarden's cross-platform UI. Android and iOS host projects are
committed so native configuration and permission changes remain reviewable.

The production Android manifest intentionally has no `INTERNET` permission.
Platform packaging decisions for the Rust bridge, PDFium, qpdf, Tesseract, and
OpenCV remain gated by the M0 feasibility issues.

## Structure

- `lib/theme/` holds the design system. `palette.dart` defines the light and
  dark color roles taken from the approved mockups (`docs/design/`), and
  `test/theme_test.dart` fails if any text pair drops below WCAG AA contrast.
  `tokens.dart` holds spacing and radii.
- `lib/shell/` is the four-tab navigation shell. Tabs stay mounted, so
  switching keeps their state; `ShellScope.select` switches tabs from anywhere.
- `lib/widgets/` holds the shared building blocks: page scaffold, section
  heading, grouped list rows, hero card, trust footer and file-type badge.
- `lib/screens/` holds one widget per tab, plus the offline privacy policy.
- `assets/legal/PRIVACY.md` is a bundled copy of the repository's `PRIVACY.md`;
  a test fails if they differ, so edit both together.
- `lib/app_info.dart` holds the version shown in Settings; a test keeps it equal
  to `pubspec.yaml`.

Unbuilt capabilities use `ListRow(comingSoon: true)`: visible, labeled and not
tappable, so the UI never advertises a feature before its engine passes
feasibility (`PRODUCT_INVARIANTS.md` 12). The app uses the platform font;
bundling a font needs its own license review, and downloading one at runtime
would break the offline-only rule.
