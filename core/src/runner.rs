//! Runs one job through the processing boundary in `ARCHITECTURE.md`:
//! validate, process in an isolated workspace with bounded resources, verify,
//! export explicitly, clean up.
//!
//! [`JobRunner::run`] either returns a [`ReadyJob`] holding a verified output
//! or a [`JobReport`] for a job that stopped. Every stopped job has already had
//! its workspace removed; a ready job removes it when it is finished or
//! dropped (`PRODUCT_INVARIANTS.md` 9).

use crate::engine::{EngineError, Guard, Interruption, OutputVerifier, ProcessContext, Processor};
use crate::export;
use crate::limits::JobLimits;
use crate::validation::{InputValidator, StagedInput, HEADER_LEN};
use crate::workspace::{RecoveryReport, Workspace};
use crate::{CancellationToken, Job, JobId, JobState};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

const STAGED_INPUT: &str = "input";
const OUTPUT: &str = "output";
const CHUNK: usize = 64 * 1024;

/// The three pieces an operation plugs into the runner.
pub struct Operation<'a> {
    /// Accepts or rejects the staged input from its content.
    pub validator: &'a dyn InputValidator,
    /// Produces the output.
    pub processor: &'a dyn Processor,
    /// Accepts or rejects the output before it can be exported.
    pub verifier: &'a dyn OutputVerifier,
}

/// Why a job stopped. Messages describe the failure, never document content
/// or paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobError {
    /// The user cancelled the job.
    Cancelled,
    /// The job exceeded its time limit.
    TimedOut,
    /// The input is not a regular file.
    InputNotAFile,
    /// The input is larger than the job allows.
    InputTooLarge {
        /// The limit in bytes.
        limit: u64,
    },
    /// The validator rejected the input.
    InputRejected(&'static str),
    /// The engine could not process the input.
    EngineFailed(&'static str),
    /// The input exceeds one of the engine's resource bounds.
    LimitExceeded(&'static str),
    /// The engine panicked; treated as a failed job (`THREAT_MODEL.md`).
    EnginePanicked,
    /// The output is larger than the job allows.
    OutputTooLarge {
        /// The limit in bytes.
        limit: u64,
    },
    /// The engine finished without writing an output.
    OutputMissing,
    /// The output is not a regular file or the verifier rejected it.
    OutputRejected(&'static str),
    /// Something already exists at the export destination.
    DestinationExists,
    /// The job's output was already exported.
    AlreadyExported,
    /// A file operation failed.
    Io(io::ErrorKind),
    /// The lifecycle refused a transition; a bug in the runner.
    InvalidTransition,
}

impl fmt::Display for JobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("the job was cancelled"),
            Self::TimedOut => formatter.write_str("the job exceeded its time limit"),
            Self::InputNotAFile => formatter.write_str("the selected item is not a file"),
            Self::InputTooLarge { limit } => {
                write!(formatter, "the file is larger than the {limit}-byte limit")
            }
            Self::InputRejected(reason)
            | Self::EngineFailed(reason)
            | Self::LimitExceeded(reason)
            | Self::OutputRejected(reason) => formatter.write_str(reason),
            Self::EnginePanicked => formatter.write_str("processing failed unexpectedly"),
            Self::OutputTooLarge { limit } => {
                write!(
                    formatter,
                    "the result is larger than the {limit}-byte limit"
                )
            }
            Self::OutputMissing => formatter.write_str("processing produced no result"),
            Self::DestinationExists => {
                formatter.write_str("a file already exists at the chosen destination")
            }
            Self::AlreadyExported => formatter.write_str("the result was already saved"),
            Self::Io(kind) => write!(formatter, "a file operation failed: {kind}"),
            Self::InvalidTransition => formatter.write_str("internal job-state error"),
        }
    }
}

impl std::error::Error for JobError {}

impl From<Interruption> for JobError {
    fn from(interruption: Interruption) -> Self {
        match interruption {
            Interruption::Cancelled => Self::Cancelled,
            Interruption::TimedOut => Self::TimedOut,
        }
    }
}

/// How a job ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// The output was exported, then the workspace was removed.
    Exported,
    /// The output was verified but not exported, and was removed.
    Discarded,
    /// The job stopped before its output was ready.
    Stopped(JobError),
}

/// The result of a finished job, observable without telemetry (ADR-0004).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobReport {
    /// The job.
    pub id: JobId,
    /// How it ended.
    pub outcome: Outcome,
    /// [`JobState::Cleaned`] if the workspace is gone; otherwise
    /// [`JobState::Failed`] or [`JobState::Cancelled`], and stale recovery
    /// removes the workspace later.
    pub state: JobState,
    /// Why cleanup failed, if it did.
    pub cleanup_error: Option<io::ErrorKind>,
}

