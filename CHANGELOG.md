# Changelog

All notable changes will be documented here after the first release baseline.

## Unreleased

- Established the Phase 0 repository constitution and feasibility scaffold.
- Configured the Rust core as a Cargo workspace with a shared lint policy that
  forbids unsafe code, panics, network sockets and subprocesses in production
  code, plus documented module boundaries and test conventions.
- Added the mobile design system (light and dark palettes checked against WCAG AA)
  and the four-tab navigation shell from the approved mockups.
- Built the Home and Tools tabs. The Files shortcut works; Scan, Convert and every
  tool are marked "Coming soon" and cannot be opened until their engines pass
  feasibility.
- Built the Files tab layout (search, filters, folders and recent files) with
  empty states; it reads no files until the library model is decided.
- Built the Settings tab: a System, Light or Dark theme choice that applies
  immediately, the app version, and the privacy policy bundled for offline
  reading. Security, export and storage settings are marked "Coming soon".
- Adopted Riverpod (`flutter_riverpod` 3.5.0, MIT) for app state.
- Prototyped the Flutter-Rust bridge (flutter_rust_bridge 2.13.0 with
  Cargokit): sync and async calls, cancellation and typed errors, proven on an
  Android emulator. CI now checks generated bindings, the Android APK's Rust
  libraries and the iOS link. The license check now covers every locked
  version of a package.
- Added the isolated processing-job skeleton: content-based validation,
  bounded processors with cancellation and time limits, output verification,
  export that never overwrites, guaranteed cleanup and crash recovery.
- Added property-checking fuzz targets for job IDs, archive entry paths, the
  content validator and the whole job pipeline, plus archive entry-path rules
  that block traversal, absolute paths, backslashes, colons, control characters
  and text-direction spoofing, and decode legacy CP437 names. Fixed the scheduled fuzz workflow, which had never
  run successfully, and added a short fuzz run to pull requests.
- Proved local storage on SQLite with FTS5 (`paperwarden-store`): settings, job
  history, workflows, an opt-in contentless search index that clears without
  residue, and forward-only migrations. Tests pass on an Android emulator.
  Android backup and device transfer of app data are now off.
