//! Feasibility probes for the bridge (#12).
//!
//! They prove synchronous and asynchronous calls, cancellation and typed
//! errors end to end. They process no documents; real job APIs replace them
//! once the job skeleton (#33) lands.

use std::thread;
use std::time::Duration;

use flutter_rust_bridge::frb;
use paperwarden_core::{CancellationToken, Cancelled, JobId};

/// The most steps a probe job may run.
pub const MAX_PROBE_STEPS: u32 = 1_000;

/// The longest a single probe step may take, in milliseconds.
pub const MAX_PROBE_STEP_MILLIS: u32 = 1_000;

/// Returns the core's version. A synchronous call: Dart gets the value
/// directly, without a `Future`.
#[frb(sync)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Lets Dart cancel a call that is still running in Rust.
///
/// Opaque to Dart: Dart holds a handle, never the flag itself.
#[frb(opaque)]
pub struct CancelHandle {
    token: CancellationToken,
}

#[allow(
    clippy::new_without_default,
    reason = "a Default impl would add a redundant `default_()` to the Dart API"
)]
impl CancelHandle {
    /// Creates a handle that has not been cancelled.
    #[frb(sync)]
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    /// Asks the call using this handle to stop at its next check.
    #[frb(sync)]
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Returns whether [`cancel`](Self::cancel) has been called.
    #[frb(sync)]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// The core token behind this handle, for other bridge modules.
    pub(crate) fn token(&self) -> &CancellationToken {
        &self.token
    }
}

/// Why a bridge call failed. Dart receives it as a thrown exception.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeError {
    /// The category Dart code branches on.
    pub kind: BridgeErrorKind,
    /// A developer-facing description. Not shown to users as-is.
    pub message: String,
}

/// Categories of [`BridgeError`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeErrorKind {
    /// An argument was outside the documented bounds.
    InvalidInput,
    /// The call stopped because its [`CancelHandle`] was cancelled.
    Cancelled,
    /// A file could not be read.
    Unreadable,
}

impl BridgeError {
    pub(crate) fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            kind: BridgeErrorKind::InvalidInput,
            message: message.into(),
        }
    }
}

impl From<Cancelled> for BridgeError {
    fn from(cancelled: Cancelled) -> Self {
        Self {
            kind: BridgeErrorKind::Cancelled,
            message: cancelled.to_string(),
        }
    }
}

/// Runs `steps` simulated steps of `step_millis` each, checking `cancel`
/// before every step, and returns the number of steps completed.
///
/// Not `sync`: the bridge runs it on a worker thread and Dart gets a
/// `Future`, so a long call never blocks the UI.
///
/// # Errors
///
/// - [`BridgeErrorKind::InvalidInput`] if `steps` is 0 or above
///   [`MAX_PROBE_STEPS`], or `step_millis` is above [`MAX_PROBE_STEP_MILLIS`].
/// - [`BridgeErrorKind::Cancelled`] if `cancel` is cancelled before the last
///   step.
pub fn run_probe_job(
    steps: u32,
    step_millis: u32,
    cancel: &CancelHandle,
) -> Result<u32, BridgeError> {
    if steps == 0 || steps > MAX_PROBE_STEPS {
        return Err(BridgeError::invalid_input(format!(
            "steps must be between 1 and {MAX_PROBE_STEPS}"
        )));
    }
    if step_millis > MAX_PROBE_STEP_MILLIS {
        return Err(BridgeError::invalid_input(format!(
            "step_millis must be at most {MAX_PROBE_STEP_MILLIS}"
        )));
    }
    for _ in 0..steps {
        cancel.token.check()?;
        thread::sleep(Duration::from_millis(u64::from(step_millis)));
    }
    Ok(steps)
}

/// Validates `id` with the core's job-id rules, proving core errors reach Dart.
///
/// # Errors
///
/// [`BridgeErrorKind::InvalidInput`] if the core rejects `id`.
#[frb(sync)]
pub fn validate_job_id(id: String) -> Result<String, BridgeError> {
    JobId::new(id)
        .map(|job_id| job_id.as_str().to_owned())
        .map_err(BridgeError::invalid_input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_every_step() {
        assert_eq!(run_probe_job(3, 0, &CancelHandle::new()), Ok(3));
    }

    #[test]
    fn a_cancelled_handle_stops_the_job() {
        let handle = CancelHandle::new();
        handle.cancel();

        let error = run_probe_job(3, 0, &handle).unwrap_err();

        assert_eq!(error.kind, BridgeErrorKind::Cancelled);
    }

    #[test]
    fn cancelling_from_another_thread_stops_a_running_job() {
        let handle = std::sync::Arc::new(CancelHandle::new());
        let canceller = {
            let handle = std::sync::Arc::clone(&handle);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(30));
                handle.cancel();
            })
        };

        let result = run_probe_job(MAX_PROBE_STEPS, 10, &handle);
        canceller.join().unwrap();

        assert_eq!(result.unwrap_err().kind, BridgeErrorKind::Cancelled);
    }

    #[test]
    fn rejects_out_of_bounds_arguments() {
        let handle = CancelHandle::new();
        for (steps, millis) in [
            (0, 0),
            (MAX_PROBE_STEPS + 1, 0),
            (1, MAX_PROBE_STEP_MILLIS + 1),
        ] {
            let error = run_probe_job(steps, millis, &handle).unwrap_err();
            assert_eq!(error.kind, BridgeErrorKind::InvalidInput);
        }
    }

    #[test]
    fn core_job_id_errors_cross_the_bridge() {
        assert_eq!(validate_job_id("job-1".into()), Ok("job-1".into()));
        let error = validate_job_id("../escape".into()).unwrap_err();
        assert_eq!(error.kind, BridgeErrorKind::InvalidInput);
    }
}
