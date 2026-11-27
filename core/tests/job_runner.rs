//! End-to-end job lifecycle through `JobRunner`, using the public API only.
//!
//! Every test checks that the job's workspace is gone afterwards and that the
//! original input is unchanged (PRODUCT_INVARIANTS.md 8 and 9).

// Marks the whole file as test code, so clippy applies its test relaxations
// (clippy.toml) to the helpers too.
#![cfg(test)]

mod support;

use paperwarden_core::engine::{EngineError, ProcessContext};
use paperwarden_core::validation::{MagicBytes, Rejection};
use paperwarden_core::{
    CancellationToken, JobError, JobId, JobLimits, JobRunner, JobState, Operation, Outcome,
    ReadyJob, Workspace,
};
use paperwarden_core::{Inert, Notice};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::{FnProcessor, FnVerifier};

const SIGNATURE: &[u8] = b"%PWT-";
const VALID_INPUT: &[u8] = b"%PWT-hello";

struct Fixture {
    root: PathBuf,
    work_root: PathBuf,
    input: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = support::unique_root(name);
        fs::create_dir_all(&root).expect("create test root");
        // Named like a PDF but validated by content, never by name (invariant 10).
        let input = root.join("original.pdf");
        fs::write(&input, VALID_INPUT).expect("write input");
        Self {
            work_root: root.join("jobs"),
            root,
            input,
        }
    }

    fn runner(&self) -> JobRunner {
        self.runner_with(JobLimits::default())
    }

    fn runner_with(&self, limits: JobLimits) -> JobRunner {
        JobRunner::new(&self.work_root, limits)
    }

    /// Asserts the original is untouched and no workspace remains.
    fn assert_clean(&self) {
        assert_eq!(fs::read(&self.input).expect("read input"), VALID_INPUT);
        let leftovers: Vec<_> = fs::read_dir(&self.work_root)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        assert!(
            leftovers.is_empty(),
            "workspaces left behind: {leftovers:?}"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn id(value: &str) -> JobId {
    JobId::new(value).expect("valid id")
}

fn uppercase(context: &mut ProcessContext<'_>) -> Result<(), EngineError> {
    let mut input = Vec::new();
    fs::File::open(context.input_path())?.read_to_end(&mut input)?;
    context.checkpoint()?;
    context
        .create_output()?
        .write_all(&input.to_ascii_uppercase())?;
    Ok(())
}

fn accept(_: &Path, _: u64) -> Result<(), Rejection> {
    Ok(())
}

fn run(
    fixture: &Fixture,
    runner: &JobRunner,
    processor: &FnProcessor<impl Fn(&mut ProcessContext<'_>) -> Result<(), EngineError>>,
    verifier: &FnVerifier<impl Fn(&Path, u64) -> Result<(), Rejection>>,
    cancel: &CancellationToken,
) -> Result<ReadyJob, paperwarden_core::JobReport> {
    let validator = MagicBytes::new(0, SIGNATURE);
    let operation = Operation {
        validator: &validator,
        processor,
        verifier,
    };
    runner.run(id("job-1"), &fixture.input, &operation, cancel)
}

fn expect_stopped(
    result: Result<ReadyJob, paperwarden_core::JobReport>,
) -> paperwarden_core::JobReport {
    match result {
        Ok(_) => panic!("job unexpectedly became ready"),
        Err(report) => report,
    }
}

// --- Success and export ------------------------------------------------------

#[test]
fn successful_job_exports_a_new_file_and_cleans_up() {
    let fixture = Fixture::new("success");
    let mut ready = run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    )
    .expect("job ready");
    assert_eq!(ready.state(), JobState::ReadyToExport);
    assert_eq!(ready.output_len(), VALID_INPUT.len() as u64);

    let destination = fixture.root.join("exported.pdf");
    let written = ready.export_to(&destination).expect("export succeeds");
    assert_eq!(written, VALID_INPUT.len() as u64);
    assert_eq!(ready.state(), JobState::Exported);

    let report = ready.finish();
    assert_eq!(report.outcome, Outcome::Exported);
    assert_eq!(report.state, JobState::Cleaned);
    assert_eq!(report.cleanup_error, None);
    assert_eq!(
        fs::read(&destination).expect("read export"),
        b"%PWT-HELLO".to_vec()
    );
    fixture.assert_clean();
}

#[test]
fn export_never_overwrites_an_existing_file_or_the_original() {
    let fixture = Fixture::new("no-overwrite");
    let mut ready = run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    )
    .expect("job ready");

    assert_eq!(
        ready.export_to(&fixture.input),
        Err(JobError::DestinationExists)
    );
    let other = fixture.root.join("existing.pdf");
    fs::write(&other, b"keep me").expect("write existing file");
    assert_eq!(ready.export_to(&other), Err(JobError::DestinationExists));
    assert_eq!(
        fs::read(&other).expect("read existing"),
        b"keep me".to_vec()
    );

    // A failed export leaves the job ready for another destination.
    assert_eq!(ready.state(), JobState::ReadyToExport);
    ready
        .export_to(&fixture.root.join("new.pdf"))
        .expect("export to a new path");
    assert_eq!(
        ready.export_to(&fixture.root.join("again.pdf")),
        Err(JobError::AlreadyExported)
    );
    ready.finish();
    fixture.assert_clean();
}

#[test]
fn export_into_a_missing_directory_fails_without_side_effects() {
    let fixture = Fixture::new("missing-dir");
    let mut ready = run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    )
    .expect("job ready");
    let destination = fixture.root.join("absent").join("out.pdf");
    assert!(matches!(
        ready.export_to(&destination),
        Err(JobError::Io(_))
    ));
    assert!(!destination.exists());
    ready.finish();
    fixture.assert_clean();
}

