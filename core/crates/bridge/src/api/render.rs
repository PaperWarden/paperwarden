//! Rendering a PDF with PDFium in another process (#59, ADR-0007).
//!
//! The job runs here, in the app process, as any other job: the input is
//! staged into an isolated workspace, validated, pre-checked, and the output
//! is verified before export. Only the PDFium step leaves the process. The
//! processor hands the staged copy and the empty output to `engine`, a Dart
//! callback that passes them to the platform's isolated render service, and
//! waits for its answer. On Android that is a service in its own
//! `android:isolatedProcess`, so the app never loads PDFium.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use flutter_rust_bridge::DartFnFuture;
use paperwarden_core::engine::{EngineError, Interruption, ProcessContext, Processor};
use paperwarden_core::validation::MagicBytes;
use paperwarden_core::{JobError, JobId, JobLimits, JobReport, JobRunner, Operation, Outcome};
use paperwarden_pdf::service::{ServiceStatus, DEFAULT_MEMORY_BUDGET, MAX_TIME_LIMIT};
use paperwarden_pdf::{precheck, PpmVerifier, MAX_RENDER_PIXELS};
use paperwarden_pdf_scan::ScanLimits;

use super::document::{document_notice, DocumentNotice};
use super::probe::{BridgeError, BridgeErrorKind, CancelHandle};

/// What the render service is asked to do. Paths are inside the job's
/// workspace: the staged copy, never the original, and an output file that
/// already exists and is empty.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineRequest {
    /// The staged copy of the input.
    pub input: String,
    /// The job's output file.
    pub output: String,
    /// Render box width in pixels.
    pub max_width: u32,
    /// Render box height in pixels.
    pub max_height: u32,
    /// The job's input limit.
    pub max_input_bytes: u64,
    /// The job's output limit.
    pub max_output_bytes: u64,
    /// The time left in the job, in milliseconds; at least 1.
    pub time_limit_ms: u64,
    /// Memory the render may use above the service's starting size.
    pub memory_budget_bytes: u64,
}

/// How the render service's work ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineEnd {
    /// The service answered with a status.
    Finished,
    /// The service did not answer in time and was stopped.
    TimedOut,
    /// The service process died before answering: PDFium crashed or hit the
    /// memory ceiling.
    Died,
    /// The service could not be started.
    Unavailable,
}

/// How the render service answered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineOutcome {
    /// How the work ended.
    pub end: EngineEnd,
    /// The service's `ServiceStatus` code when `end` is
    /// [`EngineEnd::Finished`]; ignored otherwise.
    pub status: i32,
}

/// A rendered document's first page, saved as a PPM image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedPdf {
    /// Bytes written to the destination.
    pub bytes: u64,
    /// What the user must be told about the document.
    pub notices: Vec<DocumentNotice>,
}

/// Renders the PDF at `path` inside `max_width` x `max_height` pixels through
/// the job runner, with PDFium run by `engine`, and saves the first page as a
/// PPM image at `destination`, which must not exist.
///
/// `jobs_dir` is the directory for job workspaces. Not `sync`: the call waits
/// for the service on a worker thread.
///
/// # Errors
///
/// - [`BridgeErrorKind::InvalidInput`] for an empty or oversized render box,
///   or an existing destination.
/// - [`BridgeErrorKind::Unreadable`] if the file cannot be read.
/// - [`BridgeErrorKind::Refused`] if the pre-check or a limit refuses it.
/// - [`BridgeErrorKind::TimedOut`] past the job's time limit.
/// - [`BridgeErrorKind::Cancelled`] if `cancel` is cancelled.
/// - [`BridgeErrorKind::EngineFailed`] if the render failed, crashed, or
///   produced output that did not verify.
pub fn render_pdf(
    path: String,
    jobs_dir: String,
    destination: String,
    max_width: u32,
    max_height: u32,
    cancel: &CancelHandle,
    engine: impl Fn(EngineRequest) -> DartFnFuture<EngineOutcome>,
) -> Result<RenderedPdf, BridgeError> {
    render_with(
        &path,
        &jobs_dir,
        &destination,
        (max_width, max_height),
        cancel,
        JobLimits::default(),
        |request| futures_executor::block_on(engine(request)),
    )
}

