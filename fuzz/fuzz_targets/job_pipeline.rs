//! Drives hostile files through the real job runner (ARCHITECTURE.md,
//! processing boundary). The last input byte picks what the stub engine does,
//! so the fuzzer explores every lifecycle path. Whatever happens, the original
//! is unchanged, export never overwrites, and no workspace survives
//! (PRODUCT_INVARIANTS.md 8, 9 and 10).
//!
//! The engine never panics on purpose: libfuzzer-sys aborts on any panic, so
//! panic recovery is covered by the core's integration tests instead.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paperwarden_core::engine::{EngineError, OutputVerifier, ProcessContext, Processor};
use paperwarden_core::validation::{MagicBytes, Rejection};
use paperwarden_core::{
    CancellationToken, JobError, JobId, JobLimits, JobRunner, JobState, Operation, Outcome,
};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

const SIGNATURE: &[u8] = b"%PDF-";
const MAX_INPUT: u64 = 4096;
const MAX_OUTPUT: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Copy,
    NoOutput,
    Oversized,
    EngineError,
    Cancel,
    Reverse,
}

impl Action {
    fn from_input(data: &[u8]) -> Self {
        match data.last().map_or(0, |byte| byte % 6) {
            0 => Self::Copy,
            1 => Self::NoOutput,
            2 => Self::Oversized,
            3 => Self::EngineError,
            4 => Self::Cancel,
            _ => Self::Reverse,
        }
    }
}

struct StubEngine {
    action: Action,
    cancel: CancellationToken,
}

impl Processor for StubEngine {
    fn process(&self, context: &mut ProcessContext<'_>) -> Result<(), EngineError> {
        let mut input = Vec::new();
        fs::File::open(context.input_path())?.read_to_end(&mut input)?;
        context.checkpoint()?;
        match self.action {
            Action::Copy => context.create_output()?.write_all(&input)?,
            Action::NoOutput => {}
            Action::Oversized => {
                let mut output = context.create_output()?;
                for _ in 0..=MAX_OUTPUT / 512 {
                    output.write_all(&[0x41; 512])?;
                }
            }
            Action::EngineError => return Err(EngineError::Failed("stub engine error")),
            Action::Cancel => {
                context.create_output()?.write_all(&input)?;
                self.cancel.cancel();
                context.checkpoint()?;
            }
            Action::Reverse => {
                input.reverse();
                context.create_output()?.write_all(&input)?;
            }
        }
        Ok(())
    }
}

struct SignatureVerifier;

impl OutputVerifier for SignatureVerifier {
    fn verify(&self, output: &Path, len: u64) -> Result<(), Rejection> {
        let bytes = fs::read(output).map_err(|_| Rejection("unreadable output"))?;
        if bytes.len() as u64 != len {
            return Err(Rejection("length changed"));
        }
        if bytes.starts_with(SIGNATURE) {
            Ok(())
        } else {
            Err(Rejection("missing signature"))
        }
    }
}

fn base() -> &'static PathBuf {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let base = std::env::temp_dir().join(format!("pw-fuzz-pipeline-{}", std::process::id()));
        fs::create_dir_all(&base).expect("create fuzz directory");
        base
    })
}

fn assert_no_workspaces(jobs: &Path) {
    if let Ok(entries) = fs::read_dir(jobs) {
        assert_eq!(entries.count(), 0, "a workspace survived the job");
    }
}

fuzz_target!(|data: &[u8]| {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let run = COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = base();
    let jobs = base.join("jobs");
    let input = base.join("original.pdf");
    fs::write(&input, data).expect("write input");

    let action = Action::from_input(data);
    let cancel = CancellationToken::new();
    let engine = StubEngine {
        action,
        cancel: cancel.clone(),
    };
    let validator = MagicBytes::new(0, SIGNATURE);
    let operation = Operation {
        validator: &validator,
        processor: &engine,
        verifier: &SignatureVerifier,
    };
    let limits = JobLimits::new(MAX_INPUT, MAX_OUTPUT, Duration::from_secs(30)).expect("limits");
    let runner = JobRunner::new(&jobs, limits);
    let id = JobId::new(format!("fuzz-{run}")).expect("valid id");

    let valid_input = data.starts_with(SIGNATURE) && data.len() as u64 <= MAX_INPUT;
    match runner.run(id, &input, &operation, &cancel) {
        Ok(mut ready) => {
            assert!(valid_input, "invalid input reached ReadyToExport");
            assert!(matches!(action, Action::Copy | Action::Reverse));
            assert_eq!(ready.state(), JobState::ReadyToExport);
            assert_eq!(ready.export_to(&input), Err(JobError::DestinationExists));

            let destination = base.join(format!("export-{run}.pdf"));
            ready.export_to(&destination).expect("export to a new file");
            let exported = fs::read(&destination).expect("read export");
            let mut expected = data.to_vec();
            if action == Action::Reverse {
                expected.reverse();
            }
            assert_eq!(exported, expected);
            fs::remove_file(&destination).expect("remove export");

            let report = ready.finish();
            assert_eq!(report.outcome, Outcome::Exported);
            assert_eq!(report.state, JobState::Cleaned);
        }
        Err(report) => {
            assert_eq!(report.state, JobState::Cleaned);
            assert_eq!(report.cleanup_error, None);
            let Outcome::Stopped(error) = report.outcome else {
                panic!("a stopped job reported {:?}", report.outcome);
            };
            if !valid_input {
                assert!(matches!(
                    error,
                    JobError::InputRejected(_) | JobError::InputTooLarge { .. }
                ));
            }
        }
    }

    assert_eq!(
        fs::read(&input).expect("read original"),
        data,
        "original changed"
    );
    assert_no_workspaces(&jobs);
});
