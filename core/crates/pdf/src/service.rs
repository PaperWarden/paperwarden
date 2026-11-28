//! PDFium in a disposable, confined process (#59, ADR-0007).
//!
//! PW-010 showed that PDFium inside the app process cannot be held to
//! PaperWarden's bounds: it inflated a Flate bomb in full, and its own output
//! cap is 1 GiB. On Android the app therefore never loads PDFium. A service
//! declared with `android:isolatedProcess` (no permissions, no network, no
//! access to the app's files) is handed the staged input and the job's output
//! as file descriptors and calls [`serve`], which:
//!
//! 1. starts a watchdog that kills the process at the time limit, so a page
//!    that never finishes cannot hold the job;
//! 2. caps the process's address space at its current size plus a memory
//!    budget, so an allocation PDFium cannot bound fails instead of growing;
//! 3. renders like [`RenderProcessor`](crate::RenderProcessor), writing the
//!    first page as a PPM, within the output limit.
//!
//! Neither limit can be lifted again and PDFium cannot be unloaded, so one
//! process serves one job: a second call is refused, and the app discards
//! the process after each job. If PDFium crashes or the watchdog fires, only
//! the service dies; the app's job runner reports a typed error and removes
//! the workspace.
//!
//! PaperWarden's pre-check runs in the app process before the service is
//! started, so the service only sees documents that passed it.

use crate::{render_pages, write_ppm, PdfEngine, PdfError, RenderFailure, MAX_RENDER_PIXELS};
use paperwarden_core::engine::Interruption;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Memory a job may use above the service's size when it starts: 512 MiB.
/// The largest render bitmap is 64 MiB; PW-010's worst hostile file peaked at
/// 41.5 MiB.
pub const DEFAULT_MEMORY_BUDGET: u64 = 512 << 20;
/// Smallest memory budget a request may ask for.
pub const MIN_MEMORY_BUDGET: u64 = 1 << 20;
/// Largest memory budget a request may ask for: 2 GiB.
pub const MAX_MEMORY_BUDGET: u64 = 2 << 30;
/// Longest time limit a request may ask for.
pub const MAX_TIME_LIMIT: Duration = Duration::from_secs(600);
/// How long after the time limit the watchdog kills the process, so a render
/// that stops in time at a page boundary can still report it.
pub const WATCHDOG_GRACE: Duration = Duration::from_millis(500);

/// What the app asks the service to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceRequest {
    /// Render box width in pixels.
    pub max_width: u32,
    /// Render box height in pixels.
    pub max_height: u32,
    /// The job's input limit; the staged copy never exceeds it.
    pub max_input_bytes: u64,
    /// The job's output limit.
    pub max_output_bytes: u64,
    /// The time left in the job.
    pub time_limit: Duration,
    /// Memory the job may use above the service's starting size.
    pub memory_budget: u64,
}

impl ServiceRequest {
    /// Whether every field is inside its documented bounds.
    pub fn is_valid(&self) -> bool {
        self.max_width > 0
            && self.max_height > 0
            && u64::from(self.max_width) * u64::from(self.max_height) <= MAX_RENDER_PIXELS
            && self.max_input_bytes > 0
            && self.max_output_bytes > 0
            && self.time_limit > Duration::ZERO
            && self.time_limit <= MAX_TIME_LIMIT
            && (MIN_MEMORY_BUDGET..=MAX_MEMORY_BUDGET).contains(&self.memory_budget)
    }
}

/// How a service job ended. The numeric codes cross the JNI and Binder
/// boundaries and must not change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceStatus {
    /// The first page was written to the output.
    Rendered,
    /// A request field was out of bounds.
    InvalidRequest,
    /// This process already served a job.
    AlreadyUsed,
    /// The time limit or memory ceiling could not be set, so nothing ran.
    Unconfined,
    /// PDFium could not be loaded.
    LibraryUnavailable,
    /// The input could not be read.
    Unreadable,
    /// The input exceeds the job's input limit.
    InputTooLarge,
    /// PDFium could not open the document.
    OpenFailed,
    /// The document has no pages.
    NoPages,
    /// The document has more than [`MAX_PAGES`](crate::MAX_PAGES) pages.
    TooManyPages,
    /// A page could not be rendered.
    PageFailed,
    /// The image would exceed the job's output limit.
    OutputTooLarge,
    /// The output could not be written.
    WriteFailed,
    /// The time limit passed between pages.
    TimedOut,
}

const STATUSES: [ServiceStatus; 14] = [
    ServiceStatus::Rendered,
    ServiceStatus::InvalidRequest,
    ServiceStatus::AlreadyUsed,
    ServiceStatus::Unconfined,
    ServiceStatus::LibraryUnavailable,
    ServiceStatus::Unreadable,
    ServiceStatus::InputTooLarge,
    ServiceStatus::OpenFailed,
    ServiceStatus::NoPages,
    ServiceStatus::TooManyPages,
    ServiceStatus::PageFailed,
    ServiceStatus::OutputTooLarge,
    ServiceStatus::WriteFailed,
    ServiceStatus::TimedOut,
];

