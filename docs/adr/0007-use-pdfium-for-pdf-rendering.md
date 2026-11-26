# ADR-0007: Use PDFium for PDF rendering

**Status:** Proposed  
**Date:** 2026-11-27  
**Deciders:** PaperWarden maintainers

## Context

`ARCHITECTURE.md` lists PDFium as the candidate PDF renderer. The PW-010 spike
(`docs/feasibility/PW-010-pdfium-android.md`) rendered every valid corpus PDF on
Android and handled every hostile one without a crash or hang. It also showed
that PDFium, inside the app process, cannot be held to PaperWarden's own
decompression and memory bounds.

## Decision

- Render PDFs with PDFium from `bblanchon/pdfium-binaries`, **non-V8, non-XFA**
  builds only, pinned by release and per-asset SHA-256 in
  `scripts/fetch_pdfium.py`. Provenance is verified on every update; the
  libraries are fetched at build time, never committed.
- Reach PDFium only through `paperwarden-pdf` and the job runner's engine seam.
  Every render is sized by PaperWarden within a pixel cap.
- **To decide before shipping:** run PDFium in a separate, memory-limited
  Android service process (preferably isolated) that is killed on timeout.
- Replace `pdfium-render` with a minimal in-house binding before shipping,
  because of its unmaintained `chrono` dependency.

## Consequences

- About 6.6 MB uncompressed (3.3 MB compressed) per Android ABI.
- PDFium security updates must be taken at least monthly and after Chrome
  releases with PDFium fixes.
- Damaged PDFs that PDFium can repair need an explicit product policy
  (repair and warn, or reject).
- iOS cannot isolate PDFium in another process; that residual risk is decided
  in PW-011.