static NEXT_JOB: AtomicU64 = AtomicU64::new(0);

fn job_id() -> Result<JobId, BridgeError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let sequence = NEXT_JOB.fetch_add(1, Ordering::Relaxed);
    JobId::new(format!("render-{nanos}-{sequence}")).map_err(BridgeError::invalid_input)
}

fn render_with(
    path: &str,
    jobs_dir: &str,
    destination: &str,
    (max_width, max_height): (u32, u32),
    cancel: &CancelHandle,
    limits: JobLimits,
    engine: impl Fn(EngineRequest) -> EngineOutcome,
) -> Result<RenderedPdf, BridgeError> {
    if max_width == 0
        || max_height == 0
        || u64::from(max_width) * u64::from(max_height) > MAX_RENDER_PIXELS
    {
        return Err(BridgeError::invalid_input(
            "the render box is empty or too large",
        ));
    }
    let processor = ServiceRender {
        engine,
        max_width,
        max_height,
        limits,
    };
    let validator = MagicBytes::new(0, b"%PDF-");
    let operation = Operation {
        validator: &validator,
        processor: &processor,
        verifier: &PpmVerifier,
    };
    let runner = JobRunner::new(jobs_dir, limits);
    let mut ready = runner
        .run(job_id()?, Path::new(path), &operation, cancel.token())
        .map_err(|report| report_error(&report))?;
    let exported = ready.export_to(Path::new(destination));
    let report = ready.finish();
    let bytes = exported.map_err(job_error)?;
    Ok(RenderedPdf {
        bytes,
        notices: report.notices.into_iter().map(document_notice).collect(),
    })
}

struct ServiceRender<F> {
    engine: F,
    max_width: u32,
    max_height: u32,
    limits: JobLimits,
}

impl<F: Fn(EngineRequest) -> EngineOutcome> Processor for ServiceRender<F> {
    fn process(&self, context: &mut ProcessContext<'_>) -> Result<(), EngineError> {
        // The pre-check runs here, in the app process: it is PaperWarden's own
        // bounded code, and the service only sees documents that pass it.
        let mut bytes = Vec::new();
        File::open(context.input_path())?
            .take(context.input_len())
            .read_to_end(&mut bytes)?;
        let report = precheck(&bytes, &ScanLimits::default(), || context.checkpoint())?;
        drop(bytes);
        for notice in report.notices() {
            context.notice(notice);
        }

        drop(context.create_output()?);
        let time_limit = context
            .remaining_time()
            .unwrap_or_else(|| self.limits.max_duration())
            .min(MAX_TIME_LIMIT);
        if time_limit.is_zero() {
            return Err(Interruption::TimedOut.into());
        }
        let (Some(input), Some(output)) = (
            context.input_path().to_str(),
            context.output_path().to_str(),
        ) else {
            return Err(EngineError::Failed(
                "the workspace path cannot be passed on",
            ));
        };
        let request = EngineRequest {
            input: input.to_owned(),
            output: output.to_owned(),
            max_width: self.max_width,
            max_height: self.max_height,
            max_input_bytes: self.limits.max_input_bytes(),
            max_output_bytes: self.limits.max_output_bytes(),
            time_limit_ms: u64::try_from(time_limit.as_millis())
                .unwrap_or(u64::MAX)
                .max(1),
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET,
        };
        outcome_result((self.engine)(request))?;
        context.checkpoint()?;
        Ok(())
    }
}

fn outcome_result(outcome: EngineOutcome) -> Result<(), EngineError> {
    let status = match outcome.end {
        EngineEnd::Finished => ServiceStatus::from_code(outcome.status),
        EngineEnd::TimedOut => return Err(Interruption::TimedOut.into()),
        EngineEnd::Died => return Err(EngineError::Failed("the PDF engine stopped")),
        EngineEnd::Unavailable => {
            return Err(EngineError::Failed("the PDF engine could not be started"))
        }
    };
    Err(match status {
        Some(ServiceStatus::Rendered) => return Ok(()),
        Some(ServiceStatus::TimedOut) => Interruption::TimedOut.into(),
        Some(ServiceStatus::OutputTooLarge) => EngineError::OutputLimit,
        Some(ServiceStatus::InputTooLarge) => EngineError::LimitExceeded("the file is too large"),
        Some(ServiceStatus::TooManyPages) => {
            EngineError::LimitExceeded("the PDF has too many pages")
        }
        Some(ServiceStatus::OpenFailed) => EngineError::Failed("the PDF could not be opened"),
        Some(ServiceStatus::NoPages) => EngineError::Failed("the PDF has no pages"),
        Some(ServiceStatus::PageFailed) => EngineError::Failed("a page could not be rendered"),
        Some(
            ServiceStatus::InvalidRequest
            | ServiceStatus::AlreadyUsed
            | ServiceStatus::Unconfined
            | ServiceStatus::LibraryUnavailable
            | ServiceStatus::Unreadable
            | ServiceStatus::WriteFailed,
        ) => EngineError::Failed("the PDF engine could not run"),
        None => EngineError::Failed("the PDF engine gave an unknown answer"),
    })
}

