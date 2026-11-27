# PW-011 evidence

Produced on 2026-11-26 by the `pdfium-ios` job in `.github/workflows/ios.yml`
(run 45895604215, commit 8f23158), on GitHub's `macos-latest` runner (macOS
26.7.2) with an iPhone 17 Pro simulator. PDFium `chromium/8125`: assets
`ios-device-arm64` (SHA-256
`a9ab2295c65005d6fe81b11a172645160a5194ae55d1ffdf8444dd27124aad23`) and
`ios-simulator-arm64` (SHA-256
`b2b5f0c918f661b9fe8de7dba3d3fee7d7df61db3247cfb0f3f910077af54f1b`), both
fetched and verified by `scripts/fetch_pdfium.py`.

- `ios-packaging.txt`: the device framework's architecture, minimum iOS
  version, linked libraries, size (raw and `gzip -9`), exported PDFium
  functions, and an ad hoc signature verified by `codesign`.
- `ios-simulator-tests.txt`: the five PDFium tests from
  `core/crates/pdf/tests/render.rs`, run on the simulator.
- `ios-simulator-arm64.jsonl`: `pdf_probe` over every corpus PDF on the
  simulator, one process per file, with peak memory from `getrusage`.
- `simulator.txt`: the booted simulator and the host macOS version.

The job uploads the same files plus the rendered first pages as the
`PW-011-ios-evidence` artifact (kept 14 days). Simulator timings and memory
come from a Mac, not an iPhone.
