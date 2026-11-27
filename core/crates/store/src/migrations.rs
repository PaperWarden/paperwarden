//! Forward-only schema migrations, tracked in SQLite's `user_version`.
//!
//! Migration `n` (1-based) moves the schema from version `n - 1` to `n`. Each
//! runs in its own transaction with the version bump, so a failure leaves the
//! database at the previous version. Never edit a released migration; add a
//! new one.

use crate::{Result, StoreError};
use rusqlite::Connection;

/// Every migration, oldest first.
pub(crate) const MIGRATIONS: &[&str] = &[
    // 1: settings, job history and workflows.
    "CREATE TABLE settings (
         key   TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 64),
         value TEXT NOT NULL CHECK (length(value) <= 4096)
     ) STRICT;
     CREATE TABLE job_history (
         id           INTEGER PRIMARY KEY,
         job_id       TEXT NOT NULL,
         operation    TEXT NOT NULL,
         outcome      TEXT NOT NULL CHECK (outcome IN ('exported', 'discarded', 'failed', 'cancelled')),
         started_at   INTEGER NOT NULL,
         finished_at  INTEGER NOT NULL CHECK (finished_at >= started_at),
         display_name TEXT
     ) STRICT;
     CREATE TABLE workflows (
         name       TEXT PRIMARY KEY CHECK (length(name) BETWEEN 1 AND 100),
         steps      TEXT NOT NULL CHECK (json_valid(steps) AND length(steps) <= 65536),
         updated_at INTEGER NOT NULL
     ) STRICT;",
    // 2: the optional search index. Contentless with deletes: FTS5 keeps the
    // terms only, never the indexed text.
    "CREATE TABLE index_documents (
         rowid  INTEGER PRIMARY KEY,
         doc_id TEXT NOT NULL UNIQUE,
         title  TEXT NOT NULL
     ) STRICT;
     CREATE VIRTUAL TABLE index_terms USING fts5(
         body,
         content = '',
         contentless_delete = 1,
         tokenize = 'unicode61 remove_diacritics 2'
     );",
];

/// The schema version stored in the database.
pub(crate) fn version(connection: &Connection) -> Result<u32> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Applies every migration the database has not seen yet.
pub(crate) fn migrate(connection: &mut Connection, migrations: &[&str]) -> Result<()> {
    let supported = u32::try_from(migrations.len()).unwrap_or(u32::MAX);
    let current = version(connection)?;
    if current > supported {
        return Err(StoreError::NewerSchema {
            found: current,
            supported,
        });
    }
    let pending = migrations
        .iter()
        .zip(1_u32..)
        .skip(usize::try_from(current).unwrap_or(usize::MAX));
    for (sql, target) in pending {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", target)?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_exists(connection: &Connection, name: &str) -> bool {
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name = ?1",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .expect("query schema")
            == 1
    }

    #[test]
    fn fresh_database_reaches_the_latest_version() {
        let mut connection = Connection::open_in_memory().expect("open");
        migrate(&mut connection, MIGRATIONS).expect("migrate");
        assert_eq!(
            version(&connection).expect("version") as usize,
            MIGRATIONS.len()
        );
        migrate(&mut connection, MIGRATIONS).expect("migrating again is a no-op");
    }

    #[test]
    fn version_one_database_upgrades_and_keeps_its_data() {
        let mut connection = Connection::open_in_memory().expect("open");
        migrate(&mut connection, &MIGRATIONS[..1]).expect("migrate to 1");
        connection
            .execute(
                "INSERT INTO settings (key, value) VALUES ('theme', 'dark')",
                [],
            )
            .expect("insert");
        assert!(!table_exists(&connection, "index_documents"));

        migrate(&mut connection, MIGRATIONS).expect("migrate to latest");
        assert_eq!(version(&connection).expect("version"), 2);
        assert!(table_exists(&connection, "index_documents"));
        let theme: String = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'theme'",
                [],
                |row| row.get(0),
            )
            .expect("setting survives");
        assert_eq!(theme, "dark");
    }

    #[test]
    fn failing_migration_rolls_back_completely() {
        let mut connection = Connection::open_in_memory().expect("open");
        let broken = [
            MIGRATIONS[0],
            "CREATE TABLE half_done (x); THIS IS NOT SQL;",
        ];
        assert!(migrate(&mut connection, &broken).is_err());
        assert_eq!(version(&connection).expect("version"), 1);
        assert!(!table_exists(&connection, "half_done"));
    }

    #[test]
    fn newer_database_is_refused_and_untouched() {
        let mut connection = Connection::open_in_memory().expect("open");
        connection
            .pragma_update(None, "user_version", 99)
            .expect("set version");
        match migrate(&mut connection, MIGRATIONS) {
            Err(StoreError::NewerSchema {
                found: 99,
                supported,
            }) => {
                assert_eq!(supported as usize, MIGRATIONS.len());
            }
            other => panic!("expected NewerSchema, got {other:?}"),
        }
        assert_eq!(version(&connection).expect("version"), 99);
        assert!(!table_exists(&connection, "settings"));
    }
}
