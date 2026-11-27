//! Helpers shared by integration tests.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Returns a unique, not-yet-created directory for one test, so tests can run
/// in parallel without sharing state.
pub fn unique_root(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "paperwarden-it-{test_name}-{}-{nonce}",
        std::process::id()
    ))
}
