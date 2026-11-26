# PW-010: PDFium rendering on Android

**Issue:** #13  
**Date:** 2026-11-27  
**Result:** Feasible, with one architectural condition. PDFium renders every valid
corpus PDF and handles every hostile one without crashing or hanging. It
cannot be held to PaperWarden's own decompression and memory bounds while it
runs inside the app process (finding 1).  
**Decision record:** ADR-0007 (Proposed)  
**Evidence:** `docs/feasibility/evidence/PW-010/`

## What was tested

- **PDFium build:** `bblanchon/pdfium-binaries` release `chromium/8125` (PDFium
  158.0.8125.0, published 2026-10-26), **non-V8** assets. `args.gn` confirms
  `pdf_enable_v8 = false` and `pdf_enable_xfa = false`: the library contains no
  JavaScript engine and no XFA forms, so PDF JavaScript cannot run (invariant
  6). `scripts/fetch_pdfium.py` refuses any build without both settings.
- **Supply chain:** each asset's SHA-256 matches the digest GitHub records for
  the release. Build provenance is verified with `gh attestation verify`: built
  by `bblanchon/pdfium-binaries/.github/workflows/build-all.yml` on `master`.
- **Rust:** `core/crates/pdf` (`paperwarden-pdf`) wraps PDFium through
  `pdfium-render` 0.9.6, loading the library at run time. It exposes open,
  page count and size, text, and render. `RenderProcessor` plugs it into the
  job runner's engine seam (ADR-0004): staged copy, isolated workspace,
  cancellation and time checks between pages, verified PPM output.
- **Render bound:** every render is sized by PaperWarden inside a caller's box,
  capped at 4096 x 4096 pixels (64 MiB RGBA). A page's declared size never
  decides the allocation.
- **Device:** `PaperWarden_API_35` emulator, Android 15, x86_64, on the
  development PC.

## Results on Android

All five PDFium tests pass on the emulator: rendering and text, multi-page and
incremental updates, the render bound, hostile files finishing in under 10 s,
and a full job through `JobRunner`.

`pdf_probe` ran every corpus PDF in its own process, rendering every page into
a 1080 x 1920 box (`android-x64-emulator.jsonl`).

| Fixture | Corpus outcome | PDFium result | Open | Render (all pages) | Peak RSS |
| --- | --- | --- | --- | --- | --- |
| valid/pdf/one-page | accept | rendered | 4.4 ms | 3.9 ms | 12.6 MiB |
| valid/pdf/two-pages-flate | accept | rendered | 4.3 ms | 5.8 ms | 14.1 MiB |
| valid/pdf/incremental-update | accept | rendered, newest revision | 6.1 ms | 4.2 ms | 12.5 MiB |
| valid/pdf/many-pages-200 | accept | rendered, 200 pages | 4.2 ms | 543.5 ms | 14.3 MiB |
| valid/pdf/large-image (4000 x 4000 RGB) | accept | rendered | 4.5 ms | 195.1 ms | 73.7 MiB |
| malformed/pdf/header-only | reject | refused | – | – | 8.1 MiB |
| malformed/truncated | reject | refused | – | – | 8.1 MiB |
| malformed/pdf/bad-xref-offsets | reject | **repaired and rendered** | 65.1 ms | 7.2 ms | 12.3 MiB |
| malformed/pdf/no-eof-marker | reject | **repaired and rendered** | 4.7 ms | 3.4 ms | 12.6 MiB |
| security/pdf/stream-length-overflow | reject | **repaired and rendered** | 4.9 ms | 3.7 ms | 12.8 MiB |
| security/pdf/javascript-openaction | neutralize | rendered, script never run, **not flagged** | 4.3 ms | 4.4 ms | 12.6 MiB |
| security/pdf/launch-and-uri-actions | neutralize | rendered, actions never run, **not flagged** | 5.9 ms | 5.3 ms | 12.8 MiB |
| security/pdf/xref-prev-loop | bound | **handled inside PDFium**, rendered | 6.6 ms | 6.3 ms | 12.8 MiB |
| security/pdf/deep-nesting (20,000 levels) | bound | **handled inside PDFium**, rendered | 6.7 ms | 5.4 ms | 12.7 MiB |
| security/pdf/flate-bomb (16 MiB) | bound | **decoded in full** and rendered | 4.3 ms | 71.4 ms | 41.5 MiB |
| security/pdf/extreme-page-size | bound | page refused, no large allocation | 3.9 ms | – | 9.5 MiB |

**Against the runner contract (`docs/ci/corpus.md`), 8 of 16 pass:** the 5
valid files, both refused malformed files, and the extreme page size. The
other 8 were handled **safely** (no crash, hang, runaway memory, script
execution or fetch), but not in the way the corpus states: PDFium repairs
rather than rejects, handles loops and nesting internally rather than stopping
at a PaperWarden bound, and the wrapper does not yet flag active content.
These are findings, not passes. The fixtures were not relabelled to fit.