/// Starts jobs in workspaces under one root.
#[derive(Clone, Debug)]
pub struct JobRunner {
    root: PathBuf,
    limits: JobLimits,
}

impl JobRunner {
    /// Creates a runner for workspaces under `root`, an absolute path to a
    /// directory dedicated to PaperWarden. Use one runner per root.
    pub fn new(root: impl Into<PathBuf>, limits: JobLimits) -> Self {
        Self {
            root: root.into(),
            limits,
        }
    }

    /// Removes workspaces abandoned by a crash. Call once at startup, before
    /// any job runs.
    ///
    /// # Errors
    ///
    /// Returns an error if the root is a link or a file, or cannot be read.
    pub fn recover(&self) -> io::Result<RecoveryReport> {
        Workspace::recover_stale(&self.root)
    }

    /// Validates, processes and verifies `input` without modifying it.
    ///
    /// # Errors
    ///
    /// Returns a [`JobReport`] if the job stopped. Its workspace has already
    /// been removed unless the report records a cleanup error.
    pub fn run(
        &self,
        id: JobId,
        input: &Path,
        operation: &Operation<'_>,
        cancel: &CancellationToken,
    ) -> Result<ReadyJob, JobReport> {
        let deadline = Instant::now().checked_add(self.limits.max_duration());
        let guard = Guard::new(cancel, deadline);
        let job = Job::new(id.clone(), input.to_path_buf(), PathBuf::new());

        if let Err(interruption) = guard.check() {
            return Err(stop_without_workspace(job, interruption.into()));
        }
        let workspace = match Workspace::create(&self.root, &id) {
            Ok(workspace) => workspace,
            Err(error) => return Err(stop_without_workspace(job, JobError::Io(error.kind()))),
        };
        let mut active = ActiveJob { job, workspace };
        active.job.workspace = active.workspace.path().to_path_buf();

        match self.drive(&mut active, input, operation, &guard) {
            Ok(output_len) => Ok(ReadyJob {
                active,
                output_len,
                cancel: cancel.clone(),
            }),
            Err(error) => Err(active.stop(error)),
        }
    }

    fn drive(
        &self,
        active: &mut ActiveJob,
        input: &Path,
        operation: &Operation<'_>,
        guard: &Guard<'_>,
    ) -> Result<u64, JobError> {
        let staged = active.workspace.input_dir().join(STAGED_INPUT);
        let (header, input_len) = stage(input, &staged, self.limits.max_input_bytes(), guard)?;
        operation
            .validator
            .validate(&StagedInput::new(&header, input_len))
            .map_err(|rejection| JobError::InputRejected(rejection.0))?;
        active.transition(JobState::Validated)?;

        guard.check()?;
        active.transition(JobState::Processing)?;
        let output = active.workspace.output_dir().join(OUTPUT);
        let max_output = self.limits.max_output_bytes();
        let mut context = ProcessContext::new(&staged, input_len, &output, max_output, *guard);
        match catch_unwind(AssertUnwindSafe(|| {
            operation.processor.process(&mut context)
        })) {
            Err(_) => return Err(JobError::EnginePanicked),
            Ok(result) => result.map_err(|error| engine_error(error, max_output))?,
        }

        guard.check()?;
        active.transition(JobState::Verifying)?;
        let output_len = inspect_output(&output, max_output)?;
        match catch_unwind(AssertUnwindSafe(|| {
            operation.verifier.verify(&output, output_len)
        })) {
            Err(_) => return Err(JobError::EnginePanicked),
            Ok(result) => result.map_err(|rejection| JobError::OutputRejected(rejection.0))?,
        }

        guard.check()?;
        active.transition(JobState::ReadyToExport)?;
        Ok(output_len)
    }
}

/// A job whose verified output awaits an explicit export.
///
/// Dropping it removes the workspace; [`finish`](Self::finish) does the same
/// and reports the outcome.
#[derive(Debug)]
pub struct ReadyJob {
    active: ActiveJob,
    output_len: u64,
    cancel: CancellationToken,
}

impl ReadyJob {
    /// The job's identifier.
    pub fn id(&self) -> &JobId {
        &self.active.job.id
    }

    /// The job's current state.
    pub fn state(&self) -> JobState {
        self.active.job.state
    }

    /// The verified output's length in bytes.
    pub fn output_len(&self) -> u64 {
        self.output_len
    }

    /// Writes the output to a new file at `destination`, which the user chose.
    ///
    /// # Errors
    ///
    /// Returns [`JobError::DestinationExists`] if anything is already at
    /// `destination` (it is left untouched), [`JobError::AlreadyExported`]
    /// after a successful export, or another error if the copy fails or is
    /// cancelled. The job stays ready after a failed export.
    pub fn export_to(&mut self, destination: &Path) -> Result<u64, JobError> {
        if self.active.job.state != JobState::ReadyToExport {
            return Err(JobError::AlreadyExported);
        }
        let output = self.active.workspace.output_dir().join(OUTPUT);
        let written = export::export_new(&output, destination, &self.cancel)?;
        self.active.transition(JobState::Exported)?;
        Ok(written)
    }

