# Product Invariants

These invariants are release-blocking. Changing one requires an approved ADR and
explicit maintainer agreement.

1. No required PaperWarden backend, account, cloud storage, or Internet access.
2. No paid API or cloud AI is required for core functionality.
3. No ads, subscriptions, analytics, or telemetry.
4. PaperWarden never uploads user documents, filenames, extracted text, or metadata.
5. Android must not request `android.permission.INTERNET` for 1.0.
6. VBA, PDF JavaScript, and other document-provided active content are never executed.
7. Remote document resources are never fetched automatically.
8. Original documents are not silently overwritten; operations produce a new output.
9. Every job uses an isolated temporary workspace and cleans it after success,
   failure, cancellation, and stale-workspace recovery.
10. Inputs are validated using structure and magic bytes, not filenames alone.
11. Archive extraction blocks traversal, absolute paths, unsafe links, excessive
    nesting, and decompression bombs.
12. A format or conversion pair is not advertised until its compatibility corpus
    passes and the output can be verified.
13. Redaction is complete only when underlying content is no longer searchable or
    extractable.
14. Security checks are fixed, not removed or weakened to make CI pass.

