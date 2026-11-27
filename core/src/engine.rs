//! The seam between the job runner and processing engines.
//!
//! An engine implements [`Processor`]. It reads the staged input, writes at
//! most one output through [`ProcessContext::create_output`], and calls
//! [`ProcessContext::checkpoint`] between steps so cancellation and the time
//! limit take effect. An [`OutputVerifier`] then checks the output before it
//! can be exported (`ARCHITECTURE.md`, processing boundary).
//!
//! Bounds are enforced in process and cooperatively: the output limit on every
//! write, cancellation and time at checkpoints and between stages. An engine
//! that never reaches a checkpoint cannot be preempted; isolating engines in a
//! separate process is a decision for each engine's feasibility work.

use crate::validation::Rejection;
use crate::CancellationToken;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;

/// Transforms a staged input into an output inside the job's workspace.
pub trait Processor {
    /// Processes the input.
    ///
    /// # Errors
    ///
    /// Returns an [`EngineError`] if processing fails, is interrupted or
    /// exceeds a limit. The job then fails and its workspace is removed.
    fn process(&self, context: &mut ProcessContext<'_>) -> Result<(), EngineError>;
}

/// Checks a processed output before it may be exported.
pub trait OutputVerifier {
    /// Accepts or rejects the output at `output`, which is `len` bytes long.
    ///
    /// # Errors
    ///
    /// Returns a [`Rejection`] for output that must not be exported.
    fn verify(&self, output: &Path, len: u64) -> Result<(), Rejection>;
}

/// Why work stopped before finishing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interruption {
    /// The user cancelled the job.
    Cancelled,
    /// The job exceeded its time limit.
    TimedOut,
}

impl fmt::Display for Interruption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "the job was cancelled",
            Self::TimedOut => "the job exceeded its time limit",
        })
    }
}

impl std::error::Error for Interruption {}

/// An error reported by a [`Processor`].
#[derive(Debug)]
pub enum EngineError {
    /// Cancellation or the time limit stopped the work.
    Interrupted(Interruption),
    /// The output would exceed the job's output limit.
    OutputLimit,
    /// A file operation failed.
    Io(io::ErrorKind),
    /// The engine could not process this input. The reason is shown to the
    /// user and must not contain document content.
    Failed(&'static str),
}

impl From<Interruption> for EngineError {
    fn from(interruption: Interruption) -> Self {
        Self::Interrupted(interruption)
    }
}

impl From<io::Error> for EngineError {
    fn from(error: io::Error) -> Self {
        if error
            .get_ref()
            .is_some_and(|inner| inner.is::<OutputLimitExceeded>())
        {
            Self::OutputLimit
        } else {
            Self::Io(error.kind())
        }
    }
}

/// Cancellation and deadline shared by every stage of one job.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Guard<'a> {
    cancel: &'a CancellationToken,
    deadline: Option<Instant>,
}

impl<'a> Guard<'a> {
    pub(crate) fn new(cancel: &'a CancellationToken, deadline: Option<Instant>) -> Self {
        Self { cancel, deadline }
    }

    pub(crate) fn check(&self) -> Result<(), Interruption> {
        if self.cancel.is_cancelled() {
            return Err(Interruption::Cancelled);
        }
        match self.deadline {
            Some(deadline) if Instant::now() >= deadline => Err(Interruption::TimedOut),
            _ => Ok(()),
        }
    }
}

/// What a [`Processor`] may use while it runs.
#[derive(Debug)]
pub struct ProcessContext<'a> {
    input: &'a Path,
    input_len: u64,
    output: &'a Path,
    max_output_bytes: u64,
    guard: Guard<'a>,
}

impl<'a> ProcessContext<'a> {
    pub(crate) fn new(
        input: &'a Path,
        input_len: u64,
        output: &'a Path,
        max_output_bytes: u64,
        guard: Guard<'a>,
    ) -> Self {
        Self {
            input,
            input_len,
            output,
            max_output_bytes,
            guard,
        }
    }

    /// The staged copy of the input inside the workspace. It is never the
    /// user's original, and its name carries nothing from the original.
    pub fn input_path(&self) -> &Path {
        self.input
    }

    /// The staged input's length in bytes.
    pub fn input_len(&self) -> u64 {
        self.input_len
    }

    /// Stops the work if the job was cancelled or ran out of time.
    ///
    /// # Errors
    ///
    /// Returns the [`Interruption`]; pass it on with `?`.
    pub fn checkpoint(&self) -> Result<(), Interruption> {
        self.guard.check()
    }

    /// Creates the job's single output file.
    ///
    /// # Errors
    ///
    /// Returns an error if the output already exists or cannot be created.
    pub fn create_output(&mut self) -> io::Result<BoundedWriter> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.output)?;
        Ok(BoundedWriter {
            file,
            written: 0,
            limit: self.max_output_bytes,
        })
    }
}

/// Writes the job output and fails once it would exceed the output limit.
#[derive(Debug)]
pub struct BoundedWriter {
    file: File,
    written: u64,
    limit: u64,
}

impl Write for BoundedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let requested = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        if self.written.saturating_add(requested) > self.limit {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                OutputLimitExceeded,
            ));
        }
        let written = self.file.write(buf)?;
        self.written = self
            .written
            .saturating_add(u64::try_from(written).unwrap_or(u64::MAX));
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Marks the I/O error [`BoundedWriter`] returns at the output limit.
#[derive(Debug)]
struct OutputLimitExceeded;

impl fmt::Display for OutputLimitExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("output limit exceeded")
    }
}

impl std::error::Error for OutputLimitExceeded {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn guard_reports_cancellation_before_timeout() {
        let cancel = CancellationToken::new();
        let past = Instant::now().checked_sub(Duration::from_secs(1));
        let guard = Guard::new(&cancel, past);
        assert_eq!(guard.check(), Err(Interruption::TimedOut));
        cancel.cancel();
        assert_eq!(guard.check(), Err(Interruption::Cancelled));
    }

    #[test]
    fn guard_without_deadline_runs_until_cancelled() {
        let cancel = CancellationToken::new();
        let guard = Guard::new(&cancel, None);
        assert_eq!(guard.check(), Ok(()));
    }

    #[test]
    fn output_limit_error_maps_to_engine_output_limit() {
        let error = io::Error::new(io::ErrorKind::FileTooLarge, OutputLimitExceeded);
        assert!(matches!(EngineError::from(error), EngineError::OutputLimit));
        let other = io::Error::from(io::ErrorKind::FileTooLarge);
        assert!(matches!(
            EngineError::from(other),
            EngineError::Io(io::ErrorKind::FileTooLarge)
        ));
    }
}