fn report_error(report: &JobReport) -> BridgeError {
    match report.outcome {
        Outcome::Stopped(error) => job_error(error),
        Outcome::Exported | Outcome::Discarded => BridgeError {
            kind: BridgeErrorKind::EngineFailed,
            message: "the job ended without a result".to_owned(),
        },
    }
}

fn job_error(error: JobError) -> BridgeError {
    let kind = match error {
        JobError::Cancelled => BridgeErrorKind::Cancelled,
        JobError::TimedOut => BridgeErrorKind::TimedOut,
        JobError::InputNotAFile | JobError::Io(_) => BridgeErrorKind::Unreadable,
        JobError::InputTooLarge { .. }
        | JobError::InputRejected(_)
        | JobError::LimitExceeded(_) => BridgeErrorKind::Refused,
        JobError::DestinationExists | JobError::AlreadyExported => BridgeErrorKind::InvalidInput,
        JobError::EngineFailed(_)
        | JobError::EnginePanicked
        | JobError::OutputTooLarge { .. }
        | JobError::OutputMissing
        | JobError::OutputRejected(_)
        | JobError::InvalidTransition => BridgeErrorKind::EngineFailed,
    };
    BridgeError {
        kind,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    struct Dirs {
        root: PathBuf,
    }

    impl Dirs {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "pw-render-{name}-{}-{}",
                std::process::id(),
                NEXT_JOB.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn jobs(&self) -> String {
            self.root.join("jobs").to_string_lossy().into_owned()
        }

        fn destination(&self) -> String {
            self.root.join("page.ppm").to_string_lossy().into_owned()
        }

        fn assert_no_workspace(&self) {
            let left: Vec<_> = fs::read_dir(self.root.join("jobs"))
                .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
                .unwrap_or_default();
            assert!(left.is_empty(), "workspaces left behind: {left:?}");
        }
    }

    impl Drop for Dirs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn corpus(relative: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/corpus")
            .join(relative)
            .to_string_lossy()
            .into_owned()
    }

    fn limits() -> JobLimits {
        JobLimits::new(64 << 20, 64 << 20, Duration::from_secs(30)).unwrap()
    }

    fn render(
        dirs: &Dirs,
        input: &str,
        engine: impl Fn(EngineRequest) -> EngineOutcome,
    ) -> Result<RenderedPdf, BridgeError> {
        render_with(
            input,
            &dirs.jobs(),
            &dirs.destination(),
            (100, 100),
            &CancelHandle::new(),
            limits(),
            engine,
        )
    }

    fn finished(status: i32) -> EngineOutcome {
        EngineOutcome {
            end: EngineEnd::Finished,
            status,
        }
    }

    fn ended(end: EngineEnd) -> EngineOutcome {
        EngineOutcome { end, status: -1 }
    }

    const PPM: &[u8] = b"P6\n1 1\n255\n\xff\x00\x00";

    #[test]
    fn the_service_gets_the_staged_copy_and_its_page_is_exported() {
        let dirs = Dirs::new("ok");
        let original = corpus("valid/pdf/one-page.pdf");
        let seen = RefCell::new(None);
        let rendered = render(&dirs, &original, |request| {
            assert_eq!(
                fs::read(&request.input).unwrap(),
                fs::read(&original).unwrap()
            );
            assert!(request.input.starts_with(&dirs.jobs()), "{}", request.input);
            assert!(
                request.output.starts_with(&dirs.jobs()),
                "{}",
                request.output
            );
            assert_eq!(fs::metadata(&request.output).unwrap().len(), 0);
            fs::write(&request.output, PPM).unwrap();
            *seen.borrow_mut() = Some(request);
            finished(ServiceStatus::Rendered.code())
        })
        .unwrap();

        assert_eq!(rendered.bytes, PPM.len() as u64);
        assert_eq!(fs::read(dirs.destination()).unwrap(), PPM);
        let request = seen.into_inner().unwrap();
        assert_eq!((request.max_width, request.max_height), (100, 100));
        assert_eq!(request.max_input_bytes, 64 << 20);
        assert_eq!(request.max_output_bytes, 64 << 20);
        assert!((1..=30_000).contains(&request.time_limit_ms));
        assert_eq!(request.memory_budget_bytes, DEFAULT_MEMORY_BUDGET);
        dirs.assert_no_workspace();
    }

    #[test]
    fn pre_check_notices_travel_with_the_result() {
        let dirs = Dirs::new("notices");
        let rendered = render(
            &dirs,
            &corpus("security/pdf/javascript-openaction.pdf"),
            |r| {
                fs::write(&r.output, PPM).unwrap();
                finished(0)
            },
        )
        .unwrap();
        assert!(rendered.notices.contains(&DocumentNotice::Scripts));
        dirs.assert_no_workspace();
    }

    #[test]
    fn a_document_the_pre_check_refuses_never_reaches_the_service() {
        let dirs = Dirs::new("bomb");
        let error = render(&dirs, &corpus("security/pdf/flate-bomb.pdf"), |_| {
            panic!("the service was asked to render a bomb")
        })
        .unwrap_err();
        assert_eq!(error.kind, BridgeErrorKind::Refused);
        dirs.assert_no_workspace();
    }

    #[test]
    fn a_service_that_dies_or_times_out_is_a_typed_error_and_leaves_nothing() {
        let cases = [
            (ended(EngineEnd::Died), BridgeErrorKind::EngineFailed),
            (ended(EngineEnd::Unavailable), BridgeErrorKind::EngineFailed),
            (ended(EngineEnd::TimedOut), BridgeErrorKind::TimedOut),
            (
                finished(ServiceStatus::TimedOut.code()),
                BridgeErrorKind::TimedOut,
            ),
            (
                finished(ServiceStatus::PageFailed.code()),
                BridgeErrorKind::EngineFailed,
            ),
            (finished(99), BridgeErrorKind::EngineFailed),
            (
                finished(ServiceStatus::TooManyPages.code()),
                BridgeErrorKind::Refused,
            ),
        ];
        for (outcome, kind) in cases {
            let dirs = Dirs::new("fails");
            let error = render(&dirs, &corpus("valid/pdf/one-page.pdf"), |request| {
                // Even a complete, valid image from a failed service is
                // never exported: only its answer decides.
                fs::write(&request.output, PPM).unwrap();
                outcome
            })
            .unwrap_err();
            assert_eq!(error.kind, kind, "{outcome:?}");
            assert!(!Path::new(&dirs.destination()).exists());
            dirs.assert_no_workspace();
        }
    }

    #[test]
    fn output_that_is_not_a_valid_image_is_never_exported() {
        let dirs = Dirs::new("garbage");
        let error = render(&dirs, &corpus("valid/pdf/one-page.pdf"), |request| {
            fs::write(&request.output, b"P6\n4000 4000\n255\nshort").unwrap();
            finished(0)
        })
        .unwrap_err();
        assert_eq!(error.kind, BridgeErrorKind::EngineFailed);
        assert!(!Path::new(&dirs.destination()).exists());
        dirs.assert_no_workspace();
    }

    #[test]
    fn a_render_box_out_of_bounds_is_invalid() {
        let dirs = Dirs::new("box");
        for size in [(0, 100), (100, 0), (4097, 4096)] {
            let error = render_with(
                &corpus("valid/pdf/one-page.pdf"),
                &dirs.jobs(),
                &dirs.destination(),
                size,
                &CancelHandle::new(),
                limits(),
                |_| panic!("rendered with an invalid box"),
            )
            .unwrap_err();
            assert_eq!(error.kind, BridgeErrorKind::InvalidInput);
        }
    }
}
