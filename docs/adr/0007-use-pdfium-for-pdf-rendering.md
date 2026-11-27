# ADR-0007: Use PDFium for PDF rendering

**Status:** Accepted  
**Date:** 2026-11-27  
**Deciders:** PaperWarden maintainers

## Context

`ARCHITECTURE.md` lists PDFium as the candidate PDF renderer. The PW-010 spike
(`docs/feasibility/PW-010-pdfium-android.md`) rendered every valid corpus PDF on
Android and handled every hostile one without a crash or hang. It also showed:

- PDFium, inside the app process, cannot be held to PaperWarden's own
  decompression and memory bounds. Its only Flate output cap is 1 GiB.
- PDFium's API reports only three of the eight hostile or damaged corpus files
  it handled differently from the corpus. It silently corrects a false
  `/Length`, silently truncates deep nesting, inflates a bomb in full, and
  does not count `/OpenAction` JavaScript or Launch actions.

## Decision

### Source and supply chain

- Render PDFs with PDFium from `bblanchon/pdfium-binaries`, **non-V8, non-XFA**
  builds only, pinned by release and per-asset SHA-256 in
  `scripts/fetch_pdfium.py`, which fetches over HTTPS only.
- Verify provenance on every update. The libraries are fetched at build time,
  never committed.
- Reach PDFium only through `paperwarden-pdf` and the job runner's engine seam.
  PaperWarden sizes every render within a pixel cap.

### Containment

- **Android:** PDFium runs in a separate, memory-limited service process
  (preferably `android:isolatedProcess`) that is killed on timeout, so a
  hostile PDF cannot take the app down. This is required before PDFium ships on
  Android.
- **Every platform, and the only containment on iOS:** before PDFium opens a
  file, a Rust pre-check in the core inspects the PDF structure itself.
  - **Size is measured by decoding, never by file size.** Compressed streams
    can be deceptively small. Every stream is decoded in a streaming way,
    counting output and discarding it, against:
    - a per-stream cap;
    - a per-document total cap;
    - for images, the size the image's own `/Width`, `/Height`, components
      and bits per component imply.
  - **Structure bounds:** object nesting is capped below PDFium's own 64, and
    object count and `/Prev` chain length are capped. `/Prev` cycles are
    detected.
  - **Honest lengths:** `/Length` is checked against where the stream really
    ends.
  - **Active content is flagged, never run:** JavaScript, including in
    `/OpenAction` and additional actions, Launch, URI, SubmitForm, embedded
    files and rich media.
  - **Damage is detected:** bad cross-reference offsets, a missing trailer
    end, a `/Prev` cycle and a false `/Length`.

  A file that exceeds a bound is stopped with a limit error. The implementing
  issue sets the exact limits against the corpus and real documents.
- **iOS residual risk (accepted):** iOS gives apps no separate process.
  Memory exhaustion inside PDFium, within the pre-check's bounds, ends the
  whole app. This is mitigated by the pre-check, PaperWarden's fuzzing,
  PDFium's continuous OSS-Fuzz coverage and prompt updates.

### Damaged files

- A recoverable file opens with a damaged-file warning when it passes the
  pre-check and PDFium opens it. Otherwise it is rejected.
- The corpus outcome for this is `repair`.

### Binding

- **Keep `pdfium-render` for now** and move to 0.9.7 when it ships. Upstream
  (ajrcarey/pdfium-render#289) gates all date functionality, and with it
  `chrono`, behind an optional feature. PaperWarden already disables default
  features, so 0.9.7 removes `chrono` here.
- **Until then**, `cargo audit`'s unmaintained warning for `chrono`
  (RUSTSEC-2026-0543, informational, no vulnerability) is accepted and tracked.
- **If upstream stalls** before PDFium ships, write a minimal binding of our
  own instead: about 20 functions, loaded with `libloading`, with its `unsafe`
  confined to one crate and tested like the bridge. That carries its own
  maintenance cost.
- **Alternatives evaluated on 2026-11-27 and not adopted:**
  - `pdfium` (GPL-3.0, incompatible with the project licence);
  - the `kreuzberg-` and `xberg-pdfium-render` forks, which still depend on
    `chrono` or are only weeks old;
  - `firecrawl-pdfium`, which is MIT or Apache-2.0 and depends only on
    `libloading`, but has a single 0.1.0 release (August 2026). It is worth
    re-checking if upstream stalls.

### Platforms

- The iOS minimum is **iOS 17.0**, the minimum of the PDFium iOS build
  (PW-011).

## Consequences

- About 6.6 MB uncompressed (3.3 MB compressed) per Android ABI, and about
  7.0 MB uncompressed (3.2 MB compressed) on iOS.
- Every PDF is decoded twice: once by the pre-check, which counts and discards
  the output, and once by PDFium.
- PDFium security updates must be taken at least monthly and after Chrome
  releases with PDFium fixes.
- Devices that cannot run iOS 17 (older than iPhone XS and XR) are not
  supported.
