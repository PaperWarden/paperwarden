//! Helpers shared by integration tests.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use paperwarden_core::engine::{EngineError, OutputVerifier, ProcessContext, Processor};
use paperwarden_core::validation::Rejection;
use std::path::{Path, PathBuf};
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

/// A processor defined by a closure.
pub struct FnProcessor<F>(pub F);

impl<F> Processor for FnProcessor<F>
where
    F: Fn(&mut ProcessContext<'_>) -> Result<(), EngineError>,
{
    fn process(&self, context: &mut ProcessContext<'_>) -> Result<(), EngineError> {
        (self.0)(context)
    }
}

/// A verifier defined by a closure.
pub struct FnVerifier<F>(pub F);

impl<F> OutputVerifier for FnVerifier<F>
where
    F: Fn(&Path, u64) -> Result<(), Rejection>,
{
    fn verify(&self, output: &Path, len: u64) -> Result<(), Rejection> {
        (self.0)(output, len)
    }
}
