//! qpdf structural transforms and encryption (feasibility spike PW-012;
//! `docs/feasibility/PW-012-qpdf.md`).
//!
//! qpdf 12.4.2 is built from pinned upstream source (`scripts/fetch_qpdf.py`,
//! `build.rs`) with its native crypto, and driven through its QPDFJob C API:
//! each operation is a JSON job description built here. qpdf never prints
//! (its logger discards everything) and never follows links or runs scripts;
//! it only reads and rewrites PDF structure.
//!
//! Before qpdf parses anything, [`QpdfProcessor`] runs PaperWarden's own
//! pre-check (`paperwarden-pdf-scan`), and qpdf's process-wide limits are set
//! once ([`LIMITS`]): its defaults leave Flate, DCT, PNG and TIFF decoding
//! memory unbounded. Every output is opened again by qpdf and pre-checked
//! before export ([`QpdfProcessor`] is also the job's verifier).
//!
//! Built without the fetched sources, the crate has no qpdf: every operation
//! returns [`QpdfError::Unavailable`] and [`available`] is `false`.

#[cfg(paperwarden_qpdf)]
#[allow(
    unsafe_code,
    reason = "qpdf's C API; every call and its SAFETY argument is in this one module"
)]
mod ffi;

use paperwarden_core::engine::{
    EngineError, Interruption, OutputVerifier, ProcessContext, Processor,
};
use paperwarden_core::validation::Rejection;
use paperwarden_core::Notice;
use paperwarden_pdf_scan::{scan, Interrupted, Report, ScanError, ScanLimits};
use std::cell::Cell;
use std::ffi::CString;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The qpdf release this crate builds.
pub const QPDF_VERSION: &str = "12.4.2";

/// qpdf's process-wide limits, as `(qpdf_param_e, value)`. They match the
/// pre-check's bounds (`ScanLimits::default()`) where both have one.
pub const LIMITS: &[(Limit, u32)] = &[
    (Limit::ParserMaxNesting, 32),
    (Limit::ParserMaxContainerSize, 1_000_000),
    (Limit::ParserMaxErrors, 15),
    (Limit::MaxStreamFilters, 8),
    (Limit::FlateMaxMemory, 64 << 20),
    (Limit::PngMaxMemory, 64 << 20),
    (Limit::TiffMaxMemory, 64 << 20),
    (Limit::RunLengthMaxMemory, 64 << 20),
    (Limit::DctMaxMemory, 64 << 20),
    (Limit::DocMaxWarnings, 1000),
];

/// One of qpdf's global limits (`qpdf_param_e` in `qpdf/Constants.h`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    /// Warnings per document before qpdf gives up.
    DocMaxWarnings,
    /// Nesting depth of arrays and dictionaries.
    ParserMaxNesting,
    /// Parser errors before qpdf gives up on a damaged file.
    ParserMaxErrors,
    /// Elements in one array or dictionary.
    ParserMaxContainerSize,
    /// Filters on one stream.
    MaxStreamFilters,
    /// Memory for one DCT (JPEG) decode.
    DctMaxMemory,
    /// Memory for one Flate decode.
    FlateMaxMemory,
    /// Memory for one PNG predictor.
    PngMaxMemory,
    /// Memory for one run-length decode.
    RunLengthMaxMemory,
    /// Memory for one TIFF predictor.
    TiffMaxMemory,
}

impl Limit {
    /// The `qpdf_param_e` value.
    pub fn param(self) -> i32 {
        match self {
            Self::DocMaxWarnings => 0x12000,
            Self::ParserMaxNesting => 0x13000,
            Self::ParserMaxErrors => 0x13001,
            Self::ParserMaxContainerSize => 0x13002,
            Self::MaxStreamFilters => 0x14000,
            Self::DctMaxMemory => 0x14020,
            Self::FlateMaxMemory => 0x14030,
            Self::PngMaxMemory => 0x14040,
            Self::RunLengthMaxMemory => 0x14050,
            Self::TiffMaxMemory => 0x14060,
        }
    }
}

/// Why qpdf could not do what was asked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QpdfError {
    /// This build has no qpdf, or its limits could not be set.
    Unavailable,
    /// An argument was invalid: a password or path with a NUL byte, an
    /// empty owner password, or a page range out of order.
    InvalidArgument(&'static str),
    /// qpdf could not complete the operation: the file is not a PDF, is
    /// damaged beyond repair, exceeds one of qpdf's limits, or needs a
    /// password that was not given.
    Failed,
}

impl fmt::Display for QpdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("the PDF structure engine is unavailable"),
            Self::InvalidArgument(reason) => formatter.write_str(reason),
            Self::Failed => formatter.write_str("the PDF could not be processed"),
        }
    }
}

