# Privacy

PaperWarden is designed so that core document work does not require trust in a
PaperWarden-operated service.

## Commitments

- Documents and derived content are processed locally.
- There are no accounts, ads, analytics, telemetry, or hosted processing services.
- Originals are preserved unless the user explicitly chooses to replace or delete
  a file through the operating system.
- Temporary workspaces are isolated and removed after every terminal job state.
- Local indexing is opt-in and can be cleared.
- Sanitization exports a new copy and provides a before/after report.

## Diagnostics

PaperWarden does not automatically upload crash reports or logs. Any future
user-initiated diagnostic export must be reviewed as a new privacy surface.

## Scope changes

Any proposal involving network access, remote processing, accounts, analytics, or
diagnostic upload requires a public ADR and an explicit update to the product
invariants before implementation.

