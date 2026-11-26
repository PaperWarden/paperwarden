//! The job lifecycle and its workspace, exercised through the public API only.

#[cfg(test)]
mod support;

use paperwarden_core::{Job, JobId, JobState, Workspace};
use std::fs;

#[test]
fn completed_job_leaves_no_workspace_behind() {
    let root = support::unique_root("completed");
    let id = JobId::new("complete-1").expect("valid id");
    let mut workspace = Workspace::create(&root, &id).expect("workspace created");
    let mut job = Job::new(id, root.join("input.pdf"), workspace.path().to_path_buf());
    fs::write(workspace.path().join("output.bin"), b"synthetic").expect("write output");

    for state in [
        JobState::Validated,
        JobState::Processing,
        JobState::Verifying,
        JobState::ReadyToExport,
        JobState::Exported,
    ] {
        job.transition(state).expect("valid transition");
    }
    workspace.cleanup().expect("workspace cleaned");
    job.transition(JobState::Cleaned)
        .expect("exported job may be cleaned");

    assert!(!job.workspace.exists());
    fs::remove_dir(&root).expect("only the job workspace was created in the root");
}

#[test]
fn failed_job_workspace_is_removed_when_dropped() {
    let root = support::unique_root("failed");
    let id = JobId::new("failed-1").expect("valid id");
    let mut job;
    {
        let workspace = Workspace::create(&root, &id).expect("workspace created");
        job = Job::new(id, root.join("input.pdf"), workspace.path().to_path_buf());
        fs::write(workspace.path().join("partial.bin"), b"partial").expect("write partial");
        job.transition(JobState::Validated).expect("validated");
        job.transition(JobState::Failed).expect("failure allowed");
    }

    assert!(!job.workspace.exists());
    job.transition(JobState::Cleaned)
        .expect("failed job may be cleaned");
    fs::remove_dir(&root).expect("only the job workspace was created in the root");
}

#[test]
fn restart_recovery_removes_a_workspace_abandoned_by_a_crash() {
    let root = support::unique_root("abandoned");
    let workspace = Workspace::create(&root, &JobId::new("abandoned-1").expect("valid id"))
        .expect("workspace created");
    let path = workspace.path().to_path_buf();
    fs::write(path.join("sensitive.bin"), b"residual").expect("write residual data");
    #[allow(
        clippy::mem_forget,
        reason = "simulates a process crash, where Drop never runs"
    )]
    std::mem::forget(workspace);
    assert!(path.exists(), "crash left the workspace behind");

    let removed = Workspace::recover_stale(&root).expect("recovery succeeds");

    assert_eq!(removed, 1);
    assert!(!path.exists());
    fs::remove_dir(&root).expect("recovery left the root empty");
}