impl std::error::Error for QpdfError {}

/// How an operation that succeeded went.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// No warnings.
    Clean,
    /// qpdf warned, typically because it repaired damage in the input.
    Repaired,
}

/// A quarter, half or three-quarter turn clockwise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rotation {
    /// 90 degrees.
    Quarter,
    /// 180 degrees.
    Half,
    /// 270 degrees.
    ThreeQuarters,
}

/// Pages `first` to `last` (1-based, inclusive); `last: None` is the last
/// page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageRange {
    /// The first page, from 1.
    pub first: u32,
    /// The last page, or `None` for the end of the document.
    pub last: Option<u32>,
}

impl PageRange {
    /// Every page.
    pub const ALL: Self = Self {
        first: 1,
        last: None,
    };

    fn spec(self) -> Result<String, QpdfError> {
        match self.last {
            _ if self.first == 0 => Err(QpdfError::InvalidArgument("pages count from 1")),
            Some(last) if last < self.first => {
                Err(QpdfError::InvalidArgument("the page range is out of order"))
            }
            Some(last) => Ok(format!("{}-{last}", self.first)),
            None => Ok(format!("{}-z", self.first)),
        }
    }

    /// How many pages the range selects in a document of `pages` pages, or
    /// `None` if it reaches past the end.
    pub fn len_in(self, pages: u32) -> Option<u32> {
        let last = self.last.unwrap_or(pages);
        (self.first >= 1 && last <= pages && last >= self.first).then(|| last - self.first + 1)
    }
}

/// A structural transform from one PDF to a new one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Transform {
    /// Read and write the document again, repairing what qpdf can.
    Rewrite,
    /// Rotate `pages` clockwise.
    Rotate {
        /// How far.
        rotation: Rotation,
        /// Which pages.
        pages: PageRange,
    },
    /// Keep only `pages`.
    Extract {
        /// Which pages.
        pages: PageRange,
    },
    /// Append every page of `others`, in order.
    Merge {
        /// Further PDFs, already staged.
        others: Vec<PathBuf>,
    },
    /// Encrypt with AES-256 (revision 6). `owner` must not be empty; `user`
    /// may be, which lets anyone open the file but keeps it encrypted.
    Encrypt {
        /// The password needed to open the document.
        user: String,
        /// The password that controls permissions.
        owner: String,
    },
    /// Remove encryption, opening the input with `password`.
    Decrypt {
        /// The user or owner password.
        password: String,
    },
}

impl Transform {
    /// The password qpdf opens the input with, if any.
    fn input_password(&self) -> &str {
        match self {
            Self::Decrypt { password } => password,
            _ => "",
        }
    }
}

/// Refuses text qpdf would read differently from what was meant: a NUL ends
/// a C string, so a password or path with one would be cut short.
fn no_nul(value: &str) -> Result<&str, QpdfError> {
    if value.contains('\0') {
        Err(QpdfError::InvalidArgument(
            "text for qpdf contains a NUL byte",
        ))
    } else {
        Ok(value)
    }
}

/// Appends `value` to `out` as a JSON string.
fn json_string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            character if u32::from(character) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => out.push(character),
        }
    }
    out.push('"');
}

fn path_str(path: &Path) -> Result<&str, QpdfError> {
    no_nul(
        path.to_str()
            .ok_or(QpdfError::InvalidArgument("the path is not valid text"))?,
    )
}

/// The QPDFJob JSON for `transform` from `input` to `output`.
fn job_json(input: &Path, output: &Path, transform: &Transform) -> Result<String, QpdfError> {
    let mut json = String::from("{\"inputFile\":");
    json_string(&mut json, path_str(input)?);
    json.push_str(",\"outputFile\":");
    json_string(&mut json, path_str(output)?);
    match transform {
        Transform::Rewrite => {}
        Transform::Rotate { rotation, pages } => {
            let degrees = match rotation {
                Rotation::Quarter => 90,
                Rotation::Half => 180,
                Rotation::ThreeQuarters => 270,
            };
            json.push_str(",\"rotate\":[");
            json_string(&mut json, &format!("+{degrees}:{}", pages.spec()?));
            json.push(']');
        }
        Transform::Extract { pages } => {
            json.push_str(",\"pages\":[{\"file\":\".\",\"range\":");
            json_string(&mut json, &pages.spec()?);
            json.push_str("}]");
        }
        Transform::Merge { others } => {
            json.push_str(",\"pages\":[{\"file\":\".\"}");
            for other in others {
                json.push_str(",{\"file\":");
                json_string(&mut json, path_str(other)?);
                json.push('}');
            }
            json.push(']');
        }
        Transform::Encrypt { user, owner } => {
            if owner.is_empty() {
                return Err(QpdfError::InvalidArgument("the owner password is empty"));
            }
            json.push_str(",\"encrypt\":{\"userPassword\":");
            json_string(&mut json, no_nul(user)?);
            json.push_str(",\"ownerPassword\":");
            json_string(&mut json, no_nul(owner)?);
            json.push_str(",\"256bit\":{}}");
        }
        Transform::Decrypt { password } => {
            json.push_str(",\"password\":");
            json_string(&mut json, no_nul(password)?);
            json.push_str(",\"decrypt\":\"\"");
        }
    }
    json.push('}');
    Ok(json)
}

