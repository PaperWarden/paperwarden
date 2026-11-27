# PW-010 evidence

Produced on 2026-11-27 by `core/crates/pdf/examples/pdf_probe.rs` on the
`PaperWarden_API_35` emulator (Android 15, x86_64), with PDFium `chromium/8125`
(`android-x64`, SHA-256 `bd68707f29f2316c20f9d5c793b8731796a4a65408ac4d3c30bc606390354e41`).
Each corpus PDF ran in its own process through `JobRunner`, rendering every page
into a 1080 x 1920 box.

- `android-x64-emulator.jsonl`: one line per fixture: outcome, pages, pages
  rendered, open and render time, peak resident memory (`VmHWM`) and first-page
  text.
- `*.png`: the exported first page of each valid fixture and of the JavaScript
  fixture, reduced to half size. They show what PDFium drew.
- `pdfium-diagnostics.tsv`: what PDFium itself reports for each corpus PDF
  (`FPDF_DocumentHasValidCrossReferenceTable`, trailer count, document
  JavaScript count, link and page actions), produced on the same emulator for
  the mismatch investigation. `pdfium-diagnostics.rs.txt` is the throwaway tool
  that produced it. It calls PDFium's raw bindings, which PaperWarden's own
  code may not (`unsafe` is forbidden), so it lives outside the workspace and is
  kept here only as source.

To reproduce, see "Results on Android" in `../../PW-010-pdfium-android.md`.
