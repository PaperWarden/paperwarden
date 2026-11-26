# ADR-0006: Use SQLite with contentless FTS5 for local data

**Status:** Proposed  
**Date:** 2026-11-27  
**Deciders:** PaperWarden maintainers

## Context

`ARCHITECTURE.md` lists "SQLite + FTS" as the candidate for settings, job
history, workflows and an optional local index, and `PRIVACY.md` requires the
index to be opt-in and clearable. The PW-017 spike
(`docs/feasibility/PW-017-sqlite-fts.md`) proved this on Windows and Android and
in CI for iOS.

## Decision

- The Rust core owns one SQLite database through `paperwarden-store`, using
  `rusqlite` with SQLite compiled from source (`bundled`). Flutter reaches it
  only through the bridge.
- The search index is FTS5, contentless with deletes: it stores search terms,
  never document text. It is off until the user turns it on, and clearing it
  purges the terms from every database file.
- Schema changes are forward-only migrations tracked in `user_version`; a
  database from a newer version is refused and left unchanged.
- The database is not encrypted by PaperWarden. It lives in app-private
  storage, relies on Android file-based encryption and iOS Data Protection,
  and is excluded from backups and device transfer.

## Consequences

- About 1.8 MB of SQLite code per ABI before linker garbage collection.
- No search snippets, because the index does not keep document text.
- Settings and history do not move to a new phone through OS backup.
- Revisit encryption (for example SQLCipher) only if the threat model adds
  protection of an unlocked device.
