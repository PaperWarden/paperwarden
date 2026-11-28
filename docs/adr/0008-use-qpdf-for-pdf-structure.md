# ADR-0008: Use qpdf for PDF structure tools

**Status:** Accepted  
**Date:** 2026-11-28  
**Deciders:** PaperWarden maintainers

## Context

`ARCHITECTURE.md` lists qpdf as the candidate engine for PDF structure tools:
merge, split, rotate, encrypt and decrypt. The PW-012 spike
(`docs/feasibility/PW-012-qpdf.md`) built qpdf 12.4.2 from pinned upstream
source. It performed every transform and encryption flow tested, with
identical results on Windows, the Android emulator and the iOS simulator. It
handled every malformed and hostile corpus file without a crash, a hang or any
output to the console. It also showed:

- qpdf parses hostile input in native code. Its defaults leave Flate, DCT, PNG,
  TIFF and run-length decoding memory unbounded, and it has no time limit or
  overall memory cap.
- Rewriting keeps active content (JavaScript, launch actions); qpdf never runs
  it.
- The Rust bindings on crates.io vendor unverifiable copies of qpdf, zlib and
  libjpeg and need libclang to build for mobile.
- It adds about 2.8 MB (1.1 MB compressed) to an arm64 Android build.

## Decision

Use qpdf for PDF structure tools, on these terms:

- **Source:** qpdf, zlib and IJG libjpeg are built from their upstream release
  archives, checked against the SHA-256 digests pinned in
  `scripts/fetch_qpdf.py`, with qpdf's native crypto only. No vendored copy
  and no generated binding: `core/crates/qpdf/src/ffi.rs` declares the C
  functions used and is the crate's only `unsafe` code.
- **Bounds, on every platform:** PaperWarden's PDF pre-check runs before qpdf.
  qpdf's own limits are set once per process to match it (`LIMITS`), and qpdf
  refuses to run if they cannot be set. Every output is reopened by qpdf and
  pre-checked before it can be exported.
- **Containment on Android:** like PDFium (ADR-0007), qpdf runs only in the
  isolated, memory-limited service process from #59, which is killed on
  timeout. This is required before qpdf ships on Android. On iOS, which offers
  apps no equivalent, the pre-check and qpdf's limits are the containment.
- **Silence:** qpdf's loggers discard everything; errors reach PaperWarden only
  as return values.
- **Passwords** pass only through memory, are cleared after use (best effort)
  and are never logged, stored or written to an output.
- **Active content:** structure tools keep what the document contains and pass
  on the pre-check's notices. Removing active content is a separate "sanitize"
  operation, still to be decided.
- **Updates:** qpdf, zlib and libjpeg updates are taken with security releases,
  each as one reviewed change to `scripts/fetch_qpdf.py`.

## Consequences

- About 2.8 MB more per Android ABI, and a similar amount on iOS.
- Builds need the fetched sources; without them the crate builds without qpdf
  and every operation reports `Unavailable`, so CI's default jobs stay offline.
- iOS builds must set `IPHONEOS_DEPLOYMENT_TARGET` (Xcode does for the app).
- The app must ship qpdf's `LICENSE.txt` and `NOTICE.md`, the zlib license and
  the IJG statement (`THIRD_PARTY_LICENSES.md`).
- Merging inside a job needs the job runner to stage several inputs, and
  cancellation cannot interrupt qpdf until it runs in its own process.