/// Whether this build includes qpdf.
pub fn available() -> bool {
    cfg!(paperwarden_qpdf)
}

/// Once per process: silences qpdf's default logger and sets [`LIMITS`].
/// `false` if either failed, in which case nothing may run.
#[cfg(paperwarden_qpdf)]
fn limited() -> bool {
    static SET: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SET.get_or_init(|| {
        ffi::silence_default_logger()
            && LIMITS
                .iter()
                .all(|&(limit, value)| ffi::set_global(limit.param(), value))
    })
}

/// The value qpdf currently uses for `limit`, after the one-time setup;
/// `None` without qpdf. For tests and diagnostics.
pub fn limit_in_effect(limit: Limit) -> Option<u32> {
    #[cfg(paperwarden_qpdf)]
    {
        limited().then(|| ffi::get_global(limit.param())).flatten()
    }
    #[cfg(not(paperwarden_qpdf))]
    {
        let _ = limit;
        None
    }
}

/// A string for qpdf, which must not contain NUL. The bytes are cleared on
/// drop, since job descriptions can carry passwords (best effort: copies
/// qpdf makes are its own).
struct Secret(Option<CString>);

impl Secret {
    fn new(value: String) -> Result<Self, QpdfError> {
        CString::new(value)
            .map(|value| Self(Some(value)))
            .map_err(|_| QpdfError::InvalidArgument("text for qpdf contains a NUL byte"))
    }

    #[cfg(paperwarden_qpdf)]
    fn get(&self) -> &std::ffi::CStr {
        self.0.as_deref().unwrap_or_default()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        if let Some(value) = self.0.take() {
            let mut bytes = value.into_bytes();
            bytes.fill(0);
            std::hint::black_box(&bytes);
        }
    }
}

/// Runs `transform`, writing a new PDF at `output`. `input` is never
/// modified.
///
/// # Errors
///
/// [`QpdfError`].
pub fn transform(input: &Path, output: &Path, transform: &Transform) -> Result<Outcome, QpdfError> {
    let job = Secret::new(job_json(input, output, transform)?)?;
    run(&job)
}

/// Checks the structure of the PDF at `input`, opened with `password`.
///
/// # Errors
///
/// [`QpdfError::Failed`] if qpdf finds errors.
pub fn check(input: &Path, password: &str) -> Result<Outcome, QpdfError> {
    let mut json = String::from("{\"inputFile\":");
    json_string(&mut json, path_str(input)?);
    json.push_str(",\"password\":");
    json_string(&mut json, no_nul(password)?);
    json.push_str(",\"check\":\"\"}");
    run(&Secret::new(json)?)
}

#[cfg(paperwarden_qpdf)]
fn run(job: &Secret) -> Result<Outcome, QpdfError> {
    if !limited() {
        return Err(QpdfError::Unavailable);
    }
    // QPDFJob's exit codes: 0 success, 2 errors, 3 warnings.
    match ffi::run_job(job.get()) {
        Some(0) => Ok(Outcome::Clean),
        Some(3) => Ok(Outcome::Repaired),
        Some(_) => Err(QpdfError::Failed),
        None => Err(QpdfError::Unavailable),
    }
}

#[cfg(not(paperwarden_qpdf))]
fn run(_: &Secret) -> Result<Outcome, QpdfError> {
    Err(QpdfError::Unavailable)
}

/// The number of pages in the PDF at `path`, opened with `password`.
///
/// # Errors
///
/// [`QpdfError::Failed`] if qpdf cannot open it, including with a wrong
/// password.
pub fn page_count(path: &Path, password: &str) -> Result<u32, QpdfError> {
    let path = Secret::new(path_str(path)?.to_owned())?;
    let password = Secret::new(password.to_owned())?;
    pages(&path, &password)
}

