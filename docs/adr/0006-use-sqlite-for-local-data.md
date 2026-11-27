# ADR-0006: Use SQLite with contentless FTS5 for local data

**Status:** Accepted  
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
- **Automatic cloud backup stays off.** Moving to a new phone will instead be
  a manual export and import of settings and workflows, which the user starts
  and whose file the user keeps. That feature is still to be built. Job history
  and the index are not exported.
- **Job history keeps file names**, for usability. The user can:
  - turn history off entirely, so nothing new is recorded;
  - hide file names, so new entries record no display name;
  - clear existing history, which purges it from every database file.

## Consequences

- About 1.8 MB of SQLite code per ABI before linker garbage collection.
- No search snippets, because the index does not keep document text.
- Settings and history do not move to a new phone through OS backup; until
  export and import exist, a new phone starts with default settings.
- History with file names on is more useful and less private; the controls
  above put that choice with the user.
- Revisit encryption (for example SQLCipher) only if the threat model adds
  protection of an unlocked device.
