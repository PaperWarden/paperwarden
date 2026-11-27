# PDF pre-check

**Issue:** #60 (PW-036)  
**Decision record:** ADR-0007  
**Code:** `core/crates/pdf-scan` (`paperwarden-pdf-scan`)  
**Evidence:** `docs/security/evidence/pdf-precheck/`

## Why it exists

PW-010 and PW-011 showed that PDFium, inside the app, decodes and parses
without PaperWarden's bounds, and that its API reports only 3 of the 8 hostile
or damaged corpus files it handles differently:

- **Bounds:** it inflated a 16 MiB Flate bomb in full (its only cap is
  1 GiB), and it silently truncated nesting deeper than 64.
- **Damage:** it silently corrected a false `/Length`.
- **Active content:** it did not count `/OpenAction` JavaScript or Launch
  actions.

iOS cannot isolate PDFium in another process. So before PDFium opens a file,
on every platform, PaperWarden inspects the PDF itself.

## What it does

The pre-check runs in `RenderProcessor` on the job's staged copy, after the
job runner has staged and size-checked the input and before `PdfEngine::open`.

1. **Finds every object without trusting the file's index.** It scans for
   `N G obj` headers, as PDFium does when it rebuilds a damaged file, and
   parses each object. A header inside an earlier object's stream data is
   ignored.
   - Object nesting is capped, so the parser's recursion is bounded.
   - At most 65,536 container elements are kept per object; the rest are parsed
     and dropped, so a huge array cannot exhaust memory.
   - Names are decoded (`/Java#53cript` is `/JavaScript`), and string contents
     are skipped, never kept.
2. **Checks the cross-reference chain** from `startxref` through every `/Prev`,
   for classic tables and cross-reference streams. Each in-use entry must
   point at its own object header. The number of sections is capped, and a
   cycle is damage.