#[cfg(paperwarden_qpdf)]
fn pages(path: &Secret, password: &Secret) -> Result<u32, QpdfError> {
    if !limited() {
        return Err(QpdfError::Unavailable);
    }
    ffi::page_count(path.get(), password.get()).ok_or(QpdfError::Failed)
}

#[cfg(not(paperwarden_qpdf))]
fn pages(_: &Secret, _: &Secret) -> Result<u32, QpdfError> {
    Err(QpdfError::Unavailable)
}

/// PaperWarden's pre-check, as a job step (the same mapping as
/// `paperwarden_pdf::precheck`, without depending on PDFium).
fn precheck(
    bytes: &[u8],
    limits: &ScanLimits,
    mut checkpoint: impl FnMut() -> Result<(), Interruption>,
) -> Result<Report, EngineError> {
    let mut interruption = None;
    let result = scan(bytes, limits, &mut || {
        checkpoint().map_err(|stopped| {
            interruption = Some(stopped);
            Interrupted
        })
    });
    match result {
        Ok(report) => Ok(report),
        Err(ScanError::Limit(limit)) => Err(EngineError::LimitExceeded(limit.reason())),
        Err(ScanError::Interrupted) => Err(EngineError::Interrupted(
            interruption.unwrap_or(Interruption::Cancelled),
        )),
        Err(error) => Err(EngineError::Failed(error.reason())),
    }
}

fn engine_error(error: QpdfError) -> EngineError {
    match error {
        QpdfError::Unavailable => EngineError::Failed("the PDF structure engine is unavailable"),
        QpdfError::InvalidArgument(reason) => EngineError::Failed(reason),
        QpdfError::Failed => EngineError::Failed("the PDF could not be processed"),
    }
}

/// A single-input transform as a job: pre-check, qpdf, then verification of
/// the output (page count through qpdf, and the pre-check again on an
/// unencrypted result). Use the same value as the job's processor and
/// verifier.
///
/// [`Transform::Merge`] needs further staged inputs, which the job runner
/// does not stage yet; it is refused here.
#[derive(Debug)]
pub struct QpdfProcessor {
    transform: Transform,
    limits: ScanLimits,
    expected_pages: Cell<Option<u32>>,
}

impl QpdfProcessor {
    /// A processor for `transform`.
    pub fn new(transform: Transform) -> Self {
        Self {
            transform,
            limits: ScanLimits::default(),
            expected_pages: Cell::new(None),
        }
    }

    /// The password the output opens with.
    fn output_password(&self) -> &str {
        match &self.transform {
            Transform::Encrypt { user, .. } => user,
            _ => "",
        }
    }
}

impl Processor for QpdfProcessor {
    fn process(&self, context: &mut ProcessContext<'_>) -> Result<(), EngineError> {
        self.expected_pages.set(None);
        if matches!(self.transform, Transform::Merge { .. }) {
            return Err(EngineError::Failed("merging needs every input staged"));
        }
        // PaperWarden's pre-check first. A password-protected input cannot
        // be measured before it is decrypted, so for Decrypt the pre-check
        // runs on the decrypted output instead (in verify).
        if !matches!(self.transform, Transform::Decrypt { .. }) {
            let mut bytes = Vec::new();
            std::fs::File::open(context.input_path())?
                .take(context.input_len())
                .read_to_end(&mut bytes)?;
            let report = precheck(&bytes, &self.limits, || context.checkpoint())?;
            for notice in report.notices() {
                context.notice(notice);
            }
        }
        context.checkpoint()?;

        let pages = page_count(context.input_path(), self.transform.input_password())
            .map_err(engine_error)?;
        let expected = match &self.transform {
            Transform::Extract { pages: range } | Transform::Rotate { pages: range, .. } => {
                let selected = range.len_in(pages).ok_or(EngineError::Failed(
                    "the page range is outside the document",
                ))?;
                if matches!(self.transform, Transform::Extract { .. }) {
                    selected
                } else {
                    pages
                }
            }
            _ => pages,
        };
        self.expected_pages.set(Some(expected));

        drop(context.create_output()?);
        let outcome = transform(context.input_path(), context.output_path(), &self.transform)
            .map_err(engine_error)?;
        if outcome == Outcome::Repaired {
            context.notice(Notice::Repaired);
        }
        context.checkpoint()?;
        Ok(())
    }
}

