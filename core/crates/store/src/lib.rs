//! PaperWarden's local database: settings, job history, saved workflows and
//! the optional search index (`ARCHITECTURE.md`, local data).
//!
//! One SQLite file in app-private storage, opened with defensive settings:
//! untrusted schema functions are refused, deleted content is overwritten
//! (`secure_delete`), and temporary data stays in memory. The schema is
//! versioned and migrated forward on open; a database from a newer app version
//! is refused rather than modified.
//!
//! Privacy rules (`PRIVACY.md`):
//!
//! - The search index is off until the user enables it, and clearing it leaves
//!   no indexed term in the database files.
//! - The index is contentless: it stores search terms, never a readable copy
//!   of a document's text.
//! - Job history is bounded and clearable the same way.
//!
//! The database relies on the operating system's storage encryption and is
//! not encrypted itself; `docs/feasibility/PW-017-sqlite-fts.md` states the
//! assumptions.

mod migrations;

use rusqlite::{params, Connection, OptionalExtension};
use std::fmt;
use std::path::Path;

/// Longest setting key, in bytes.
pub const MAX_KEY_BYTES: usize = 64;
/// Longest setting value, in bytes.
pub const MAX_VALUE_BYTES: usize = 4096;
/// Longest workflow definition, in bytes of JSON.
pub const MAX_WORKFLOW_BYTES: usize = 64 * 1024;
/// Longest text indexed for one document, in bytes.
pub const MAX_INDEXED_TEXT_BYTES: usize = 1024 * 1024;
/// Job-history entries kept; older ones are removed.
pub const MAX_HISTORY_ENTRIES: u32 = 1000;
/// Most search terms used from one query.
pub const MAX_QUERY_TERMS: usize = 16;

const INDEX_ENABLED_KEY: &str = "index.enabled";

/// Why a store operation failed.
#[derive(Debug)]
pub enum StoreError {
    /// SQLite reported an error.
    Database(rusqlite::Error),
    /// The database was written by a newer app version.
    NewerSchema {
        /// The database's schema version.
        found: u32,
        /// The newest version this build understands.
        supported: u32,
    },
    /// An argument broke a documented rule.
    Invalid(&'static str),
    /// Indexing was requested while the index is off.
    IndexDisabled,
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::NewerSchema { found, supported } => write!(
                formatter,
                "the database is from a newer version (schema {found}, this build supports {supported})"
            ),
            Self::Invalid(reason) => formatter.write_str(reason),
            Self::IndexDisabled => formatter.write_str("the search index is turned off"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// Shorthand for store results.
pub type Result<T> = std::result::Result<T, StoreError>;

/// How a recorded job ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryOutcome {
    /// The result was saved.
    Exported,
    /// The result was verified but not saved.
    Discarded,
    /// The job failed.
    Failed,
    /// The user cancelled the job.
    Cancelled,
}

impl HistoryOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exported => "exported",
            Self::Discarded => "discarded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "exported" => Some(Self::Exported),
            "discarded" => Some(Self::Discarded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// One entry in the job history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobRecord {
    /// The job's identifier.
    pub job_id: String,
    /// The operation, such as `pdf.merge`.
    pub operation: String,
    /// How the job ended.
    pub outcome: HistoryOutcome,
    /// Start time, seconds since the Unix epoch.
    pub started_at: i64,
    /// End time, seconds since the Unix epoch.
    pub finished_at: i64,
    /// The file name shown to the user, if any. Removed with the history.
    pub display_name: Option<String>,
}

/// How job history behaves (ADR-0006). Both are on unless the user turns
/// them off.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistorySettings {
    /// Whether finished jobs are recorded at all.
    pub enabled: bool,
    /// Whether entries keep the file name shown to the user.
    pub file_names: bool,
}

const HISTORY_ENABLED: &str = "history.enabled";
const HISTORY_FILE_NAMES: &str = "history.file-names";

/// A search result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    /// The indexed document's identifier.
    pub doc_id: String,
    /// The title stored with it.
    pub title: String,
}

/// The local database.
#[derive(Debug)]
pub struct Store {
    connection: Connection,
    file_backed: bool,
}

impl Store {
    /// Opens or creates the database at `path` and migrates it to the
    /// current schema.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::NewerSchema`] for a database from a newer app
    /// version, which is left unchanged, or a database error.
    pub fn open(path: &Path) -> Result<Self> {
        Self::configure(Connection::open(path)?, true)
    }

