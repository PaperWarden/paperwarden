# Corpus manifest, checks and runner boundaries

The corpus in `tests/corpus` is the evidence `SUPPORTED_FORMATS.md` and
`PRODUCT_INVARIANTS.md` 12 rely on. This page defines what a fixture records,
what CI proves about it today, and what an engine runner must do before a
fixture counts as passing.

## Manifest schema 2

`tests/corpus/manifest.json` lists every file in the corpus. Each entry has
exactly these fields (`references` is optional); unknown fields fail the check.

| Field | Meaning |
| --- | --- |
| `path` | Relative POSIX path. No `..`, absolute paths, backslashes, colons, control or text-direction characters. |
| `sha256`, `bytes` | Hash and size of the file. |
| `license` | SPDX identifier cleared for redistribution: CC0-1.0, CC-BY-4.0, MIT, Apache-2.0, BSD-2-Clause or BSD-3-Clause. |
| `provenance` | Where the file came from. Generated fixtures name `scripts/generate_corpus.py`. |
| `category` | `valid`, `malformed`, `security`, `conversion` or `ocr`; must equal the first directory. |
| `format` | `pdf`, `office`, `archive`, `image`, `text`, `html` or `svg`; under `valid`, `malformed` and `security` it must equal the second directory. |
| `media_type` | The type the file claims to be. |
| `outcome` | What a correct engine does with it; see below. |
| `requires` | Capabilities an engine needs to run it: `pdf`, `office`, `archive`, `image`, `text`, `html`, `svg`, `ocr`, `conversion`. Empty only for `reference` fixtures. |
| `purpose`, `expected` | Why the fixture exists and, in words, what must happen. |
| `facts` | Structural properties of the bytes, checked by CI (below). |
| `references` | Ground-truth fixtures for this input, such as OCR text or conversion expectations. |

### Outcomes

| Outcome | A runner passes the fixture when the engine |
| --- | --- |
| `accept` | opens or processes it and produces what `expected` describes. |
| `reject` | refuses it as invalid, produces no output and leaves no workspace. |
| `neutralize` | never performs the dangerous effect (executing, fetching, escaping the workspace, overwriting) and flags it; it may still show safe content. |
| `bound` | stops at a size, ratio, nesting, dimension or loop bound with a limit error, without exhausting memory, time or disk, and leaves no workspace. |
| `reference` | not an input; ground truth for another fixture. |

### Limits

At most 1 MiB per fixture and 8 MiB for the whole corpus. Bombs stay small on
disk; what they expand to is a fact, not a file size.

## What CI proves today

`.github/workflows/corpus.yml` runs on every pull request:

1. `tests/scripts/test_check_corpus.py` proves the checker works. For every
   declared fact it substitutes a false value and expects a rejection, and it
   breaks each schema rule in a copy of the corpus.
2. `scripts/check_corpus.py` checks every entry against the rules above,
   rejects files missing from the manifest, and verifies each fact with
   `scripts/corpus_facts.py`. That module is independent of the generator,
   uses only the standard library, never parses XML that declares a DTD, and
   caps decompression at 64 MiB.

Fact kinds: `signature`, `contains`, `absent`, `utf8`, `text`, `csv_shape`,
`pdf_xref_valid`, `pdf_text`, `pdf_prev_loop`, `max_inflated_bytes`,
`max_bracket_depth`, `zip_valid`, `zip_entries`, `zip_utf8_flags`,
`zip_symlinks`, `zip_uncompressed_bytes`, `zip_ratio_at_least`,
`zip_nesting_depth`, `zip_first_stored`, `zip_text_contains`, `xml_doctype`,
`xml_wellformed`, `png_crc_valid`, `png_complete` and `image_size`.

This proves each fixture is intact, licensed and is what it claims to be, for
example that the zip bomb inflates to 16 MiB and the traversal archive holds
exactly `../../paperwarden-escape.txt`. It does **not** prove any engine
handles it. With no engine yet, every input is reported as `pending`.

## Runner boundaries

Engine runners arrive with the engines (PW-010 to PW-016). Each runner:

- Runs only fixtures whose `requires` it implements and reports the rest as
  `pending`. A pending fixture is never evidence of support.
- Runs every fixture through `JobRunner` (ADR-0004): a staged copy in an
  isolated workspace, the job's size and time limits, cancellation, and
  cleanup checked afterwards. It never opens the fixture in place and never
  uses the network.
- Gives each fixture a time limit (30 seconds by default) and treats a timeout,
  a crash or a leftover workspace as a failure, except that a `bound` fixture
  passes when the engine stops with a limit error.
- Records `pass` or `fail` per fixture and platform in the corpus report,
  alongside the existing integrity and fact results.

A format or conversion pair may move beyond `Research` in
`SUPPORTED_FORMATS.md` only when every fixture that requires it passes on both
Android and iOS.

## Artifact retention

| Artifact | Produced by | Kept |
| --- | --- | --- |
| `corpus-report` (JSON, per fixture: outcome, integrity, facts verified, engine status) | every corpus run | 14 days |
| Engine outputs and logs of failing fixtures | engine runners, on failure only | 14 days |
| `fuzz-crash-<target>` | fuzzing, on failure | 14 days |

Artifacts hold only synthetic fixtures and what engines derive from them. CI
never processes user documents, so artifacts never contain user data. Nothing
a run produces is committed. Results cited as release evidence are copied into
the release record before they expire (`RELEASE_CHECKLIST.md`).

## Adding or changing a fixture

1. Add it to `scripts/generate_corpus.py`, with its facts in `DECLARED`, or add
   a reviewed third-party file under a cleared license with its provenance.
2. Run `python scripts/generate_corpus.py`, then `python
   scripts/check_corpus.py`, and review the diff of the fixture and the
   manifest.
3. Security fixtures must stay inert: no working exploit, remote references
   only to `.invalid`, and bombs bounded so a mistaken open cannot exhaust a
   developer machine.
