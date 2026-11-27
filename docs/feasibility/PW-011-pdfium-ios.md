# PW-011: PDFium rendering on iOS

**Issue:** #14  
**Date:** 2026-11-27  
**Result:** Feasible. The same PDFium release and wrapper that PW-010 proved on
Android render every valid corpus PDF on an iOS simulator and treat every
hostile file exactly as on Android. The device framework is a self-contained,
signable arm64 framework that needs iOS 17. iOS cannot isolate PDFium in
another process, so the Rust pre-check in ADR-0007 is its containment.  
**Decision record:** ADR-0007 (Accepted 2026-11-27)  
**Evidence:** `docs/feasibility/evidence/PW-011/`

## What was tested

- **PDFium build:** `bblanchon/pdfium-binaries` release `chromium/8125`, the
  same as PW-010.
  - **Assets:** `ios-device-arm64` for packaging and `ios-simulator-arm64` for
    running.
  - **No scripts or forms:** both are non-V8, non-XFA builds.
    `scripts/fetch_pdfium.py` verifies each asset's pinned SHA-256 and
    refuses builds with V8 or XFA. It now fetches over HTTPS only.
- **Where:** GitHub's `macos-latest` runner (macOS 26.7.2), job `pdfium-ios` in
  `.github/workflows/ios.yml`, on an iPhone 17 Pro simulator. No Mac or iPhone
  was available locally, so CI is the iOS environment.
- **Code:** unchanged from PW-010. `paperwarden-pdf` (through `pdfium-render`
  0.9.6) is built for `aarch64-apple-ios-sim`. The tests and `pdf_probe` run
  inside the simulator through `xcrun simctl spawn`. The crate and probe also
  build for `aarch64-apple-ios` (devices).
- **Memory:** `pdf_probe` reports peak resident memory on Apple platforms
  through `getrusage` (`ru_maxrss`), using `nix` as a development-only
  dependency.

## Results on the iOS simulator

All five PDFium tests pass (`ios-simulator-tests.txt`):
- rendering and text;
- multiple pages and incremental updates;
- the render bound;
- hostile files finishing in under 10 s;
- a full job through `JobRunner` with no workspace left behind.

`pdf_probe` ran every corpus PDF in its own process, rendering every page into
a 1080 x 1920 box (`ios-simulator-arm64.jsonl`).

| Fixture | Corpus outcome | PDFium result | Pages / rendered | Render (all pages) | Peak RSS | Same as Android |
| --- | --- | --- | --- | --- | --- | --- |
| valid/pdf/one-page | accept | rendered | 1 / 1 | 0.8 ms | 39.5 MiB | yes |
| valid/pdf/two-pages-flate | accept | rendered | 2 / 2 | 1.2 ms | 41.1 MiB | yes |
| valid/pdf/incremental-update | accept | rendered, newest revision | 1 / 1 | 0.8 ms | 39.5 MiB | yes |
| valid/pdf/many-pages-200 | accept | rendered | 200 / 200 | 117.5 ms | 41.5 MiB | yes |
| valid/pdf/large-image (4000 x 4000 RGB) | accept | rendered | 1 / 1 | 79.1 ms | 97.1 MiB | yes |
| malformed/pdf/header-only | reject | refused | – | – | 31.3 MiB | yes |
| malformed/truncated | reject | refused | – | – | 31.3 MiB | yes |
| malformed/pdf/bad-xref-offsets | repair | repaired and rendered, no warning yet | 1 / 1 | 1.6 ms | 39.6 MiB | yes |
| malformed/pdf/no-eof-marker | repair | repaired and rendered, no warning yet | 1 / 1 | 0.9 ms | 39.5 MiB | yes |
| security/pdf/stream-length-overflow | repair | repaired and rendered, no warning yet | 1 / 1 | 0.7 ms | 39.4 MiB | yes |
| security/pdf/xref-prev-loop | repair | repaired and rendered, no warning yet | 1 / 1 | 1.7 ms | 39.5 MiB | yes |
| security/pdf/javascript-openaction | neutralize | rendered, script never run, not flagged | 1 / 1 | 0.8 ms | 39.5 MiB | yes |
| security/pdf/launch-and-uri-actions | neutralize | rendered, actions never run, not flagged | 1 / 1 | 1.6 ms | 39.5 MiB | yes |
| security/pdf/deep-nesting | bound | handled inside PDFium, rendered | 1 / 1 | 0.8 ms | 39.6 MiB | yes |
| security/pdf/flate-bomb (16 MiB) | bound | decoded in full and rendered | 1 / 1 | 23.8 ms | 64.2 MiB | yes |
| security/pdf/extreme-page-size | bound | page refused, no large allocation | 2 / 0 | – | 36.3 MiB | yes |

