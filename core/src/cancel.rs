//! Cooperative cancellation for running jobs.
//!
//! A [`CancellationToken`] only signals. Long-running work checks it between
//! steps and stops with [`Cancelled`]; the job's owner then moves the job to
//! `JobState::Cancelled` and cleans its workspace (`PRODUCT_INVARIANTS.md` 9).

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A shared flag that asks running work to stop.
///
/// Clones share one flag, so the UI keeps one clone and the worker another.
/// Cancelling is permanent and idempotent.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Creates a token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks every holder of this token to stop.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether [`cancel`](Self::cancel) has been called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Returns early from work that should stop, for use with `?`.
    ///
    /// # Errors
    ///
    /// Returns [`Cancelled`] once the token has been cancelled.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

/// The error returned by work that stopped because it was cancelled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the job was cancelled")
    }
}

impl std::error::Error for Cancelled {}
