# Contributing

PaperWarden handles hostile files. Small, reviewable, evidence-backed changes are
preferred over broad rewrites.

## Before coding

1. Read the issue, `PRODUCT_INVARIANTS.md`, `ARCHITECTURE.md`, and relevant ADRs.
2. For architecture or dependency changes, obtain maintainer agreement first.
3. Create a focused branch such as `feat/123-pdf-outline` or
   `security/142-block-path-traversal`.

## Pull requests

- Link the issue and complete every applicable template section.
- Add tests and corpus fixtures for new or corrected behavior.
- Never include private, copyrighted, malicious-without-documentation, or oversized
  real-user documents in the corpus.
- Do not weaken validation, limits, or security checks to make a sample pass.
- Obtain review from the other maintainer and wait for required CI.

## Generated code

Generated FFI bindings must be reproducible. A pull request changing an interface
must update and verify the generated artifacts in the same change.