- **Identical behaviour:** every fixture's outcome, page counts and
  first-page text match Android exactly. The PW-010 mismatch investigation and
  its conclusions therefore hold on iOS unchanged.
- **Contract score:** against the runner contract, 8 of 16 pass on both
  platforms. The other eight wait on the pre-check (#60) and on the warnings
  (#61).
- **Opening:** about 0.1 ms per file. The first probe process took 3.0 ms.
- **Memory:** the simulator's baseline is higher than Android's. Each process
  starts at about 31 MiB with nothing open, which is the simulator runtime,
  and reaches about 40 MiB with PDFium bound. The increments are what count:
  - about 58 MiB for the 48 MB image page (Android: 61 MiB);
  - about 25 MiB for the Flate bomb (Android: 29 MiB).
- **Not phone numbers:** the simulator runs on the Mac's CPU and memory, and
  iOS enforces its memory limit on a different measure (`phys_footprint`) from
  `ru_maxrss`. Measure on an iPhone before setting budgets.

## Size

| Item | Bytes |
| --- | --- |
| `pdfium.framework/pdfium`, arm64 device, uncompressed | 7,021,200 |
| Same, `gzip -9` (approximates App Store download growth) | 3,219,521 |

- **Format:** one arm64 slice, the only architecture needed for iOS 17
  devices.
- **Exports:** it exports 433 `FPDF*` functions.
- **Linked libraries:** Foundation, CoreFoundation, CoreGraphics and
  `libSystem` only. There is no C++ runtime to ship, and CI fails if it ever
  links anything else.
- **No network:** it imports no network API, also enforced in CI.

## Packaging and updates

- **Fetch, do not commit**, as on Android. CI fetches both iOS assets through
  `fetch_pdfium.py`.
- **XCFramework:** the device and simulator frameworks combine into one
  `pdfium.xcframework` with `xcodebuild -create-xcframework`, which CI does on
  every run.
- **Embed and sign (follow-up):** the app embeds it in `Runner` as "Embed &
  Sign". Xcode signs it with the app, so no unsigned or downloaded code ever
  runs; App Store review allows bundled, signed frameworks. CI proves the
  framework accepts a signature: an ad hoc `codesign` verifies.
- **Loading:** `pdfium-render` binds with `dlopen` at run time, the same as on
  Android. The iOS adapter passes only the path inside the app bundle
  (`Bundle.main.privateFrameworksPath` + `/pdfium.framework/pdfium`) to the
  core. That is a thin adapter; Swift does no document processing.
- **Minimum iOS:** the framework's `LC_BUILD_VERSION` minimum is iOS 17.0,
  above the app's previous target of 15.0. The app now targets iOS 17.0
  (maintainer decision, 2026-11-27). The `pdfium-ios` job fails if the app's
  target ever drops below the framework's minimum.
- **Updates:** the same process as Android (PW-010). Change `RELEASE` and every
  digest in one reviewed change, verify provenance, run the PDFium tests on
  Linux, Android and the iOS simulator, re-run the corpus probe, and
  re-measure size.

## Findings

1. **iOS has no process isolation for PDFium.** Apps cannot spawn helper
   processes. A hostile PDF that exhausts memory inside PDFium makes iOS end
   the whole app. *Decided (ADR-0007):* the Rust pre-check (#60) bounds every
   PDF by decoding before PDFium sees it. The remaining risk within those
   bounds is accepted, mitigated by fuzzing, PDFium's OSS-Fuzz coverage and
   prompt updates.
2. **The same gaps as Android.** Repairs are silent, active content is
   unflagged, and nesting and bombs are handled only inside PDFium. The same
   work closes them on both platforms: the pre-check (#60) and the warnings
   (#61).
3. **iOS 17 minimum.** The prebuilt framework requires iOS 17.0. The app now
   targets it, which drops devices older than iPhone XS and XR. Building
   PDFium ourselves for a lower target was the alternative and was not taken.
4. **Privacy manifest required.** The framework imports `stat` and `fstat`,
   which Apple lists as file-timestamp required-reason APIs. The app has no
   `PrivacyInfo.xcprivacy` yet (#65).
5. **The fetch script had a Semgrep finding.** It could in principle have been
   given a `file://` URL. It now accepts only the pinned release URL and uses
   an opener with HTTPS handlers only, covered by offline tests
   (`tests/scripts/test_fetch_pdfium.py`).

## Follow-ups

- **Package the framework in the app:** Embed & Sign, the adapter passes the
  bundle path, and then measure on an iPhone using `phys_footprint`.
- **Containment and warnings:** the pre-check (#60) and the warnings (#61).
- **Privacy manifest** (#65).
- **`pdfium-render` 0.9.7** to drop `chrono` (#62).