impl ServiceStatus {
    /// The status's stable code.
    pub fn code(self) -> i32 {
        match self {
            Self::Rendered => 0,
            Self::InvalidRequest => 1,
            Self::AlreadyUsed => 2,
            Self::Unconfined => 3,
            Self::LibraryUnavailable => 4,
            Self::Unreadable => 5,
            Self::InputTooLarge => 6,
            Self::OpenFailed => 7,
            Self::NoPages => 8,
            Self::TooManyPages => 9,
            Self::PageFailed => 10,
            Self::OutputTooLarge => 11,
            Self::WriteFailed => 12,
            Self::TimedOut => 13,
        }
    }

    /// The status with `code`, if there is one.
    pub fn from_code(code: i32) -> Option<Self> {
        STATUSES.into_iter().find(|status| status.code() == code)
    }
}

static USED: AtomicBool = AtomicBool::new(false);

/// Confines this process, then renders the PDF read from `input` into
/// `output` with the PDFium library at `library`.
///
/// Call it only in the service process: it sets process-wide limits that
/// cannot be undone, and the watchdog kills the process at the time limit.
pub fn serve(library: &Path, input: File, output: File, request: &ServiceRequest) -> ServiceStatus {
    serve_with(library, input, output, request, confine)
}

/// [`serve`] with the confinement step supplied, so tests never confine the
/// test process.
fn serve_with(
    library: &Path,
    input: impl Read,
    output: impl Write,
    request: &ServiceRequest,
    confine: impl FnOnce(&ServiceRequest) -> io::Result<()>,
) -> ServiceStatus {
    if !request.is_valid() {
        return ServiceStatus::InvalidRequest;
    }
    if USED.swap(true, Ordering::SeqCst) {
        return ServiceStatus::AlreadyUsed;
    }
    let Some(deadline) = Instant::now().checked_add(request.time_limit) else {
        return ServiceStatus::InvalidRequest;
    };
    if confine(request).is_err() {
        return ServiceStatus::Unconfined;
    }
    render(library, input, output, request, deadline)
}

fn render(
    library: &Path,
    input: impl Read,
    output: impl Write,
    request: &ServiceRequest,
    deadline: Instant,
) -> ServiceStatus {
    let bytes = match read_bounded(input, request.max_input_bytes) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return ServiceStatus::InputTooLarge,
        Err(_) => return ServiceStatus::Unreadable,
    };
    let Ok(engine) = PdfEngine::load(library) else {
        return ServiceStatus::LibraryUnavailable;
    };
    let document = match engine.open(&bytes) {
        Ok(document) => document,
        Err(PdfError::TooManyPages(_)) => return ServiceStatus::TooManyPages,
        Err(_) => return ServiceStatus::OpenFailed,
    };
    let checkpoint = || {
        if Instant::now() >= deadline {
            Err(Interruption::TimedOut)
        } else {
            Ok(())
        }
    };
    let image = match render_pages(
        &document,
        request.max_width,
        request.max_height,
        checkpoint,
        |_| {},
    ) {
        Ok(image) => image,
        Err(RenderFailure::Interrupted(_)) => return ServiceStatus::TimedOut,
        Err(RenderFailure::NoPages) => return ServiceStatus::NoPages,
        Err(RenderFailure::Pdf(_)) => return ServiceStatus::PageFailed,
    };
    let mut output = Capped {
        inner: output,
        left: request.max_output_bytes,
    };
    match write_ppm(&image, &mut output) {
        Ok(()) => ServiceStatus::Rendered,
        Err(error) if error.kind() == io::ErrorKind::FileTooLarge => ServiceStatus::OutputTooLarge,
        Err(_) => ServiceStatus::WriteFailed,
    }
}

