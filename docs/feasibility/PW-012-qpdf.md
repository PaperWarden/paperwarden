# PW-012: qpdf operations and packaging

**Issue:** #15  
**Date:** 2026-11-28  
**Result:** Feasible. qpdf 12.4.2, built from pinned upstream source, performs
every structural transform and encryption flow tested, with identical results
on Windows and on the Android emulator; on the iOS simulator in CI (see "iOS").
It handles every malformed and hostile corpus file without crashing, printing
or hanging. Like PDFium it parses hostile input in native code, so the same
containment question applies (finding 1).  
**Decision record:** none yet; adopting qpdf needs an ADR (follow-ups).  
**Evidence:** `docs/feasibility/evidence/PW-012/`

## What was tested

- **Sources:** qpdf 12.4.2, zlib 1.3.2 and IJG libjpeg 9f, from their upstream
  release archives. `scripts/fetch_qpdf.py` downloads over HTTPS only and
  checks SHA-256 digests pinned in the script:
  - qpdf against the release's own `qpdf-12.4.2.sha256` and GitHub's record;
  - zlib against GitHub's record for the release asset;
  - libjpeg, for which IJG publishes no checksum, pinned on first download.

  The `qpdf` / `qpdf-sys` crates on crates.io were rejected. They vendor
  copies from a single maintainer, including a zlib development snapshot
  ("1.3.1.1, January xxth 2024"), and need `bindgen` and libclang to build
  for Android or iOS.
- **Build:** `core/crates/qpdf/build.rs` compiles the three with `cc` (C++20),
  writing the one configuration header qpdf's CMake would. Only qpdf's
  **native crypto** is built: no OpenSSL or GnuTLS. On Android, libc++ is
  linked statically, so the app needs no `libc++_shared.so`. Without the
  fetched sources, the crate builds without qpdf and every operation reports
  `Unavailable`; CI's default jobs stay network-free.
- **Binding:** no generated binding. `src/ffi.rs`, the crate's only `unsafe`
  code, declares the 20 C functions used (QPDFJob, logger, document, global
  limits), each call with a SAFETY note. qpdf's C API catches every C++
  exception, so nothing unwinds into Rust.
- **Operations** (`Transform`), each a QPDFJob JSON description built and
  escaped in Rust:
  - rewrite (and repair);
  - rotate pages;
  - extract a page range;
  - merge further PDFs;
  - encrypt with AES-256 (revision 6, owner password required);
  - decrypt with a password.
- **Job runner:** `QpdfProcessor` is both processor and verifier.
  - **Before qpdf:** the PDF pre-check runs, then qpdf counts the input's
    pages.
  - **After qpdf:** the output must open in qpdf with the expected page count,
    and an unencrypted output must pass the pre-check before it can be
    exported.
- **Devices:**
  - Windows 11 (MSVC, x86_64);
  - the `PaperWarden_API_35` emulator (Android 15, x86_64): test binary run
    with `adb shell`;
  - CI: Linux, Android cross-builds for arm64, arm and x86_64, and the iOS
    simulator.

## Results

`cargo test -p paperwarden-qpdf -- --include-ignored` gives **5 unit and 5
qpdf tests passed** on Windows and on the Android emulator:

| Test | What it shows |
| --- | --- |
| Limits in effect | each of PaperWarden's 10 limits reads back from qpdf |
| Corpus through a job | every corpus PDF meets its outcome (table below); no workspace left |
| Pages | pages 10 to 12 of 200 extracted; 3 pages rotated, each with `/Rotate 90`; 1 + 2 pages merged into 3; a range past the end refused before qpdf writes anything |
| Encryption | AES-256 revision 6 (`/R 6`, `/AESV3`); opens with either password, not with none or a wrong one; a wrong password fails the decrypt job; decrypting restores a 2-page PDF that passes the pre-check; the password never appears in the output |
| Malformed input | empty, not-a-PDF, header-only, 64 KiB of noise, and 16 truncations of a valid file: each a typed error or a recovered PDF that opens, in under 5 s, never a crash |

Corpus (`windows-x64.jsonl`, `android-x64-emulator.jsonl`; identical on both):