Rendered first pages of the valid files are kept as PNG thumbnails beside the
results. Timings come from an x86_64 emulator on a desktop CPU and are not phone
numbers; measure on arm64 hardware before setting budgets.

## Size

| Item | Bytes |
| --- | --- |
| `libpdfium.so`, arm64-v8a, uncompressed | 6,598,376 |
| Same, deflate level 6 (approximates Play download per ABI) | 3,309,482 |
| `libpdfium.so`, x86_64 | 6,886,848 |

Android Gradle stores native libraries uncompressed by default for minSdk 23 or
later, so an APK grows by about 6.6 MB per ABI it carries; ship ABI-split App
Bundles. Each library's LOAD segments are aligned to 16 KB (`0x4000`), as
Google Play requires for Android 15 targets. Its only dependencies are
`libc.so`, `libm.so` and `libdl.so`, with no `libc++_shared.so` to ship.

## Packaging and updates

- **Fetch, do not commit.** `scripts/fetch_pdfium.py <platform>` downloads one
  pinned asset, checks its SHA-256 against the digest in the script, refuses
  builds with V8 or XFA, and extracts only the library and license files to
  `third_party/pdfium/<platform>/`, which git ignores. CI fetches the Linux
  build and runs the PDFium tests (`rust.yml`, job `pdfium`).
- **Android integration (follow-up).** Fetch the four ABIs into the app's
  `jniLibs` at build time and load `libpdfium.so` by name from the app's
  native library directory. This spike loaded it by path from a test binary.
- **Updates.** PDFium ships security fixes with Chromium. Update at least
  monthly, and promptly after a Chrome release lists PDFium fixes:
  1. Change `RELEASE` and every digest in `fetch_pdfium.py` in one reviewed
     change, after `gh attestation verify` on each asset.
  2. Run the PDFium tests, the corpus probe on a device, and fuzzing.
  3. Re-measure size.

  Never update automatically.

## Findings

1. **In-process PDFium cannot be bounded by PaperWarden.** The job runner's
   time limit is checked between pages, and the render box caps bitmaps, but
   decompression, parsing depth and memory inside one PDFium call are
   PDFium's own. The 16 MiB Flate bomb was decoded in full, peaking at
   41.5 MiB. A larger bomb or a parser loop inside one page could exhaust
   memory or stall a page render. **Recommendation:** on Android, run PDFium in
   a separate service process (ideally `android:isolatedProcess`) with a
   memory ceiling, killed on timeout, so a hostile PDF cannot take the app
   down. iOS offers no equivalent for apps (PW-011); there, rely on PDFium's
   continuous OSS-Fuzz coverage plus our own fuzzing, and record that as an
   accepted risk.
2. **Repair or reject.** PDFium repairs broken cross-reference tables and lying
   stream lengths, which users generally want when viewing. The corpus says
   `reject`. Maintainers should decide: either add a `repair` outcome
   ("open, render, and tell the user the file was damaged") or reject files
   PDFium had to repair. PDFium can report this through
   `FPDF_DocumentHasValidCrossReferenceTable`.
3. **Active content is ignored but not reported.** The non-V8 build cannot run
   scripts, and PaperWarden never invokes actions, but `neutralize` also
   requires flagging. Detect document JavaScript and Launch/URI actions and
   tell the user.
4. **`pdfium-render` brings an unmaintained crate.** It depends on `chrono`
   (RUSTSEC-2026-0543, unmaintained, no fix). `cargo deny` did not report this
   because it is transitive; `cargo audit` did. It also brings about 25 crates
   for the roughly 20 PDFium functions PaperWarden needs. **Recommendation:**
   replace it with a small binding of our own, loaded with `libloading`, with
   its `unsafe` confined to one crate as for the bridge.
5. **One binding per process.** PDFium can be bound once, and `pdfium-render`'s
   thread-safe mode serialises calls. That is fine for a viewer; parallel
   thumbnailing would need more processes, which fits finding 1.
6. **Licensing is permissive.** See `THIRD_PARTY_LICENSES.md`; the app must ship
   the notices, including the FreeType and IJG credits.

## Follow-ups

- Decide finding 1 (process isolation) in ADR-0007 before PDFium ships.
- Decide the `repair` outcome (finding 2) and implement active-content flags
  (finding 3).
- Replace `pdfium-render` with a minimal binding (finding 4).
- Package the libraries in the app and measure on an arm64 phone.
- Add a PDF fuzz target through the wrapper.
- PW-011: the same for iOS (`ios-device-arm64` asset, pinned here).