3. **Decrypts encrypted documents in memory, only to measure them** (#67).
   The standard security handler is opened with the **empty user password**:
   - **Revisions:** 2 to 4 (RC4 40 to 128-bit, AES-128) and 5 to 6
     (AES-256), with crypt filters.
   - **The password is checked first:** algorithms 4 and 5 for revisions 2 to
     4, algorithm 2.B for revision 6. A key is never used on trust.
   - **Streams the handler leaves alone stay alone:** cross-reference streams,
     `/Identity` crypt filters, and metadata when `/EncryptMetadata` is false.
   - **Each stream is then decrypted and measured** under exactly the same
     bounds as an unencrypted one. Object streams are decrypted before their
     objects are checked. Decrypted bytes never leave the scan.

   A document that needs a real password is refused with
   `ScanError::PasswordRequired`. One that uses another handler (certificates,
   custom handlers), an unknown revision or a malformed encryption dictionary
   is refused with `ScanError::Encrypted`. The encryption dictionary is found
   through the newest trailer or cross-reference stream. A standard handler
   dictionary that no trailer names is still refused, never ignored.

   Strings up to 128 bytes are now kept (decoded from literal or hex form),
   because the keys and file ID live in strings. Longer strings are still
   skipped, never stored.
4. **Measures every stream by decoding it.** Each stream is decoded into a
   counter that discards the output (Flate, LZW, ASCIIHex, ASCII85,
   RunLength), against these bounds:
   - **images:** the data their own `/Width`, `/Height`, components and bits
     per component allow, plus a pixel cap;
   - **other streams:** a size cap and a compression-ratio cap;
   - **the whole document:** a total cap.

   JPEG and JPEG 2000 images are not decoded. Their real dimensions are read
   from the codestream, so a small `/Width` cannot hide a 65,535 × 65,535
   JPEG. Object streams are decoded and their objects checked like any other.
5. **Reports what it found:**
   - Damage (the corpus `repair` outcome): a cross-reference table that is
     wrong or missing, a missing `%%EOF`, a `/Prev` cycle, a wrong `/Length`,
     and corrupt or truncated streams.
   - Active content (the `neutralize` outcome): JavaScript, Launch, URI,
     SubmitForm, ImportData, remote go-to actions, embedded files, rich media
     and XFA.

   `RenderStats::precheck` carries both. The processor also records them as
   the job's notices (`Notice::Repaired`, `Notice::NotRun(..)`), which reach
   the app as typed values through the bridge's `checkPdf` (#61).
6. **Stops cooperatively.** The job's checkpoint runs every 256 objects, for
   every stream and every MiB decoded, so cancellation and the time limit
   apply.

A bound becomes `JobError::LimitExceeded` with a reason that contains no
document content. A refused file becomes `JobError::EngineFailed`.

## Limits

The defaults are **provisional**: they are checked against the corpus and the
real-world set below, and should be revisited with a larger real-world corpus.

| Limit | Default | Why |
| --- | --- | --- |
| Nesting | 32 | PDFium truncates beyond 64. Real documents nest a few levels, using references for depth. |
| Objects | 1,000,000 | The largest real-world file here has 11,672. |
| Cross-reference sections | 64 | One per incremental save. |
| Non-image stream, decoded | 64 MiB | Content streams of large drawings run to tens of MiB. |
| Compression ratio (non-image) | 256:1, after 1 MiB | Real content compresses about 3 to 50 times. The 16 MiB bomb is 1024:1. |
| Document total, decoded | 1 GiB | Bounds CPU time. JPEG and JPEG 2000 data is not decoded, so scanned books stay far below it. |
| Image pixels | 40 million | An A4 page at 600 dpi is 35 million. |
| Image data | its dimensions, plus one predictor byte per row and 64 KiB | Data beyond that is what an image bomb adds. |

File size is never the measure. The corpus's own large-image scan expands
48 times, so a ratio alone could not separate it from a bomb; images are
bounded by their dimensions instead.

## Results

### Corpus (Android 15 emulator, PDFium `chromium/8125`)

`android-x64-emulator.jsonl` has the full results. **All 16 corpus PDFs now
meet their outcome at the engine level.** That was 8 of 16 with PDFium alone.

| Fixture | Outcome | Result | Pre-check | Peak RSS |
| --- | --- | --- | --- | --- |
| valid: one-page, two-pages-flate, incremental-update, many-pages-200 | accept | rendered, nothing found | 0.3 to 11.3 ms | 12.9 to 14.9 MiB |
| valid/pdf/large-image (48 MB decoded) | accept | rendered, nothing found | 63.9 ms | 74.5 MiB |
| malformed: header-only, truncated | reject | refused: no content | 0.0 ms | 5.9 MiB |
| malformed/pdf/bad-xref-offsets | repair | rendered, damage: cross-reference | 0.2 ms | 12.9 MiB |
| malformed/pdf/no-eof-marker | repair | rendered, damage: missing `%%EOF` | 0.3 ms | 12.8 MiB |
| security/pdf/stream-length-overflow | repair | rendered, damage: stream length | 0.5 ms | 12.8 MiB |
| security/pdf/xref-prev-loop | repair | rendered, damage: `/Prev` cycle | 0.2 ms | 13.0 MiB |
| security/pdf/javascript-openaction | neutralize | rendered, flagged: JavaScript | 0.4 ms | 12.9 MiB |
| security/pdf/launch-and-uri-actions | neutralize | rendered, flagged: Launch, URI | 0.6 ms | 12.9 MiB |
| security/pdf/deep-nesting | bound | **limit: nesting**, PDFium never opened it | 0.0 ms | 5.9 MiB |
| security/pdf/flate-bomb | bound | **limit: compression ratio**, PDFium never opened it | 0.0 ms | 6.1 MiB (was 41.5) |
| security/pdf/extreme-page-size | bound | the page refused by the render box | 0.9 ms | 9.7 MiB |

The user still sees no warning for `repair` and `neutralize` files; that is
#61.

### Encrypted PDFs (Android 15 emulator, PDFium `chromium/8125`)

`encrypted-android-x64.jsonl` has the results. The six fixtures were encrypted
by pypdf 6.19.0, which is independent of PaperWarden's code
(`scripts/make_encrypted_fixtures.py`). All six give the same answer from the
pre-check and from PDFium.

| Fixture | Encryption | Outcome | Result | Pre-check |
| --- | --- | --- | --- | --- |
| valid/pdf/encrypted-rc4-128 | RC4-128, R3 | accept | rendered, "PaperWarden corpus" | 0.5 ms |
| valid/pdf/encrypted-aes-128 | AES-128, R4 | accept | rendered | 1.1 ms |
| valid/pdf/encrypted-aes-256 | AES-256, R6 | accept | rendered | 5.3 ms |
| security/pdf/encrypted-flate-bomb | AES-128, R4 | bound | **limit: compression ratio**, PDFium never opened it | 0.0 ms |
| security/pdf/encrypted-javascript | AES-256, R6 | neutralize | rendered, flagged: JavaScript | 3.5 ms |
| security/pdf/encrypted-needs-password | AES-256, R6 | reject | **refused: needs a password** | 0.0 ms |

Revision 6 is the slowest because its password hash is deliberately costly.
It is computed twice per document, never per stream.

### Real-world PDFs (no false positives)

`real-world.tsv` has 35 PDFs from 15 producers:
- the PDF manuals shipped with software installed on the development PC:
  pdfTeX, Ghostscript, xdvipdfmx, Apache FOP and Figma;
- four generated locally: Microsoft Edge printing, Matplotlib, ReportLab, and
  a Pillow multi-page 300 dpi scan.

19 of them use object streams and cross-reference streams.

- **No false damage or refusals:** all 35 are allowed with no damage.
- **URIs are found:** the 16 with web links are flagged URI, which is correct,
  since links are `neutralize` content.
- **Small in every measure:** the largest has 11,672 objects, the most
  decoded is 2.9 MB, and the slowest scan took 29.8 ms (Windows, release
  build).

This set is modest and mostly TeX output. Office suites, scanners, CAD tools
and print drivers are not represented yet.

## Tests

- **Unit tests:** 16, covering the parser, filters (including the LZW example
  from ISO 32000-1), the predictor and JPEG headers.
- **`tests/corpus.rs`:** every corpus PDF meets its expectation, every corpus
  PDF has one, and the bomb stops near the ratio cap.
- **`tests/hostile.rs`:** 16 attacks beyond the corpus:
  - an image bomb, and a JPEG larger than it declares;
  - declared-huge images, the stream and total caps;
  - active content and nesting hidden in object streams, obfuscated names,
    every active-content kind;
  - encryption, a wrong offset in a table or a cross-reference stream;
  - long revision chains, the object cap, and random garbage.
- **Mutation checks:** removing name decoding, object-stream reading, the
  ratio check or cycle detection each makes at least one test fail.
- **PDFium tests:** the PDF crate's tests, on the emulator with PDFium. A bomb
  through `JobRunner` stops with `LimitExceeded` before PDFium opens it, and a
  repairable file renders with its damage recorded.
- **Fuzzing:** the `pdf_scan` target, with default and tight limits, checks
  determinism, limits and checkpoints. Locally it ran 25,301 runs in 120 s
  with no failure; it runs in `fuzz.yml` on every pull request.

## Not covered yet

- **Password-protected PDFs** are refused until password entry is decided:
  it needs a user interface, and the password must never be stored.
- **Inline images** (`BI ... ID ... EI`) inside content streams are not
  bounded by their dimensions. Their compressed data counts toward the content
  stream, but not their expansion.
- **Sampled functions, Type 3 fonts, shadings and font programs** are bounded
  only as streams, not by what PDFium builds from them.
- **JBIG2 and CCITT images** are bounded by their declared dimensions only.
  Their codestreams are not read.
- **The limits need a larger real-world corpus** before they are final.
