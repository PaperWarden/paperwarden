use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobId(String);

impl JobId {
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobState {
    Selected,
    Validated,
    Processing,
    Verifying,
    ReadyToExport,
    Exported,
    Failed,
    Cancelled,
    Cleaned,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Job {
    pub id: JobId,
    pub input: PathBuf,
    pub workspace: PathBuf,
    pub state: JobState,
}

impl Job {
    pub fn new(id: JobId, input: PathBuf, workspace: PathBuf) -> Self {
        Self {
            id,
            input,
            workspace,
            state: JobState::Selected,
        }
    }

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