#[test]
fn unexported_job_is_discarded_and_cleaned() {
    let fixture = Fixture::new("discard");
    let ready = run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    )
    .expect("job ready");
    let report = ready.finish();
    assert_eq!(report.outcome, Outcome::Discarded);
    assert_eq!(report.state, JobState::Cleaned);
    fixture.assert_clean();
}

#[test]
fn dropping_a_ready_job_removes_its_workspace() {
    let fixture = Fixture::new("drop-ready");
    let ready = run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    )
    .expect("job ready");
    drop(ready);
    fixture.assert_clean();
}

#[test]
fn processor_works_on_a_staged_copy_never_the_original() {
    let fixture = Fixture::new("staged");
    let processor = FnProcessor(|context: &mut ProcessContext<'_>| {
        // Even a processor that writes to its input cannot reach the original.
        fs::write(context.input_path(), b"clobbered")?;
        context.create_output()?.write_all(b"%PWT-out")?;
        Ok(())
    });
    let ready = run(
        &fixture,
        &fixture.runner(),
        &processor,
        &FnVerifier(accept),
        &CancellationToken::new(),
    )
    .expect("job ready");
    ready.finish();
    fixture.assert_clean();
}

// --- Validation and input bounds ---------------------------------------------

#[test]
fn input_with_wrong_magic_bytes_is_rejected_despite_its_name() {
    let fixture = Fixture::new("bad-magic");
    fs::write(&fixture.input, b"MZ not a document").expect("write input");
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert!(matches!(
        report.outcome,
        Outcome::Stopped(JobError::InputRejected(_))
    ));
    assert_eq!(report.state, JobState::Cleaned);
    fs::write(&fixture.input, VALID_INPUT).expect("restore input");
    fixture.assert_clean();
}

#[test]
fn input_over_the_size_limit_is_rejected() {
    let fixture = Fixture::new("too-large");
    let limits = JobLimits::new(4, 1024, Duration::from_secs(60)).expect("valid limits");
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner_with(limits),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert_eq!(
        report.outcome,
        Outcome::Stopped(JobError::InputTooLarge { limit: 4 })
    );
    fixture.assert_clean();
}

#[test]
fn directory_input_is_rejected() {
    let fixture = Fixture::new("dir-input");
    let validator = MagicBytes::new(0, SIGNATURE);
    let operation = Operation {
        validator: &validator,
        processor: &FnProcessor(uppercase),
        verifier: &FnVerifier(accept),
    };
    let report = expect_stopped(fixture.runner().run(
        id("dir-1"),
        &fixture.root,
        &operation,
        &CancellationToken::new(),
    ));
    assert_eq!(report.outcome, Outcome::Stopped(JobError::InputNotAFile));
    fixture.assert_clean();
}

