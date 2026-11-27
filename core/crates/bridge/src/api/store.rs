//! The app's local database (ADR-0006), as an opaque handle for Dart.
//!
//! Dart opens it once at launch with a path inside app-private storage and
//! keeps the handle. Only the history controls (#63) are exposed so far;
//! keep this narrow and add calls as features need them.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use flutter_rust_bridge::frb;
use paperwarden_store::{Store, StoreError};

use super::probe::{BridgeError, BridgeErrorKind};

/// How job history behaves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryPrefs {
    /// Whether finished jobs are recorded at all.
    pub enabled: bool,
    /// Whether entries keep the file name shown to the user.
    pub file_names: bool,
}

/// The open database. Calls on one handle are serialised.
#[frb(opaque)]
pub struct AppStore {
    store: Mutex<Store>,
}

fn storage(error: &StoreError) -> BridgeError {
    // Only the category and SQLite's numeric code: SQLite's own text can
    // quote values, and the message must never carry user data.
    let message = match error {
        StoreError::Database(rusqlite::Error::SqliteFailure(code, _)) => {
            format!("database error (SQLite code {})", code.extended_code)
        }
        StoreError::Database(_) => "database error".to_owned(),
        StoreError::NewerSchema { found, supported } => format!(
            "the database is from a newer version (schema {found}, this build supports {supported})"
        ),
        StoreError::Invalid(reason) => (*reason).to_owned(),
        StoreError::IndexDisabled => "the search index is turned off".to_owned(),
    };
    BridgeError {
        kind: BridgeErrorKind::Storage,
        message,
    }
}

impl AppStore {
    /// Opens or creates the database at `path`, migrating it if needed.
    ///
    /// # Errors
    ///
    /// [`BridgeErrorKind::Storage`] if the database cannot be opened,
    /// including one written by a newer version of the app.
    pub fn open(path: String) -> Result<AppStore, BridgeError> {
        let store = Store::open(Path::new(&path)).map_err(|error| storage(&error))?;
        Ok(Self {
            store: Mutex::new(store),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Store>, BridgeError> {
        self.store.lock().map_err(|_| BridgeError {
            kind: BridgeErrorKind::Storage,
            message: "the database is unavailable after an earlier failure".to_owned(),
        })
    }

    /// How job history behaves now.
    ///
    /// # Errors
    ///
    /// [`BridgeErrorKind::Storage`] on a database error.
    pub fn history_prefs(&self) -> Result<HistoryPrefs, BridgeError> {
        let settings = self
            .lock()?
            .history_settings()
            .map_err(|error| storage(&error))?;
        Ok(HistoryPrefs {
            enabled: settings.enabled,
            file_names: settings.file_names,
        })
    }

    /// Turns job history on or off. Existing entries stay until cleared.
    ///
    /// # Errors
    ///
    /// [`BridgeErrorKind::Storage`] on a database error.
    pub fn set_history_enabled(&self, enabled: bool) -> Result<(), BridgeError> {
        self.lock()?
            .set_history_enabled(enabled)
            .map_err(|error| storage(&error))
    }

    /// Keeps or hides file names. Hiding also removes existing names.
    ///
    /// # Errors
    ///
    /// [`BridgeErrorKind::Storage`] on a database error.
    pub fn set_history_file_names(&self, keep: bool) -> Result<(), BridgeError> {
        self.lock()?
            .set_history_file_names(keep)
            .map_err(|error| storage(&error))
    }

    /// Deletes the whole job history, leaving no trace in the database files.
    ///
    /// # Errors
    ///
    /// [`BridgeErrorKind::Storage`] on a database error.
    pub fn clear_history(&self) -> Result<(), BridgeError> {
        self.lock()?
            .clear_job_history()
            .map_err(|error| storage(&error))
    }

    /// How many entries the job history holds.
    ///
    /// # Errors
    ///
    /// [`BridgeErrorKind::Storage`] on a database error.
    pub fn history_count(&self) -> Result<u32, BridgeError> {
        let jobs = self
            .lock()?
            .recent_jobs(u32::MAX)
            .map_err(|error| storage(&error))?;
        Ok(u32::try_from(jobs.len()).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paperwarden_store::{HistoryOutcome, JobRecord};

    fn temp_db(name: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!(
            "paperwarden-bridge-store-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("paperwarden.db").to_string_lossy().into_owned();
        (dir, path)
    }

    #[test]
    fn history_controls_persist_and_clear() {
        let (dir, path) = temp_db("controls");
        let store = AppStore::open(path.clone()).expect("open");
        assert_eq!(
            store.history_prefs().expect("prefs"),
            HistoryPrefs {
                enabled: true,
                file_names: true
            }
        );
        store
            .lock()
            .expect("lock")
            .record_job(&JobRecord {
                job_id: "job-1".to_owned(),
                operation: "pdf.view".to_owned(),
                outcome: HistoryOutcome::Exported,
                started_at: 1,
                finished_at: 2,
                display_name: Some("tax.pdf".to_owned()),
            })
            .expect("record");
        assert_eq!(store.history_count().expect("count"), 1);

        store.set_history_enabled(false).expect("off");
        store.set_history_file_names(false).expect("hide");
        drop(store);
        let store = AppStore::open(path).expect("reopen");
        assert_eq!(
            store.history_prefs().expect("prefs"),
            HistoryPrefs {
                enabled: false,
                file_names: false
            }
        );
        store.clear_history().expect("clear");
        assert_eq!(store.history_count().expect("count"), 0);
        drop(store);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn an_unopenable_database_is_a_storage_error() {
        let (dir, _) = temp_db("bad");
        // A directory cannot be opened as a database.
        let error = AppStore::open(dir.to_string_lossy().into_owned())
            .err()
            .expect("refused");
        assert_eq!(error.kind, BridgeErrorKind::Storage);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
