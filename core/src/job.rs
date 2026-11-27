//! Job identifiers and the lifecycle state machine.
//!
//! Every operation is a [`Job`] moving through [`JobState`]s. Success, failure
//! and cancellation all end in [`JobState::Cleaned`]
//! (`PRODUCT_INVARIANTS.md` 9). This module is pure logic and performs no I/O.

use std::path::PathBuf;

/// A validated job identifier.
///
/// Identifiers are non-empty ASCII alphanumerics and hyphens, so one can be
/// embedded in a workspace directory name without enabling path traversal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobId(String);

impl JobId {
    /// Validates `value` as a job identifier.
    ///
    /// # Errors
    ///
    /// Returns an error if `value` is empty or contains any character other
    /// than an ASCII alphanumeric or a hyphen.
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.is_empty()
            || !value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err("job id must be non-empty ASCII alphanumeric text or hyphens");
        }
        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A stage in the job lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobState {
    /// The user chose an input; nothing has been read yet.
    Selected,
    /// The input passed type and structure validation.
    Validated,
    /// An engine is transforming the input inside the workspace.
    Processing,
    /// The output is being checked before it may be exported.
    Verifying,
    /// A verified output awaits an explicit user export.
    ReadyToExport,
    /// The output was written to a destination the user chose.
    Exported,
    /// The job stopped because of an error; cleanup is still required.
    Failed,
    /// The user cancelled the job; cleanup is still required.
    Cancelled,
    /// The job's workspace has been removed. Terminal.
    Cleaned,
}

/// One operation on one input, tracked through its lifecycle.
#[derive(Debug, Eq, PartialEq)]
pub struct Job {
    /// Unique identifier, also used to name the job's workspace.
    pub id: JobId,
    /// The user-selected input. The core never overwrites it.
    pub input: PathBuf,
    /// The job's isolated temporary workspace directory.
    pub workspace: PathBuf,
    /// The current lifecycle stage.
    pub state: JobState,
}

impl Job {
    /// Creates a job in the [`JobState::Selected`] state.
    pub fn new(id: JobId, input: PathBuf, workspace: PathBuf) -> Self {
        Self {
            id,
            input,
            workspace,
            state: JobState::Selected,
        }
    }

    /// Moves the job to `next` if the lifecycle allows it.
    ///
    /// # Errors
    ///
    /// Returns an error, leaving the state unchanged, if `next` is not a
    /// permitted successor of the current state.
    pub fn transition(&mut self, next: JobState) -> Result<(), &'static str> {
        let allowed = matches!(
            (self.state, next),
            (JobState::Selected, JobState::Validated)
                | (JobState::Validated, JobState::Processing)
                | (JobState::Processing, JobState::Verifying)
                | (JobState::Verifying, JobState::ReadyToExport)
                | (JobState::ReadyToExport, JobState::Exported)
                | (JobState::Exported, JobState::Cleaned)
                | (JobState::Failed, JobState::Cleaned)
                | (JobState::Cancelled, JobState::Cleaned)
                | (JobState::Selected, JobState::Failed)
                | (JobState::Validated, JobState::Failed)
                | (JobState::Processing, JobState::Failed)
                | (JobState::Verifying, JobState::Failed)
                | (JobState::ReadyToExport, JobState::Failed)
                | (JobState::Selected, JobState::Cancelled)
                | (JobState::Validated, JobState::Cancelled)
                | (JobState::Processing, JobState::Cancelled)
                | (JobState::Verifying, JobState::Cancelled)
                | (JobState::ReadyToExport, JobState::Cancelled)
        );
        if !allowed {
            return Err("invalid job-state transition");
        }
        self.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> Job {
        Job::new(
            JobId::new("test-1").expect("valid id"),
            PathBuf::from("input.pdf"),
            PathBuf::from("workspace/test-1"),
        )
    }

    #[test]
    fn happy_path_ends_cleaned() {
        let mut job = job();
        for state in [
            JobState::Validated,
            JobState::Processing,
            JobState::Verifying,
            JobState::ReadyToExport,
            JobState::Exported,
            JobState::Cleaned,
        ] {
            job.transition(state).expect("valid transition");
        }
        assert_eq!(job.state, JobState::Cleaned);
    }

    #[test]
    fn failed_job_must_clean_before_any_other_transition() {
        let mut job = job();
        job.transition(JobState::Failed).expect("failure allowed");
        assert!(job.transition(JobState::Processing).is_err());
        job.transition(JobState::Cleaned).expect("cleanup allowed");
    }

    #[test]
    fn cancellation_can_be_cleaned() {
        let mut job = job();
        job.transition(JobState::Validated).expect("validated");
        job.transition(JobState::Cancelled).expect("cancelled");
        job.transition(JobState::Cleaned).expect("cleaned");
    }
}