#[test]
fn missing_input_fails_cleanly() {
    let fixture = Fixture::new("missing-input");
    let validator = MagicBytes::new(0, SIGNATURE);
    let operation = Operation {
        validator: &validator,
        processor: &FnProcessor(uppercase),
        verifier: &FnVerifier(accept),
    };
    let report = expect_stopped(fixture.runner().run(
        id("missing-1"),
        &fixture.root.join("absent.pdf"),
        &operation,
        &CancellationToken::new(),
    ));
    assert!(matches!(report.outcome, Outcome::Stopped(JobError::Io(_))));
    fixture.assert_clean();
}

// --- Processing bounds -------------------------------------------------------

#[test]
fn output_over_the_size_limit_fails_the_job() {
    let fixture = Fixture::new("output-limit");
    let limits = JobLimits::new(1024, 8, Duration::from_secs(60)).expect("valid limits");
    let processor = FnProcessor(|context: &mut ProcessContext<'_>| {
        context.create_output()?.write_all(&[0_u8; 64])?;
        Ok(())
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner_with(limits),
        &processor,
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert_eq!(
        report.outcome,
        Outcome::Stopped(JobError::OutputTooLarge { limit: 8 })
    );
    fixture.assert_clean();
}

#[test]
fn processor_that_writes_no_output_fails_the_job() {
    let fixture = Fixture::new("no-output");
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(|_: &mut ProcessContext<'_>| Ok(())),
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert_eq!(report.outcome, Outcome::Stopped(JobError::OutputMissing));
    fixture.assert_clean();
}

#[test]
fn engine_error_fails_the_job() {
    let fixture = Fixture::new("engine-error");
    let processor = FnProcessor(|_: &mut ProcessContext<'_>| {
        Err(EngineError::Failed("malformed cross-reference table"))
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &processor,
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert_eq!(
        report.outcome,
        Outcome::Stopped(JobError::EngineFailed("malformed cross-reference table"))
    );
    assert_eq!(report.state, JobState::Cleaned);
    fixture.assert_clean();
}

#[test]
fn notices_travel_with_the_result_sorted_and_once_each() {
    let fixture = Fixture::new("notices");
    let processor = FnProcessor(|context: &mut ProcessContext<'_>| {
        context.notice(Notice::NotRun(Inert::WebLinks));
        context.notice(Notice::Repaired);
        context.notice(Notice::NotRun(Inert::Scripts));
        context.notice(Notice::Repaired);
        uppercase(context)
    });
    let ready = run(
        &fixture,
        &fixture.runner(),
        &processor,
        &FnVerifier(accept),
        &CancellationToken::new(),
    )
    .expect("job ready");
    let expected = vec![
        Notice::Repaired,
        Notice::NotRun(Inert::Scripts),
        Notice::NotRun(Inert::WebLinks),
    ];
    assert_eq!(ready.notices(), expected.as_slice());
    let report = ready.finish();
    assert_eq!(report.notices, expected);
    fixture.assert_clean();
}

#[test]
fn a_failed_job_reports_no_notices() {
    let fixture = Fixture::new("notices-failed");
    let processor = FnProcessor(|context: &mut ProcessContext<'_>| {
        context.notice(Notice::Repaired);
        Err(EngineError::Failed("the PDF could not be opened"))
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &processor,
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert!(report.notices.is_empty(), "{:?}", report.notices);
    fixture.assert_clean();
}

#[test]
fn engine_limit_is_a_typed_limit_error() {
    let fixture = Fixture::new("engine-limit");
    let processor = FnProcessor(|context: &mut ProcessContext<'_>| {
        context.create_output()?.write_all(b"partial")?;
        Err(EngineError::LimitExceeded(
            "an image in the PDF is too large",
        ))
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &processor,
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert_eq!(
        report.outcome,
        Outcome::Stopped(JobError::LimitExceeded("an image in the PDF is too large"))
    );
    assert_eq!(report.state, JobState::Cleaned);
    fixture.assert_clean();
}

#[test]
fn processor_panic_fails_the_job_and_cleans_up() {
    let fixture = Fixture::new("panic");
    let processor = FnProcessor(|context: &mut ProcessContext<'_>| {
        context.create_output()?.write_all(b"partial")?;
        panic!("engine bug triggered by hostile input");
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &processor,
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert_eq!(report.outcome, Outcome::Stopped(JobError::EnginePanicked));
    assert_eq!(report.state, JobState::Cleaned);
    fixture.assert_clean();
}

#[test]
fn job_past_its_time_limit_times_out() {
    let fixture = Fixture::new("timeout");
    let limits = JobLimits::new(1024, 1024, Duration::from_millis(20)).expect("valid limits");
    let processor = FnProcessor(|context: &mut ProcessContext<'_>| {
        std::thread::sleep(Duration::from_millis(60));
        context.checkpoint()?;
        Ok(())
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner_with(limits),
        &processor,
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert_eq!(report.outcome, Outcome::Stopped(JobError::TimedOut));
    fixture.assert_clean();
}

// --- Verification ------------------------------------------------------------

#[test]
fn rejected_output_is_never_exportable() {
    let fixture = Fixture::new("verify-reject");
    let verifier = FnVerifier(|output: &Path, _len: u64| {
        let bytes = fs::read(output).map_err(|_| Rejection("unreadable output"))?;
        if bytes.starts_with(SIGNATURE) {
            Ok(())
        } else {
            Err(Rejection("output lacks the expected signature"))
        }
    });
    let processor = FnProcessor(|context: &mut ProcessContext<'_>| {
        context.create_output()?.write_all(b"garbage")?;
        Ok(())
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &processor,
        &verifier,
        &CancellationToken::new(),
    ));
    assert_eq!(
        report.outcome,
        Outcome::Stopped(JobError::OutputRejected(
            "output lacks the expected signature"
        ))
    );
    fixture.assert_clean();
}

// --- Cancellation ------------------------------------------------------------

#[test]
fn job_cancelled_before_start_creates_no_workspace() {
    let fixture = Fixture::new("cancel-early");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &cancel,
    ));
    assert_eq!(report.outcome, Outcome::Stopped(JobError::Cancelled));
    assert_eq!(report.state, JobState::Cleaned);
    assert!(!fixture.work_root.exists());
    fixture.assert_clean();
}

#[test]
fn job_cancelled_during_processing_stops_and_cleans_up() {
    let fixture = Fixture::new("cancel-mid");
    let cancel = CancellationToken::new();
    let handle = cancel.clone();
    let processor = FnProcessor(move |context: &mut ProcessContext<'_>| {
        context.create_output()?.write_all(b"%PWT-partial")?;
        handle.cancel(); // the user taps Cancel mid-job
        context.checkpoint()?;
        Ok(())
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &processor,
        &FnVerifier(accept),
        &cancel,
    ));
    assert_eq!(report.outcome, Outcome::Stopped(JobError::Cancelled));
    assert_eq!(report.state, JobState::Cleaned);
    fixture.assert_clean();
}

#[test]
fn processor_that_ignores_cancellation_is_still_stopped_before_verification() {
    let fixture = Fixture::new("cancel-ignored");
    let cancel = CancellationToken::new();
    let handle = cancel.clone();
    let processor = FnProcessor(move |context: &mut ProcessContext<'_>| {
        context.create_output()?.write_all(b"%PWT-done")?;
        handle.cancel();
        Ok(()) // never checks the token
    });
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &processor,
        &FnVerifier(accept),
        &cancel,
    ));
    assert_eq!(report.outcome, Outcome::Stopped(JobError::Cancelled));
    fixture.assert_clean();
}

