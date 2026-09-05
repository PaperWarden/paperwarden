# ADR-0001: Use Flutter for the mobile UI

**Status:** Accepted  
**Date:** 2026-09-06  
**Deciders:** PaperWarden maintainers

## Context

PaperWarden needs one responsive Android/iOS UI while native file, camera, and
sharing integrations remain available.

## Decision

Use Flutter/Dart for the application UI and narrow Kotlin/Swift adapters for
platform integration. Riverpod remains the intended state-management candidate,
to be added after the workspace spike validates the application boundary.

## Consequences

- Product UI and accessibility work can be shared across platforms.
- Native adapters and platform builds still require first-class testing.
- Document parsing remains outside Dart UI code.