    /// Opens a private in-memory database, for tests and probes.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn open_in_memory() -> Result<Self> {
        Self::configure(Connection::open_in_memory()?, false)
    }

    fn configure(mut connection: Connection, file_backed: bool) -> Result<Self> {
        // Refuse functions with side effects in schema definitions, overwrite
        // deleted content, and keep temporary tables and indices in memory.
        connection.pragma_update(None, "trusted_schema", false)?;
        connection.pragma_update(None, "secure_delete", true)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "temp_store", "MEMORY")?;
        // Migrate (which refuses a newer schema) before anything writes to
        // the file, such as switching the journal mode.
        migrations::migrate(&mut connection, migrations::MIGRATIONS)?;
        if file_backed {
            let _mode: String =
                connection
                    .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        }
        Ok(Self {
            connection,
            file_backed,
        })
    }

    /// The schema version of the open database.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn schema_version(&self) -> Result<u32> {
        migrations::version(&self.connection)
    }

    /// The SQLite library version, such as `3.54.1`.
    pub fn sqlite_version() -> &'static str {
        rusqlite::version()
    }

    // --- Settings ----------------------------------------------------------

    /// Returns a setting's value.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Invalid`] for a malformed key, or a database error.
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        check_key(key)?;
        Ok(self
            .connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// Stores a setting.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Invalid`] for a malformed key or an oversized
    /// value, or a database error.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        check_key(key)?;
        if value.len() > MAX_VALUE_BYTES {
            return Err(StoreError::Invalid("setting value is too long"));
        }
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Removes a setting, so its default applies again.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Invalid`] for a malformed key, or a database error.
    pub fn remove_setting(&self, key: &str) -> Result<()> {
        check_key(key)?;
        self.connection
            .execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }

    // --- Job history -------------------------------------------------------

    fn flag(&self, key: &str) -> Result<bool> {
        Ok(self.setting(key)?.as_deref() != Some("off"))
    }

    fn set_flag(&self, key: &str, on: bool) -> Result<()> {
        self.set_setting(key, if on { "on" } else { "off" })
    }

    /// How job history behaves.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn history_settings(&self) -> Result<HistorySettings> {
        Ok(HistorySettings {
            enabled: self.flag(HISTORY_ENABLED)?,
            file_names: self.flag(HISTORY_FILE_NAMES)?,
        })
    }

    /// Turns job history on or off. While it is off, [`record_job`]
    /// records nothing; entries already there stay until
    /// [`clear_job_history`].
    ///
    /// [`record_job`]: Self::record_job
    /// [`clear_job_history`]: Self::clear_job_history
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn set_history_enabled(&mut self, enabled: bool) -> Result<()> {
        self.set_flag(HISTORY_ENABLED, enabled)
    }

    /// Keeps or hides file names in job history. Hiding them also removes the
    /// names from existing entries and purges them from the database files.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn set_history_file_names(&mut self, keep: bool) -> Result<()> {
        self.set_flag(HISTORY_FILE_NAMES, keep)?;
        if !keep {
            self.connection
                .execute("UPDATE job_history SET display_name = NULL", [])?;
            self.purge()?;
        }
        Ok(())
    }

    /// Records a finished job and drops entries beyond [`MAX_HISTORY_ENTRIES`].
    ///
    /// The user's [`HistorySettings`] apply here, in the core: with history
    /// off nothing is written, and with file names hidden the entry has no
    /// display name.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Invalid`] for oversized or inconsistent fields,
    /// or a database error.
    pub fn record_job(&mut self, record: &JobRecord) -> Result<()> {
        if record.job_id.is_empty() || record.job_id.len() > 64 || record.operation.len() > 64 {
            return Err(StoreError::Invalid(
                "job id or operation has an invalid length",
            ));
        }
        if record
            .display_name
            .as_ref()
            .is_some_and(|name| name.len() > 255)
        {
            return Err(StoreError::Invalid("display name is too long"));
        }
        if record.finished_at < record.started_at {
            return Err(StoreError::Invalid("job finished before it started"));
        }
        let settings = self.history_settings()?;
        if !settings.enabled {
            return Ok(());
        }
        let display_name = record.display_name.as_ref().filter(|_| settings.file_names);
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO job_history (job_id, operation, outcome, started_at, finished_at, display_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.job_id,
                record.operation,
                record.outcome.as_str(),
                record.started_at,
                record.finished_at,
                display_name,
            ],
        )?;
        transaction.execute(
            "DELETE FROM job_history WHERE id NOT IN
             (SELECT id FROM job_history ORDER BY id DESC LIMIT ?1)",
            [MAX_HISTORY_ENTRIES],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Returns up to `limit` jobs, newest first.
    ///
    /// # Errors
    ///
    /// Returns a database error, including for an unknown stored outcome.
    pub fn recent_jobs(&self, limit: u32) -> Result<Vec<JobRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT job_id, operation, outcome, started_at, finished_at, display_name
             FROM job_history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            let outcome: String = row.get(2)?;
            Ok(JobRecord {
                job_id: row.get(0)?,
                operation: row.get(1)?,
                outcome: HistoryOutcome::parse(&outcome).ok_or_else(|| {
                    rusqlite::Error::InvalidColumnType(2, outcome, rusqlite::types::Type::Text)
                })?,
                started_at: row.get(3)?,
                finished_at: row.get(4)?,
                display_name: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Deletes the whole job history and purges it from the database files.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn clear_job_history(&mut self) -> Result<()> {
        self.connection.execute("DELETE FROM job_history", [])?;
        self.purge()
    }

    // --- Workflows ---------------------------------------------------------

    /// Saves a workflow, replacing one with the same name. `steps` must be
    /// valid JSON; the workflow engine defines its structure.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Invalid`] for a bad name, oversized or invalid
    /// JSON, or a database error.
    pub fn save_workflow(&self, name: &str, steps: &str, updated_at: i64) -> Result<()> {
        if name.trim().is_empty() || name.len() > 100 || name.chars().any(char::is_control) {
            return Err(StoreError::Invalid(
                "workflow name is empty, too long or has control characters",
            ));
        }
        if steps.len() > MAX_WORKFLOW_BYTES {
            return Err(StoreError::Invalid("workflow is too large"));
        }
        let valid: bool = self
            .connection
            .query_row("SELECT json_valid(?1)", [steps], |row| row.get(0))?;
        if !valid {
            return Err(StoreError::Invalid("workflow steps are not valid JSON"));
        }
        self.connection.execute(
            "INSERT INTO workflows (name, steps, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (name) DO UPDATE SET steps = excluded.steps, updated_at = excluded.updated_at",
            params![name, steps, updated_at],
        )?;
        Ok(())
    }

    /// Returns a workflow's steps.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn workflow(&self, name: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT steps FROM workflows WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Returns workflow names, alphabetically.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn workflow_names(&self) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT name FROM workflows ORDER BY name")?;
        let names = statement.query_map([], |row| row.get(0))?;
        Ok(names.collect::<rusqlite::Result<_>>()?)
    }

    /// Deletes a workflow. Deleting one that does not exist is not an error.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn delete_workflow(&self, name: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM workflows WHERE name = ?1", [name])?;
        Ok(())
    }

    // --- Search index ------------------------------------------------------

    /// Whether the user has turned the search index on. Off by default.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn index_enabled(&self) -> Result<bool> {
        Ok(self.setting(INDEX_ENABLED_KEY)?.as_deref() == Some("true"))
    }

    /// Turns the search index on.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn enable_index(&self) -> Result<()> {
        self.set_setting(INDEX_ENABLED_KEY, "true")
    }

    /// Turns the search index off and clears it.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn disable_index(&mut self) -> Result<()> {
        self.remove_setting(INDEX_ENABLED_KEY)?;
        self.clear_index()
    }

    /// Adds or replaces a document in the index. Only search terms are kept;
    /// `text` itself is not stored.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::IndexDisabled`] while the index is off,
    /// [`StoreError::Invalid`] for bad arguments, or a database error.
    pub fn index_document(&mut self, doc_id: &str, title: &str, text: &str) -> Result<()> {
        if !self.index_enabled()? {
            return Err(StoreError::IndexDisabled);
        }
        if doc_id.is_empty() || doc_id.len() > 128 || title.len() > 255 {
            return Err(StoreError::Invalid(
                "document id or title has an invalid length",
            ));
        }
        if text.len() > MAX_INDEXED_TEXT_BYTES {
            return Err(StoreError::Invalid("document text is too large to index"));
        }
        let transaction = self.connection.transaction()?;
        let existing: Option<i64> = transaction
            .query_row(
                "SELECT rowid FROM index_documents WHERE doc_id = ?1",
                [doc_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(rowid) = existing {
            transaction.execute("DELETE FROM index_terms WHERE rowid = ?1", [rowid])?;
            transaction.execute("DELETE FROM index_documents WHERE rowid = ?1", [rowid])?;
        }
        transaction.execute(
            "INSERT INTO index_documents (doc_id, title) VALUES (?1, ?2)",
            params![doc_id, title],
        )?;
        let rowid = transaction.last_insert_rowid();
        transaction.execute(
            "INSERT INTO index_terms (rowid, body) VALUES (?1, ?2)",
            params![rowid, text],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Removes one document from the index.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn remove_document(&mut self, doc_id: &str) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let rowid: Option<i64> = transaction
            .query_row(
                "SELECT rowid FROM index_documents WHERE doc_id = ?1",
                [doc_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(rowid) = rowid {
            transaction.execute("DELETE FROM index_terms WHERE rowid = ?1", [rowid])?;
            transaction.execute("DELETE FROM index_documents WHERE rowid = ?1", [rowid])?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Finds documents containing every word of `query`, best match first.
    /// Words match as prefixes; the query is never interpreted as FTS5
    /// syntax, so any input is safe.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>> {
        let Some(expression) = match_expression(query) else {
            return Ok(Vec::new());
        };
        let mut statement = self.connection.prepare(
            "SELECT d.doc_id, d.title FROM index_terms
             JOIN index_documents d ON d.rowid = index_terms.rowid
             WHERE index_terms MATCH ?1 ORDER BY bm25(index_terms) LIMIT ?2",
        )?;
        let hits = statement.query_map(params![expression, limit], |row| {
            Ok(SearchHit {
                doc_id: row.get(0)?,
                title: row.get(1)?,
            })
        })?;
        Ok(hits.collect::<rusqlite::Result<_>>()?)
    }

    /// Deletes every indexed document and purges the terms from the database
    /// files.
    ///
    /// # Errors
    ///
    /// Returns a database error.
    pub fn clear_index(&mut self) -> Result<()> {
        self.connection.execute_batch(
            "DELETE FROM index_documents;
             INSERT INTO index_terms (index_terms) VALUES ('delete-all');",
        )?;
        self.purge()
    }

    /// Rebuilds the file so freed pages are gone, then empties the
    /// write-ahead log, so deleted data is left in neither file.
    fn purge(&mut self) -> Result<()> {
        self.connection.execute_batch("VACUUM;")?;
        if self.file_backed {
            let _busy: i64 =
                self.connection
                    .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        }
        Ok(())
    }
}

fn check_key(key: &str) -> Result<()> {
    let valid = !key.is_empty()
        && key.len() <= MAX_KEY_BYTES
        && key.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        });
    if valid {
        Ok(())
    } else {
        Err(StoreError::Invalid(
            "setting keys are 1-64 characters of a-z, 0-9, '.', '_' or '-'",
        ))
    }
}

/// Turns user input into an FTS5 expression of quoted prefix terms, so no
/// input is parsed as FTS5 syntax. Returns `None` when there is nothing to
/// search for.
fn match_expression(query: &str) -> Option<String> {
    // Control characters, NUL among them, are dropped: FTS5 rejects them
    // even inside a quoted term, and they never belong in a search.
    let printable: String = query
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let terms: Vec<String> = printable
        .split_whitespace()
        .take(MAX_QUERY_TERMS)
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_expression_quotes_every_term() {
        assert_eq!(match_expression("  "), None);
        assert_eq!(
            match_expression("tax report").as_deref(),
            Some("\"tax\"* \"report\"*")
        );
        assert_eq!(match_expression("a\"b").as_deref(), Some("\"a\"\"b\"*"));
        assert_eq!(match_expression("\0\x07\n").as_deref(), None);
        assert_eq!(match_expression("ta\0x").as_deref(), Some("\"ta\"* \"x\"*"));
        let many = "w ".repeat(40);
        assert_eq!(
            match_expression(&many).map(|expression| expression.matches('*').count()),
            Some(MAX_QUERY_TERMS)
        );
    }

    #[test]
    fn setting_keys_are_restricted() {
        assert!(check_key("theme.mode").is_ok());
        let too_long = "k".repeat(MAX_KEY_BYTES + 1);
        for key in ["", "Theme", "a b", "x;DROP", too_long.as_str()] {
            assert!(check_key(key).is_err(), "{key:?}");
        }
    }
}
