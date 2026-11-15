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

The repository pins Rust 1.98.0 and Flutter 3.47.2. Android and iOS host
projects are committed; the first engineering work remains the M0 feasibility
milestone in `ROADMAP.md`.

```sh
cd core
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

```sh
cd apps/mobile
flutter pub get
dart format --output=none --set-exit-if-changed .
flutter analyze
flutter test
flutter build apk --debug
```

Run `python scripts/check_invariants.py`, `python scripts/check_licenses.py`, and
`python scripts/check_corpus.py` before opening a pull request that changes
guardrails, dependencies, or fixtures.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Report vulnerabilities privately using
the process in [SECURITY.md](SECURITY.md).