impl OutputVerifier for QpdfProcessor {
    fn verify(&self, output: &Path, _len: u64) -> Result<(), Rejection> {
        let Some(expected) = self.expected_pages.get() else {
            return Err(Rejection("the job did not run"));
        };
        let pages = page_count(output, self.output_password())
            .map_err(|_| Rejection("the result is not a PDF qpdf can open"))?;
        if pages != expected {
            return Err(Rejection("the result has the wrong number of pages"));
        }
        // An unencrypted result must pass the pre-check like any input.
        if !matches!(self.transform, Transform::Encrypt { .. }) {
            let bytes =
                std::fs::read(output).map_err(|_| Rejection("the result cannot be read"))?;
            scan(&bytes, &self.limits, &mut || Ok(()))
                .map_err(|_| Rejection("the result does not pass the PDF pre-check"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(transform: &Transform) -> String {
        job_json(Path::new("/w/in.pdf"), Path::new("/w/out.pdf"), transform).unwrap()
    }

    #[test]
    fn each_transform_becomes_a_qpdf_job() {
        let head = r#"{"inputFile":"/w/in.pdf","outputFile":"/w/out.pdf""#;
        let cases = [
            (Transform::Rewrite, "}"),
            (
                Transform::Rotate {
                    rotation: Rotation::Quarter,
                    pages: PageRange::ALL,
                },
                r#","rotate":["+90:1-z"]}"#,
            ),
            (
                Transform::Extract {
                    pages: PageRange {
                        first: 2,
                        last: Some(3),
                    },
                },
                r#","pages":[{"file":".","range":"2-3"}]}"#,
            ),
            (
                Transform::Merge {
                    others: vec![PathBuf::from("/w/b.pdf")],
                },
                r#","pages":[{"file":"."},{"file":"/w/b.pdf"}]}"#,
            ),
            (
                Transform::Encrypt {
                    user: "u".into(),
                    owner: "o".into(),
                },
                r#","encrypt":{"userPassword":"u","ownerPassword":"o","256bit":{}}}"#,
            ),
            (
                Transform::Decrypt {
                    password: "p".into(),
                },
                r#","password":"p","decrypt":""}"#,
            ),
        ];
        for (transform, tail) in cases {
            assert_eq!(json(&transform), format!("{head}{tail}"), "{transform:?}");
        }
    }

    #[test]
    fn json_strings_are_escaped() {
        let mut out = String::new();
        json_string(&mut out, "a\"b\\c\n\u{1}é");
        assert_eq!(out, r#""a\"b\\c\u000a\u0001é""#);
        // A password cannot add keys to the job.
        let job = json(&Transform::Decrypt {
            password: r#"x","outputFile":"/etc/passwd"#.into(),
        });
        assert_eq!(job.matches("\"outputFile\":").count(), 1);
    }

    #[test]
    fn invalid_arguments_are_refused_before_qpdf() {
        let invalid = [
            Transform::Encrypt {
                user: "u".into(),
                owner: String::new(),
            },
            Transform::Extract {
                pages: PageRange {
                    first: 0,
                    last: None,
                },
            },
            Transform::Rotate {
                rotation: Rotation::Half,
                pages: PageRange {
                    first: 3,
                    last: Some(2),
                },
            },
        ];
        for transform in invalid {
            assert!(
                matches!(
                    job_json(Path::new("a"), Path::new("b"), &transform),
                    Err(QpdfError::InvalidArgument(_))
                ),
                "{transform:?}"
            );
        }
        let nul = transform(
            Path::new("a"),
            Path::new("b"),
            &Transform::Decrypt {
                password: "a\0b".into(),
            },
        );
        assert!(matches!(nul, Err(QpdfError::InvalidArgument(_))));
    }

    #[test]
    fn page_ranges_count_pages_inside_the_document() {
        let range = |first, last| PageRange { first, last };
        assert_eq!(PageRange::ALL.len_in(5), Some(5));
        assert_eq!(range(2, Some(3)).len_in(5), Some(2));
        assert_eq!(range(5, None).len_in(5), Some(1));
        assert_eq!(range(2, Some(6)).len_in(5), None);
        assert_eq!(range(6, None).len_in(5), None);
        assert_eq!(range(0, None).len_in(5), None);
    }

    #[test]
    fn every_limit_has_a_distinct_qpdf_parameter() {
        let mut params: Vec<_> = LIMITS.iter().map(|(limit, _)| limit.param()).collect();
        params.sort_unstable();
        params.dedup();
        assert_eq!(params.len(), LIMITS.len());
    }

    #[cfg(not(paperwarden_qpdf))]
    #[test]
    fn without_qpdf_every_operation_is_unavailable() {
        assert!(!available());
        let result = transform(Path::new("a"), Path::new("b"), &Transform::Rewrite);
        assert_eq!(result, Err(QpdfError::Unavailable));
        assert_eq!(page_count(Path::new("a"), ""), Err(QpdfError::Unavailable));
    }
}
