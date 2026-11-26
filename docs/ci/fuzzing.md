# Fuzzing

Fuzz targets live in `/fuzz` (cargo-fuzz, libFuzzer). Each one checks
properties of the result, not only that the code does not crash, so a target
fails when a rule is broken, not just on a panic.

## Targets

| Target | Code under test | Properties checked |
| --- | --- | --- |
| `job_id` | `JobId::new` | Accepted IDs are non-empty ASCII alphanumerics and hyphens, unchanged, and form exactly one plain path component inside a workspace name. |
| `archive_entry_path` | `archive::EntryPath::parse` | Accepted names respect every length and depth bound; each component is a plain name with no separator, `:`, control character, `.`/`..` or trailing dot or space; joined to a directory the path stays inside it on the platform's own path rules; re-parsing the canonical form gives the same result. |
| `magic_bytes` | `validation::MagicBytes` | Never panics on any header or offset, including offsets near `usize::MAX`, and accepts exactly the inputs that carry the signature. |
| `job_pipeline` | `JobRunner`, staging, limits, verification, export, cleanup | For any input file and any stub-engine behaviour (copy, no output, oversized output, engine error, cancellation, wrong output): invalid input never becomes exportable, export refuses to overwrite the original, an exported file holds exactly the verified output, the original is never modified, and no workspace remains afterwards. |

The targets were checked against planted bugs: removing the backslash rule from
`EntryPath` made `archive_entry_path` fail within about 1,500 runs without
seeds, and making export overwrite existing files made `job_pipeline` fail on
its seeds.

## Not covered yet

The issue also names metadata, document containers and PDF boundaries. No code
parses those yet; their fuzz targets are added with the engines that parse them
(PDFium, qpdf, the archive engine). A target with nothing real to call would
only give false assurance.

Other limits:

- `job_pipeline` uses a stub engine, so it tests the lifecycle around engines,
  not an engine itself. It never panics on purpose, because libFuzzer aborts on
  any panic; panic recovery is covered by `core/tests/job_runner.rs`.
- Fuzzing finds bugs within the time it runs. A clean run is evidence, not
  proof.

## Running

CI (`.github/workflows/fuzz.yml`) runs every target from its seeds, for 60
seconds on pull requests that touch the core or `/fuzz` and for 10 minutes
weekly, with a 2 GB memory limit and a 10-second per-input timeout. A crashing
input is uploaded as a workflow artifact.

Locally, with the pinned nightly toolchain and `cargo install cargo-fuzz
--version 0.13.2 --locked`, run from `core/`:

```sh
cargo +nightly-2026-08-26 fuzz run --fuzz-dir ../fuzz archive_entry_path \
  ../fuzz/corpus/archive_entry_path ../fuzz/seeds/archive_entry_path -- -max_total_time=60
```

On Windows, put the MSVC AddressSanitizer runtime
(`clang_rt.asan_dynamic-x86_64.dll`, under
`VC\Tools\MSVC\<version>\bin\Hostx64\x64`) on `PATH`. Windows ends the process
on a panic before libFuzzer can save the input, so reproduce crashes on Linux
or from the CI artifact.

## When a target fails

1. Download the `fuzz-crash-<target>` artifact and reproduce it with
   `cargo fuzz run --fuzz-dir ../fuzz <target> <crash-file>`.
2. Fix the code, never the property (`PRODUCT_INVARIANTS.md` 14).
3. Add the input to `fuzz/seeds/<target>/` and a unit test for it, so the case
   stays covered.
