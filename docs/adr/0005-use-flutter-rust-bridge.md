# ADR-0005: Use flutter_rust_bridge with Cargokit for the FFI bridge

**Status:** Proposed  
**Date:** 2026-11-26  
**Deciders:** PaperWarden maintainers

## Context

ADR-0002 exposes the Rust core to Flutter "through a generated FFI bridge
after the feasibility spike". `ARCHITECTURE.md` lists flutter_rust_bridge as
the candidate. The spike (#12, `docs/feasibility/PW-009-flutter-rust-bridge.md`)
proved sync and async calls, cancellation, typed errors, reproducible
generation and Android builds; iOS builds are proven in CI.

## Decision

- Generate the bridge with flutter_rust_bridge 2.13.0, pinned exactly in
  `core/crates/bridge/Cargo.toml`, `apps/mobile/pubspec.yaml` and the `Bridge`
  workflow. Upgrade all three together and regenerate.
- The bridge is the workspace member `core/crates/bridge`, so `cargo deny`,
  `cargo audit` and the license inventory cover its dependencies.
- Build it into the app with Cargokit (`apps/mobile/rust_builder`), from
  source, with prebuilt-binary download disabled.
- Keep the API in `core/crates/bridge/src/api` narrow. Long-running calls run
  on the bridge's worker pool and take a cancellation handle.
- CI fails if committed bindings differ from a fresh generation, if the APK
  lacks the library for any ABI, or if the iOS app lacks the bridge symbols.

## Consequences

- Building the app requires a Rust toolchain, the Android NDK and Rust mobile
  targets.
- Cargokit's build tool resolves its own Dart packages at build time without a
  lockfile (finding 2 in the report); release reproducibility work must
  address it.
- The bridge crate holds the only `unsafe` in the Rust workspace, confined to
  generated code.
