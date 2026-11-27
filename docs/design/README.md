# Approved UI mockups

The approved visual direction for the four main tabs. The design system in
`apps/mobile/lib/theme/` and the screens in `apps/mobile/lib/screens/` are
built against these.

| Tab | Mockup | Issue |
| --- | --- | --- |
| Home | [home.jpg](home.jpg) | #42 |
| Files | [files.jpg](files.jpg) | #43 |
| Tools | [tools.jpg](tools.jpg) | #42 |
| Settings | [settings.jpg](settings.jpg) | #45 |

The shared shell, palette and type scale come from #44.

## Agreed deviations

The mockups set the direction; the product invariants decide where the app
departs from them.

- **No notification bell** until it has a purpose that works entirely on the
  device. Push notifications would need a network (`PRODUCT_INVARIANTS.md` 1).
- **Platform font.** A bundled font needs its own license review, and
  downloading one at runtime breaks the offline rule.
- **Generic file-type badges** (`PDF`, `DOC`, `XLS`, `IMG`) instead of
  third-party product logos.
- **Capabilities still in feasibility** are shown as "Coming soon" and cannot
  be opened (`PRODUCT_INVARIANTS.md` 12).
- **Sample content** in the mockups (file names, sizes, folders, version) is
  illustrative. The app shows only real data on the device.