// --- Workspaces and recovery -------------------------------------------------

#[test]
fn duplicate_job_id_fails_without_touching_the_running_job() {
    let fixture = Fixture::new("duplicate");
    let first = run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    )
    .expect("first job ready");
    let report = expect_stopped(run(
        &fixture,
        &fixture.runner(),
        &FnProcessor(uppercase),
        &FnVerifier(accept),
        &CancellationToken::new(),
    ));
    assert!(matches!(report.outcome, Outcome::Stopped(JobError::Io(_))));
    assert_eq!(first.state(), JobState::ReadyToExport);
    assert!(first.output_len() > 0);
    first.finish();
    fixture.assert_clean();
}

#[test]
fn recovery_removes_crashed_workspaces_and_is_idempotent() {
    let fixture = Fixture::new("recovery");
    let runner = fixture.runner();
    let workspace = Workspace::create(&fixture.work_root, &id("crashed-1")).expect("workspace");
    fs::write(workspace.input_dir().join("input"), b"residual document").expect("write residue");
    #[allow(
        clippy::mem_forget,
        reason = "simulates a process crash, where Drop never runs"
    )]
    std::mem::forget(workspace);
    fs::create_dir_all(fixture.work_root.join("unrelated")).expect("unrelated dir");
    fs::write(fixture.work_root.join("pw-not-a-dir"), b"file").expect("unrelated file");

    let first = runner.recover().expect("recovery runs");
    assert_eq!((first.removed, first.failed), (1, 0));
    let second = runner.recover().expect("recovery runs again");
    assert_eq!((second.removed, second.failed), (0, 0));

    assert!(fixture.work_root.join("unrelated").is_dir());
    assert!(fixture.work_root.join("pw-not-a-dir").is_file());
    fs::remove_dir(fixture.work_root.join("unrelated")).expect("remove unrelated");
    fs::remove_file(fixture.work_root.join("pw-not-a-dir")).expect("remove file");
    fixture.assert_clean();
}

