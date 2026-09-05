# Corpus

Keep small, redistributable, reviewed fixtures grouped by behavior rather than by
where they came from.

- `valid`: minimal valid files and edge cases.
- `malformed`: truncated, inconsistent, or otherwise invalid structures.
- `security`: inert fixtures for traversal, active-content detection, size limits,
  metadata, and redaction regressions.
- `conversion`: source/output expectation pairs.
- `ocr`, `images`, `pdf`, `office`, `archives`: focused capability sets.

Never include real private documents, live credentials, functional malware, or
unreviewed third-party copyrighted material. Each fixture needs provenance,
license, purpose, and an expected result.

