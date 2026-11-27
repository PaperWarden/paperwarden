//! The local database through its public API, on real files.

#![cfg(test)]

use paperwarden_store::{HistoryOutcome, JobRecord, Store, StoreError, MAX_HISTORY_ENTRIES};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A sentinel no SQLite structure or other test data contains by chance.
const SENTINEL: &str = "zqxjvorpalsentinel";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "paperwarden-store-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    fn db(&self) -> PathBuf {
        self.0.join("paperwarden.db")
    }

    /// Every byte of the database and its side files.
    fn all_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for entry in fs::read_dir(&self.0).expect("read dir") {
            bytes.extend(fs::read(entry.expect("entry").path()).expect("read file"));
        }
        bytes
    }

    fn contains(&self, needle: &str) -> bool {
        let haystack = self.all_bytes();
        haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn job(id: u32, name: Option<&str>) -> JobRecord {
    JobRecord {
        job_id: format!("job-{id}"),
        operation: "pdf.merge".to_owned(),
        outcome: HistoryOutcome::Exported,
        started_at: 1_700_000_000 + i64::from(id),
        finished_at: 1_700_000_010 + i64::from(id),
        display_name: name.map(str::to_owned),
    }
}

#[test]
fn bundled_sqlite_has_fts5_and_json() {
    assert!(
        Store::sqlite_version() >= "3.43",
        "{}",
        Store::sqlite_version()
    );
    let mut store = Store::open_in_memory().expect("open");
    store.enable_index().expect("enable");
    store
        .index_document("d", "t", "fts5 works")
        .expect("fts5 available");
    store
        .save_workflow("w", "{\"steps\":[]}", 0)
        .expect("json1 available");
}

#[test]
fn settings_round_trip_and_persist() {
    let dir = TempDir::new("settings");
    {
        let store = Store::open(&dir.db()).expect("open");
        assert_eq!(store.setting("theme.mode").expect("read"), None);
        store.set_setting("theme.mode", "dark").expect("write");
        store.set_setting("theme.mode", "light").expect("overwrite");
    }
    let store = Store::open(&dir.db()).expect("reopen");
    assert_eq!(
        store.setting("theme.mode").expect("read").as_deref(),
        Some("light")
    );
    store.remove_setting("theme.mode").expect("remove");
    assert_eq!(store.setting("theme.mode").expect("read"), None);
    assert!(matches!(
        store.set_setting("Bad Key", "x"),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.set_setting("big", &"x".repeat(5000)),
        Err(StoreError::Invalid(_))
    ));
}

#[test]
fn job_history_is_bounded_newest_first() {
    let mut store = Store::open_in_memory().expect("open");
    for id in 0..MAX_HISTORY_ENTRIES + 5 {
        store.record_job(&job(id, None)).expect("record");
    }
    let jobs = store.recent_jobs(u32::MAX).expect("list");
    assert_eq!(jobs.len() as u32, MAX_HISTORY_ENTRIES);
    assert_eq!(jobs[0].job_id, format!("job-{}", MAX_HISTORY_ENTRIES + 4));
    assert_eq!(jobs.last().expect("oldest").job_id, "job-5");

    let mut backwards = job(1, None);
    backwards.finished_at = backwards.started_at - 1;
    assert!(matches!(
        store.record_job(&backwards),
        Err(StoreError::Invalid(_))
    ));
}

#[test]
fn clearing_history_leaves_no_trace_in_the_files() {
    let dir = TempDir::new("history");
    let mut store = Store::open(&dir.db()).expect("open");
    store
        .record_job(&job(1, Some(&format!("{SENTINEL}-tax-return.pdf"))))
        .expect("record");
    assert!(dir.contains(SENTINEL), "test cannot see stored data");

    store.clear_job_history().expect("clear");
    assert!(store.recent_jobs(10).expect("list").is_empty());
    assert!(
        !dir.contains(SENTINEL),
        "history survived in the database files"
    );
}

#[test]
fn workflows_require_valid_bounded_json() {
    let store = Store::open_in_memory().expect("open");
    store
        .save_workflow("Scan to PDF", "{\"steps\":[\"scan\",\"pdf\"]}", 1)
        .expect("save");
    store
        .save_workflow("Scan to PDF", "{\"steps\":[\"scan\"]}", 2)
        .expect("replace");
    store.save_workflow("Archive", "[]", 3).expect("save");
    assert_eq!(
        store.workflow_names().expect("names"),
        ["Archive", "Scan to PDF"]
    );
    assert_eq!(
        store.workflow("Scan to PDF").expect("read").as_deref(),
        Some("{\"steps\":[\"scan\"]}")
    );
    for (name, steps) in [("Bad", "{not json"), ("", "[]"), ("Tab\there", "[]")] {
        assert!(
            matches!(
                store.save_workflow(name, steps, 4),
                Err(StoreError::Invalid(_))
            ),
            "{name:?} {steps:?}"
        );
    }
    let huge = format!("[\"{}\"]", "x".repeat(70_000));
    assert!(matches!(
        store.save_workflow("Huge", &huge, 5),
        Err(StoreError::Invalid(_))
    ));
    store.delete_workflow("Archive").expect("delete");
    store
        .delete_workflow("Archive")
        .expect("deleting twice is fine");
    assert_eq!(store.workflow_names().expect("names"), ["Scan to PDF"]);
}

#[test]
fn index_is_off_until_enabled() {
    let mut store = Store::open_in_memory().expect("open");
    assert!(!store.index_enabled().expect("flag"));
    assert!(matches!(
        store.index_document("a", "A", "text"),
        Err(StoreError::IndexDisabled)
    ));
    store.enable_index().expect("enable");
    store.index_document("a", "A", "text").expect("index");
}

#[test]
fn search_finds_prefixes_and_folds_diacritics() {
    let mut store = Store::open_in_memory().expect("open");
    store.enable_index().expect("enable");
    store
        .index_document("doc-1", "Résumé", "Curriculum vitae for the Résumé review")
        .expect("index");
    store
        .index_document("doc-2", "Invoice", "Invoice number 42 for consulting")
        .expect("index");
    let titles = |query: &str| -> Vec<String> {
        store
            .search(query, 10)
            .expect("search")
            .into_iter()
            .map(|hit| hit.title)
            .collect()
    };
    assert_eq!(titles("resume"), ["Résumé"]);
    assert_eq!(titles("invo consult"), ["Invoice"]);
    assert_eq!(titles("invoice resume"), Vec::<String>::new());
    assert!(titles("").is_empty());
}

#[test]
fn hostile_queries_are_literal_text() {
    let mut store = Store::open_in_memory().expect("open");
    store.enable_index().expect("enable");
    store
        .index_document("d", "Doc", "plain words here")
        .expect("index");
    for query in [
        "\"",
        "\"\"\"",
        "NEAR(a b)",
        "body:plain",
        "*",
        "^plain",
        "plain OR",
        "AND NOT",
        "a\"b",
        "(",
        ")",
        "-plain",
        "plain'; DROP TABLE settings; --",
        "\u{0}",
    ] {
        assert!(store.search(query, 10).is_ok(), "{query:?} broke the query");
    }
    assert_eq!(store.search("plain", 10).expect("still works").len(), 1);
}

#[test]
fn reindexing_replaces_and_removal_forgets() {
    let mut store = Store::open_in_memory().expect("open");
    store.enable_index().expect("enable");
    store.index_document("d", "Old", "alpha").expect("index");
    store.index_document("d", "New", "beta").expect("reindex");
    assert!(store.search("alpha", 10).expect("search").is_empty());
    assert_eq!(store.search("beta", 10).expect("search")[0].title, "New");
    store.remove_document("d").expect("remove");
    assert!(store.search("beta", 10).expect("search").is_empty());
}

#[test]
fn index_never_stores_document_text_and_clearing_purges_terms() {
    let dir = TempDir::new("index");
    let mut store = Store::open(&dir.db()).expect("open");
    store.enable_index().expect("enable");
    let sentence = format!("quarterly {SENTINEL} salary figures");
    store
        .index_document("doc-1", "Payroll", &sentence)
        .expect("index");
    assert_eq!(store.search(SENTINEL, 10).expect("search").len(), 1);

    assert!(dir.contains(SENTINEL), "test cannot see indexed terms");
    assert!(
        !dir.contains(&sentence),
        "the contentless index stored the document text"
    );

    store.clear_index().expect("clear");
    assert!(store.search(SENTINEL, 10).expect("search").is_empty());
    assert!(!dir.contains(SENTINEL), "indexed terms survived clearing");
    assert!(!dir.contains("Payroll"), "titles survived clearing");
    assert!(
        store.index_enabled().expect("flag"),
        "clearing keeps the index on"
    );
}

#[test]
fn disabling_the_index_turns_it_off_and_clears_it() {
    let dir = TempDir::new("disable");
    let mut store = Store::open(&dir.db()).expect("open");
    store.enable_index().expect("enable");
    store.index_document("d", "Doc", SENTINEL).expect("index");
    store.disable_index().expect("disable");
    assert!(!store.index_enabled().expect("flag"));
    assert!(!dir.contains(SENTINEL));
}

#[test]
fn database_from_a_newer_version_is_refused_and_left_alone() {
    let dir = TempDir::new("newer");
    {
        let store = Store::open(&dir.db()).expect("open");
        store.set_setting("keep", "me").expect("write");
    }
    set_user_version(&dir.db(), 99);
    let before = fs::read(dir.db()).expect("read");
    match Store::open(&dir.db()) {
        Err(StoreError::NewerSchema { found: 99, .. }) => {}
        other => panic!("expected NewerSchema, got {other:?}"),
    }
    assert_eq!(
        fs::read(dir.db()).expect("read"),
        before,
        "a newer database was modified"
    );
}

/// Sets `user_version` in a closed database file by patching the header field
/// the SQLite file format defines at offset 60. Closing the store checkpointed
/// its write-ahead log, so the main file is current.
fn set_user_version(path: &Path, version: u32) {
    let mut bytes = fs::read(path).expect("read db");
    bytes[60..64].copy_from_slice(&version.to_be_bytes());
    fs::write(path, bytes).expect("write db");
}

#[test]
fn schema_is_current_after_open() {
    let store = Store::open_in_memory().expect("open");
    assert_eq!(store.schema_version().expect("version"), 2);
}
