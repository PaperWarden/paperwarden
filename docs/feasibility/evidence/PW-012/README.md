# PW-012 evidence

Produced on 2026-11-28 by `core/crates/qpdf/tests/qpdf.rs`
(`every_corpus_pdf_meets_its_outcome_through_a_qpdf_job`, with
`PAPERWARDEN_EVIDENCE` set), with qpdf 12.4.2, zlib 1.3.2 and IJG libjpeg 9f
built from the sources `scripts/fetch_qpdf.py` verifies.

- `windows-x64.jsonl`: Windows 11, MSVC, debug build.
- `android-x64-emulator.jsonl`: the `PaperWarden_API_35` emulator (Android 15,
  x86_64), debug build, test binary run with `adb shell`.

One line per corpus PDF:

- `job`: the result of a `Rewrite` job through `JobRunner` (pre-check, qpdf,
  verification of the output): `rewritten`, `rewritten, flagged` (the
  pre-check found active content, which qpdf keeps), `repaired`, or why the
  job stopped.
- `qpdf_alone`: qpdf rewriting the file with no pre-check, to show what qpdf
  itself does with hostile input.
- `check`: `qpdf --check` on the input.
- `job_ms`, `qpdf_ms`: wall-clock times.

The iOS simulator results come from CI (`qpdf-ios` in
`.github/workflows/ios.yml`, artifact `PW-012-ios-evidence`).

To reproduce, see "Reproducing" in `../../PW-012-qpdf.md`.