    /// Removes the workspace and reports how the job ended.
    pub fn finish(mut self) -> JobReport {
        if self.active.job.state == JobState::Exported {
            self.active.finish(Outcome::Exported)
        } else {
            // A verified result the user chose not to keep.
            let _ = self.active.transition(JobState::Cancelled);
            self.active.finish(Outcome::Discarded)
        }
    }
}

#[derive(Debug)]
struct ActiveJob {
    job: Job,
    workspace: Workspace,
}

impl ActiveJob {
    fn transition(&mut self, next: JobState) -> Result<(), JobError> {
        self.job
            .transition(next)
            .map_err(|_| JobError::InvalidTransition)
    }

    fn stop(mut self, error: JobError) -> JobReport {
        let next = if error == JobError::Cancelled {
            JobState::Cancelled
        } else {
            JobState::Failed
        };
        // Every non-terminal state may move to Failed or Cancelled.
        let _ = self.transition(next);
        self.finish(Outcome::Stopped(error))
    }

    fn finish(&mut self, outcome: Outcome) -> JobReport {
        let cleanup_error = match self.workspace.cleanup() {
            Ok(()) => {
                let _ = self.transition(JobState::Cleaned);
                None
            }
            Err(error) => Some(error.kind()),
        };
        JobReport {
            id: self.job.id.clone(),
            outcome,
            state: self.job.state,
            cleanup_error,
        }
    }
}

fn stop_without_workspace(mut job: Job, error: JobError) -> JobReport {
    let next = if error == JobError::Cancelled {
        JobState::Cancelled
    } else {
        JobState::Failed
    };
    let _ = job.transition(next);
    let _ = job.transition(JobState::Cleaned);
    JobReport {
        id: job.id,
        outcome: Outcome::Stopped(error),
        state: job.state,
        cleanup_error: None,
    }
}

fn engine_error(error: EngineError, max_output: u64) -> JobError {
    match error {
        EngineError::Interrupted(interruption) => interruption.into(),
        EngineError::OutputLimit => JobError::OutputTooLarge { limit: max_output },
        EngineError::Io(kind) => JobError::Io(kind),
        EngineError::Failed(reason) => JobError::EngineFailed(reason),
        EngineError::LimitExceeded(reason) => JobError::LimitExceeded(reason),
    }
}

/// Copies the input into the workspace, never opening the original for
/// writing. Returns the first [`HEADER_LEN`] bytes and the length.
fn stage(
    input: &Path,
    staged: &Path,
    max_bytes: u64,
    guard: &Guard<'_>,
) -> Result<(Vec<u8>, u64), JobError> {
    let io_error = |error: io::Error| JobError::Io(error.kind());
    if !fs::metadata(input).map_err(io_error)?.is_file() {
        return Err(JobError::InputNotAFile);
    }
    let mut source = File::open(input).map_err(io_error)?;
    let metadata = source.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(JobError::InputNotAFile);
    }
    if metadata.len() > max_bytes {
        return Err(JobError::InputTooLarge { limit: max_bytes });
    }
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staged)
        .map_err(io_error)?;

    let mut buffer = vec![0_u8; CHUNK];
    let mut header = Vec::with_capacity(HEADER_LEN);
    let mut total: u64 = 0;
    loop {
        guard.check()?;
        let read = source.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            return Ok((header, total));
        }
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        // The file may grow after the metadata check.
        if total > max_bytes {
            return Err(JobError::InputTooLarge { limit: max_bytes });
        }
        let chunk = buffer
            .get(..read)
            .ok_or(JobError::Io(io::ErrorKind::InvalidData))?;
        let wanted = HEADER_LEN.saturating_sub(header.len()).min(chunk.len());
        if let Some(head) = chunk.get(..wanted) {
            header.extend_from_slice(head);
        }
        target.write_all(chunk).map_err(io_error)?;
    }
}

/// Checks the output exists as a regular file within the output limit.
fn inspect_output(output: &Path, max_bytes: u64) -> Result<u64, JobError> {
    let metadata = match fs::symlink_metadata(output) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(JobError::OutputMissing)
        }
        Err(error) => return Err(JobError::Io(error.kind())),
    };
    if !metadata.file_type().is_file() {
        return Err(JobError::OutputRejected("the result is not a regular file"));
    }
    if metadata.len() > max_bytes {
        return Err(JobError::OutputTooLarge { limit: max_bytes });
    }
    Ok(metadata.len())
}