| Outcome | Fixtures | Job result |
| --- | --- | --- |
| accept | 5 | rewritten, no notices |
| repair | 5 | rewritten with the "repaired" notice |
| neutralize | 2 | rewritten and flagged (finding 3) |
| bound | 3 | deep nesting and the flate bomb refused by the pre-check; the extreme page size rewritten (qpdf does not render) |
| reject | 2 | refused: "the PDF has no content" |

The whole corpus takes under 1.1 s in debug builds. The slowest job is
`large-image` at about 0.4 s, almost all of it the pre-check decoding the
image.

**Size:** an arm64 Android release binary calling every operation is
**2.82 MB larger stripped (1.13 MB gzip -9)** with qpdf than the same probe
built without it (3,172,712 against 352,024 bytes). That includes the
statically linked libc++. CI prints the same measurement for every Android
ABI and for iOS devices.

## iOS

The `qpdf-ios` CI job builds the probe for iOS devices and checks two things:
the size, and that it imports no network API. It then runs the five qpdf tests
on an iOS simulator, writing `ios-simulator-arm64.jsonl` to the
`PW-012-ios-evidence` artifact. Its results are in the PR, since this machine
has no macOS.

## Findings

1. **qpdf parses hostile input in native code.** It is a mature, fuzzed
   (OSS-Fuzz) library, but it has no time limit or overall memory cap, and its
   defaults leave Flate, DCT, PNG, TIFF and run-length decoding **unbounded**.
   - **Done here:** PaperWarden sets explicit limits once per process (64 MiB
     per decode, nesting 32, 1,000,000 elements per container, 8 filters per
     stream, 1,000 warnings). These match the pre-check's limits, and qpdf
     refuses to run if they cannot be set. The pre-check also runs first.
   - **Recommendation:** on Android, run qpdf in the isolated render process
     from #59 (renamed for both engines), for the same reasons as PDFium. On
     iOS, the pre-check and these limits are the containment.
2. **qpdf would print.** Its C API writes unhandled errors to the default
   logger, including file paths ("application did not handle error: <path>:
   invalid password"); the first test run caught it.
   - **Fixed:** every job gets a logger that discards, the default logger is
     silenced once, and errors are always collected.
3. **Structure tools keep active content.** qpdf copies `/OpenAction`
   JavaScript and launch actions into its output: it never runs them, but
   rewriting does not remove them. The pre-check's notices travel with the
   result. Removing active content would be a separate "sanitize" operation.
4. **Passwords.**
   - They travel in the JSON job description, which is cleared after use
     (best effort; qpdf's own copies are its own).
   - NUL bytes are refused.
   - Escaping is tested, so a password cannot add keys to the job.
   - A password-protected input cannot be pre-checked until it is decrypted,
     so for `Decrypt` the pre-check runs on the output, before export. #67's
     pre-check work could take the password instead.
5. **Merging needs more than one staged input.** The job runner stages one
   input, so merge works through `transform` but is refused by
   `QpdfProcessor`. Merging in a job needs the runner to stage several inputs.
6. **No cancellation inside qpdf.** QPDFJob runs to completion; cancellation
   and the time limit take effect before and after it. Isolation (finding 1)
   would allow stopping it.

## Follow-ups

- An ADR to adopt qpdf, deciding containment on Android (finding 1).
- Run qpdf in the Android isolated process (with #59) and package it with the
  app's notices (`THIRD_PARTY_LICENSES.md`).
- Multi-input jobs for merge (finding 5), and a pre-check that takes a
  password (finding 4).
- A qpdf fuzz target through `transform`, and measuring on an arm64 phone.

## Reproducing

```sh
python scripts/fetch_qpdf.py              # or --archives DIR for local copies
cd core
cargo test -p paperwarden-qpdf -- --include-ignored
```

For Android, build the tests with the NDK's clang and clang++
(`--target x86_64-linux-android --no-run`). Then push the test binary and
`tests/corpus/` with adb, and run the binary with `PAPERWARDEN_CORPUS` and
`TMPDIR` set to the pushed paths.
