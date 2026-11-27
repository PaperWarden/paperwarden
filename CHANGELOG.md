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