/// Reads all of `input`, or `None` if it is longer than `limit`.
fn read_bounded(input: impl Read, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    input
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

/// A writer that fails with [`io::ErrorKind::FileTooLarge`] rather than
/// write more than `left` bytes.
struct Capped<W> {
    inner: W,
    left: u64,
}

impl<W: Write> Write for Capped<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let wanted = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        if wanted > self.left {
            return Err(io::Error::from(io::ErrorKind::FileTooLarge));
        }
        let written = self.inner.write(buf)?;
        self.left = self
            .left
            .saturating_sub(u64::try_from(written).unwrap_or(u64::MAX));
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The `VmSize` line of `/proc/self/status`, in bytes.
#[cfg_attr(not(any(target_os = "android", target_os = "linux")), allow(dead_code))]
fn vm_size(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmSize:"))?;
    let mut fields = line.split_ascii_whitespace().skip(1);
    let kib = fields.next()?.parse::<u64>().ok()?;
    (fields.next() == Some("kB")).then_some(kib.checked_mul(1024)?)
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn confine(request: &ServiceRequest) -> io::Result<()> {
    use nix::sys::resource::{rlim_t, setrlimit, Resource};
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    // The watchdog's own thread exists before the ceiling is set.
    let wait = request.time_limit.saturating_add(WATCHDOG_GRACE);
    std::thread::Builder::new()
        .name("pw-watchdog".to_owned())
        .spawn(move || {
            std::thread::sleep(wait);
            // SIGKILL: no handler, no unwinding, no crash report.
            let _ = kill(Pid::this(), Signal::SIGKILL);
            std::process::abort();
        })?;

    let status = std::fs::read_to_string("/proc/self/status")?;
    let size = vm_size(&status).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
    // `rlim_t` is 32 bits on 32-bit Android. A ceiling it cannot hold is an
    // error, never a silent "unlimited", so nothing renders unconfined.
    let ceiling = rlim_t::try_from(size.saturating_add(request.memory_budget))
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // Soft and hard together: the process cannot raise it again.
    setrlimit(Resource::RLIMIT_AS, ceiling, ceiling).map_err(io::Error::from)
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
fn confine(_: &ServiceRequest) -> io::Result<()> {
    // Only Android runs the service; elsewhere nothing may render unconfined.
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ServiceRequest {
        ServiceRequest {
            max_width: 1080,
            max_height: 1920,
            max_input_bytes: 1 << 20,
            max_output_bytes: 64 << 20,
            time_limit: Duration::from_secs(10),
            memory_budget: DEFAULT_MEMORY_BUDGET,
        }
    }

    #[test]
    fn every_status_round_trips_through_its_code() {
        for status in STATUSES {
            assert_eq!(ServiceStatus::from_code(status.code()), Some(status));
        }
        let mut codes: Vec<_> = STATUSES.iter().map(|status| status.code()).collect();
        codes.dedup();
        assert_eq!(codes, (0..14).collect::<Vec<_>>());
        assert_eq!(ServiceStatus::from_code(14), None);
        assert_eq!(ServiceStatus::from_code(-1), None);
    }

    #[test]
    fn requests_outside_their_bounds_are_invalid() {
        assert!(request().is_valid());
        let invalid = [
            ServiceRequest {
                max_width: 0,
                ..request()
            },
            ServiceRequest {
                max_height: 0,
                ..request()
            },
            ServiceRequest {
                max_width: 4097,
                max_height: 4096,
                ..request()
            },
            ServiceRequest {
                max_input_bytes: 0,
                ..request()
            },
            ServiceRequest {
                max_output_bytes: 0,
                ..request()
            },
            ServiceRequest {
                time_limit: Duration::ZERO,
                ..request()
            },
            ServiceRequest {
                time_limit: MAX_TIME_LIMIT + Duration::from_millis(1),
                ..request()
            },
            ServiceRequest {
                memory_budget: MIN_MEMORY_BUDGET - 1,
                ..request()
            },
            ServiceRequest {
                memory_budget: MAX_MEMORY_BUDGET + 1,
                ..request()
            },
        ];
        for request in invalid {
            assert!(!request.is_valid(), "{request:?}");
        }
    }

    #[test]
    fn an_invalid_request_is_refused_before_anything_is_confined() {
        let invalid = ServiceRequest {
            max_width: 0,
            ..request()
        };
        let status = serve_with(Path::new("unused"), &b""[..], io::sink(), &invalid, |_| {
            panic!("confined for an invalid request")
        });
        assert_eq!(status, ServiceStatus::InvalidRequest);
    }

    // The only test that gets past validation: one process serves one job.
    #[test]
    fn nothing_renders_unconfined_and_a_process_serves_one_job() {
        let mut confined = 0;
        let status = serve_with(
            Path::new("unused"),
            &b""[..],
            io::sink(),
            &request(),
            |_| {
                confined += 1;
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            },
        );
        assert_eq!(status, ServiceStatus::Unconfined);
        assert_eq!(confined, 1);

        let status = serve_with(
            Path::new("unused"),
            &b""[..],
            io::sink(),
            &request(),
            |_| panic!("confined twice"),
        );
        assert_eq!(status, ServiceStatus::AlreadyUsed);
    }

    #[test]
    fn input_longer_than_the_limit_is_refused() {
        assert_eq!(
            read_bounded(&b"12345"[..], 5).unwrap(),
            Some(b"12345".to_vec())
        );
        assert_eq!(read_bounded(&b"123456"[..], 5).unwrap(), None);
    }

    #[test]
    fn the_output_never_exceeds_its_limit() {
        let mut sink = Vec::new();
        let mut capped = Capped {
            inner: &mut sink,
            left: 4,
        };
        capped.write_all(b"1234").unwrap();
        let error = capped.write_all(b"5").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        assert_eq!(sink, b"1234");
    }

    #[test]
    fn the_process_size_is_read_from_proc_status() {
        let status = "Name:\tpw\nVmPeak:\t  900 kB\nVmSize:\t  812344 kB\nVmRSS:\t 1 kB\n";
        assert_eq!(vm_size(status), Some(812_344 * 1024));
        assert_eq!(vm_size("VmSize:\t12 MB\n"), None);
        assert_eq!(vm_size("VmSize:\n"), None);
        assert_eq!(vm_size("Name:\tpw\n"), None);
    }

    #[cfg(any(target_os = "android", target_os = "linux"))]
    #[test]
    fn this_platform_reports_its_process_size() {
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        assert!(vm_size(&status).unwrap() > 0);
    }
}
