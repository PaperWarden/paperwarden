# PW-017: SQLite and full-text search feasibility

**Issue:** #20  
**Date:** 2026-11-27  
**Result:** Feasible. Proven on Windows and on an Android emulator (x86_64,
Android 15); iOS is proven in CI only (macOS tests plus an iOS device build).  
**Decision record:** ADR-0006 (Proposed)

## What was built

`core/crates/store` (`paperwarden-store`): one SQLite database owned by the
Rust core, through `rusqlite` 0.43.0 with SQLite 3.54.1 compiled from source
(`bundled`), so no system SQLite is used on any platform.

| Need | How it is met |
| --- | --- |
| Settings | Key/value table; keys are 1-64 characters of `a-z 0-9 . _ -`, values at most 4 KiB. |
| Job history | Outcome, operation, times and an optional display name. Kept to the newest 1,000 entries; clearable. |
| Workflows | Name plus steps as JSON, validated with SQLite's `json_valid` and capped at 64 KiB. The workflow engine defines the step format later. |
| Optional index | FTS5, **off by default**, contentless with deletes, `unicode61` tokenizer with diacritics folded. Clearable and removable per document. |
| Migrations | Forward-only list tracked in `user_version`; each step is one transaction. |

The database is opened with `trusted_schema=OFF`, `secure_delete=ON`,
`foreign_keys=ON`, `temp_store=MEMORY` and, for files, WAL journaling.

## Criteria

| Criterion | Evidence | Result |
| --- | --- | --- |
| Settings persist | Write, overwrite, reopen, remove; malformed keys and oversized values refused | Pass |
| Job history | Bounded to 1,000 newest-first; inconsistent records refused; clearing leaves no trace in the database files | Pass |
| Workflows | Invalid JSON, empty or control-character names and oversized definitions refused; replace and delete | Pass |
| Index is opt-in | Indexing while off returns `IndexDisabled` | Pass |
| Index is not a copy of documents | After indexing, a unique term is found in the database files, but the indexed sentence is not | Pass |
| Index is clearable | After `clear_index` or `disable_index`, the term and the title appear in no database file (main, WAL, shared memory) | Pass |
| Search is safe | 14 hostile queries (FTS5 operators, quotes, column filters, SQL, NUL) are literal text; fuzzing `store_search` | Pass |
| Search quality | Prefix matching; "resume" finds "Résumé"; all words must match | Pass |
| Migrations | Fresh database reaches the latest version; a version-1 database upgrades and keeps its data; a failing migration rolls back completely; a database from a newer version is refused and left byte-identical | Pass |
| Android packaging | The store's 19 tests, cross-compiled with NDK 28 for x86_64, pass on the `PaperWarden_API_35` emulator | Pass |
| iOS packaging | `ios.yml` runs the tests on macOS and builds for `aarch64-apple-ios` | CI only |
| Size | SQLite with FTS5 and JSON, arm64 release: 1,791,757 bytes of code and 24,320 of data before linking | Measured |

## Encrypted-device assumptions

The database is **not encrypted by PaperWarden**. It relies on the operating
system's storage encryption, under these assumptions:

- **Location.** The database lives only in app-private internal storage:
  `Context.getFilesDir()` (or `getDatabasePath`) on Android and Application
  Support on iOS. Never on shared or external storage.
- **Android.** Devices launched with Android 10 or later must use file-based
  encryption, and app-private storage is credential-encrypted: unreadable
  until the user first unlocks the device after boot.
- **iOS.** Data Protection encrypts every file. The default class makes files
  available after first unlock; because PaperWarden does no background work,
  the database directory should use `NSFileProtectionComplete` so it is also
  unreadable while the device is locked. To be set when the iOS adapter
  creates the directory.
- **Backups.** App data must not leave the device. This change sets
  `android:allowBackup="false"` and excludes every domain from cloud backup
  and device-to-device transfer; on the emulator the installed app no longer
  carries the `ALLOW_BACKUP` flag. On iOS the database directory must be
  marked `isExcludedFromBackup` when the adapter creates it.
- **What this protects against.** A lost or stolen device that is powered off
  or locked.
- **What it does not.** An unlocked device in someone else's hands, a rooted or
  jailbroken device, or malware with root access can read the database. The
  index is contentless and the history clearable, which limits what is there
  to read.
- **Deleted data.** `secure_delete`, `VACUUM` and truncating the write-ahead
  log remove cleared data from the database files, which the tests verify.
  Flash storage may keep old physical blocks, but those are encrypted with the
  file's key under file-based encryption.

SQLCipher was considered and not adopted. Its key would live in the Android
Keystore or iOS Keychain, so it adds little against the threats above. It
would add a cryptographic native dependency, with licensing, size and
vulnerability exposure, and key-management failure modes. Revisit if the
threat model adds protection of an unlocked device.

## Supply chain

New Rust crates, all permissive and recorded in
`third_party/dependencies.json`:

- **Runtime:** `rusqlite` and `libsqlite3-sys` (MIT; SQLite itself is public
  domain), `hashlink`, `hashbrown`, `foldhash` (Zlib), `bitflags`, `smallvec`
  and the `fallible-*` iterators.
- **Build only:** `pkg-config`, `vcpkg`.
- **WebAssembly only, never built for Android or iOS:** `sqlite-wasm-rs`,
  `rsqlite-vfs`, `thiserror`. They are in the lockfile because Cargo locks
  every target.

`cargo deny` and `cargo audit` pass; the inventory check passes for 129
packages.

## Findings

1. **Android Auto Backup was on.** The manifest did not set `allowBackup`, so
   Android's default would copy app data, including future job history and
   index, to the user's cloud backup and to new devices. Turned off here (see
   above). Settings therefore do not move to a new phone; that trade-off is a
   product decision for maintainers.
2. **Opening a newer database must not write to it.** The first version
   switched to WAL before checking the schema version, which writes the file
   header. Migration checks now run first; a test verifies the file stays
   byte-identical.
3. **FTS5 rejects NUL even inside a quoted term.** Queries now drop control
   characters. Found by the hostile-query test; the `store_search` fuzz target
   finds the same bug within about 40 runs if the fix is removed.
4. **No snippets.** A contentless index cannot show matching passages. That is
   the price of not storing document text; showing a passage means opening
   the document.

## Follow-ups

- Expose the store through the bridge and create it from the platform
  adapters: app-private path, iOS `NSFileProtectionComplete` and
  `isExcludedFromBackup`.
- Decide whether job history stores display names at all, or only on request.
- Measure linked size and search latency in the app with a realistic index.