#[test]
fn recovery_of_a_missing_root_does_nothing() {
    let fixture = Fixture::new("recovery-missing");
    let report = fixture.runner().recover().expect("recovery runs");
    assert_eq!((report.removed, report.failed), (0, 0));
}

#[test]
fn relative_workspace_root_is_rejected() {
    let fixture = Fixture::new("relative-root");
    let runner = JobRunner::new("relative-jobs", JobLimits::default());
    let validator = MagicBytes::new(0, SIGNATURE);
    let operation = Operation {
        validator: &validator,
        processor: &FnProcessor(uppercase),
        verifier: &FnVerifier(accept),
    };
    let report = expect_stopped(runner.run(
        id("rel-1"),
        &fixture.input,
        &operation,
        &CancellationToken::new(),
    ));
    assert!(matches!(report.outcome, Outcome::Stopped(JobError::Io(_))));
    assert!(!Path::new("relative-jobs").exists());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn workspaces_are_private_to_the_app() {
        let fixture = Fixture::new("private");
        let workspace = Workspace::create(&fixture.work_root, &id("private-1")).expect("workspace");
        let mode = fs::metadata(workspace.path())
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "workspace readable by others: {mode:o}");
    }

    #[test]
    fn symlinked_workspace_root_is_rejected() {
        let fixture = Fixture::new("root-link");
        let target = fixture.root.join("elsewhere");
        fs::create_dir_all(&target).expect("target");
        symlink(&target, &fixture.work_root).expect("symlink root");
        assert!(Workspace::create(&fixture.work_root, &id("link-1")).is_err());
        assert!(Workspace::recover_stale(&fixture.work_root).is_err());
        assert_eq!(fs::read_dir(&target).expect("read target").count(), 0);
    }

    #[test]
    fn recovery_does_not_follow_links_out_of_the_root() {
        let fixture = Fixture::new("recovery-link");
        let outside = fixture.root.join("outside");
        fs::create_dir_all(&outside).expect("outside dir");
        fs::write(outside.join("keep.txt"), b"keep").expect("outside file");
        fs::create_dir_all(&fixture.work_root).expect("work root");
        symlink(&outside, fixture.work_root.join("pw-link")).expect("link named like a workspace");

        let crashed = fixture.work_root.join("pw-crashed");
        fs::create_dir_all(&crashed).expect("crashed workspace");
        symlink(&outside, crashed.join("escape")).expect("link inside a workspace");

        let report = Workspace::recover_stale(&fixture.work_root).expect("recovery");
        assert_eq!(report.removed, 1);
        assert!(!crashed.exists());
        assert!(
            outside.join("keep.txt").is_file(),
            "recovery followed a link"
        );
        fs::remove_file(fixture.work_root.join("pw-link")).expect("remove link");
    }
}
