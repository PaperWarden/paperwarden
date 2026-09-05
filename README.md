# PaperWarden

PaperWarden is a free, open-source, offline-first document viewer, converter,
scanner, OCR, and privacy toolbox for Android and iOS.

## Product promise

- No account, ads, subscription, analytics, or telemetry.
- No PaperWarden backend or required Internet connection.
- Documents are processed on-device and are never uploaded by PaperWarden.
- Original files are never silently overwritten.
- Untrusted documents are always treated as hostile input.

## Status

PaperWarden is in **Phase 0: Constitution and Technical Feasibility**. The first
release is viewer-and-converter-first; full Office editing is explicitly deferred
until after 1.0.

## Architecture

The monorepo contains:

- `apps/mobile`: Flutter/Dart application shell.
- `core`: Rust processing core and job lifecycle.
- `native/android` and `native/ios`: platform integration boundaries.
- `tests/corpus`: small, reviewed compatibility and hostile-input fixtures.
- `docs/adr`: architecture decisions.

Read [PRODUCT_INVARIANTS.md](PRODUCT_INVARIANTS.md),
[ARCHITECTURE.md](ARCHITECTURE.md), and [ROADMAP.md](ROADMAP.md) before changing
product behavior.

## Development

This seed intentionally contains only a minimal Flutter shell and Rust job model.
The first engineering work is the feasibility milestone in `ROADMAP.md`.

```sh
cd core
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Flutter platform projects will be generated and reviewed in the dedicated
workspace-bootstrap issue before Android or iOS builds are considered complete.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security reports must follow the private
reporting instructions that will be published in `SECURITY.md` before accepting
outside contributions.

