//! PDF page rendering and text extraction through PDFium (feasibility spike
//! PW-010; `docs/feasibility/PW-010-pdfium-android.md`).
//!
//! PDFium is loaded at run time from the platform library the app ships
//! (`libpdfium.so` on Android), fetched and verified by
//! `scripts/fetch_pdfium.py`. That build has V8 and XFA compiled out, so it
//! cannot run PDF JavaScript (`PRODUCT_INVARIANTS.md` 6). Nothing here follows
//! links or actions in a document (invariant 7).
//!
//! Every render is sized here, inside a pixel box the caller chooses and
//! [`MAX_RENDER_PIXELS`] caps, so a page's declared size can never decide how
//! much memory a bitmap takes. [`RenderProcessor`] plugs PDFium into the job
//! runner's engine seam.
//!
//! Before PDFium sees a document, [`RenderProcessor`] runs PaperWarden's own
//! pre-check ([`precheck`], `paperwarden-pdf-scan`, ADR-0007). It stops what
//! PDFium would otherwise decode unbounded, and records damage and active
//! content for the user's warnings.
//!
//! On Android, PDFium never runs in the app process: [`service`] is the entry
//! point for the disposable, confined process that hosts it (#59).

pub mod service;

use paperwarden_core::engine::{
    EngineError, Interruption, OutputVerifier, ProcessContext, Processor,
};
use paperwarden_core::validation::Rejection;
use paperwarden_pdf_scan::{scan, Interrupted, Report, ScanError, ScanLimits};
use pdfium_render::prelude::{PdfDocument, PdfRenderConfig, Pdfium};
use std::cell::RefCell;
use std::fmt;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Largest bitmap one render may allocate: 4096 x 4096 pixels, 64 MiB as RGBA.
pub const MAX_RENDER_PIXELS: u64 = 4096 * 4096;
/// Most pages a document may have before the engine refuses it.
pub const MAX_PAGES: u32 = 5000;

/// Why PDFium could not do what was asked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PdfError {
    /// The PDFium library could not be loaded.
    Library(String),
    /// The document could not be opened; PDFium's reason.
    Open(String),
    /// A page could not be read or rendered; PDFium's reason.
    Page(String),
    /// The page has no usable size, such as zero, negative or not a number.
    DegeneratePage,
    /// The requested render box exceeds [`MAX_RENDER_PIXELS`].
    RenderTooLarge,
    /// The document has more than [`MAX_PAGES`] pages.
    TooManyPages(u32),
}

impl fmt::Display for PdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library(reason) => write!(formatter, "PDF engine unavailable: {reason}"),
            Self::Open(reason) => write!(formatter, "the PDF could not be opened: {reason}"),
            Self::Page(reason) => write!(formatter, "a page could not be rendered: {reason}"),
            Self::DegeneratePage => formatter.write_str("a page has no usable size"),
            Self::RenderTooLarge => formatter.write_str("the requested image is too large"),
            Self::TooManyPages(count) => write!(formatter, "the PDF has too many pages ({count})"),
        }
    }
}

impl std::error::Error for PdfError {}

/// The process-wide PDFium instance. PDFium can be bound only once per
/// process, so the first successful [`PdfEngine::load`] wins and later calls
/// return the same engine.
pub struct PdfEngine {
    pdfium: Pdfium,
}

impl fmt::Debug for PdfEngine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PdfEngine")
    }
}

static ENGINE: OnceLock<PdfEngine> = OnceLock::new();

impl PdfEngine {
    /// Loads PDFium from `library` once per process.
    ///
    /// # Errors
    ///
    /// Returns [`PdfError::Library`] if the library cannot be loaded.
    pub fn load(library: &Path) -> Result<&'static Self, PdfError> {
        if let Some(engine) = ENGINE.get() {
            return Ok(engine);
        }
        let bindings = Pdfium::bind_to_library(library)
            .map_err(|error| PdfError::Library(error.to_string()))?;
        Ok(ENGINE.get_or_init(|| Self {
            pdfium: Pdfium::new(bindings),
        }))
    }

    /// Opens a document from memory. `bytes` must outlive the document.
    ///
    /// # Errors
    ///
    /// Returns [`PdfError::Open`] for anything PDFium cannot open, including
    /// encrypted documents, or [`PdfError::TooManyPages`].
    pub fn open<'a>(&'a self, bytes: &'a [u8]) -> Result<Document<'a>, PdfError> {
        let document = self
            .pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|error| PdfError::Open(error.to_string()))?;
        let count = u32::try_from(document.pages().len()).unwrap_or(0);
        if count > MAX_PAGES {
            return Err(PdfError::TooManyPages(count));
        }
        Ok(Document { document, count })
    }
}

/// An open document.
pub struct Document<'a> {
    document: PdfDocument<'a>,
    count: u32,
}

impl fmt::Debug for Document<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Document")
            .field("pages", &self.count)
            .finish()
    }
}

/// A rendered page as tightly packed RGBA rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, top row first.
    pub rgba: Vec<u8>,
}

impl Document<'_> {
    /// The number of pages.
    pub fn page_count(&self) -> u32 {
        self.count
    }

    /// A page's size in points (1/72 inch).
    ///
    /// # Errors
    ///
    /// Returns [`PdfError::Page`] if the page cannot be read.
    pub fn page_size(&self, index: u32) -> Result<(f32, f32), PdfError> {
        let page = self.page(index)?;
        Ok((page.width().value, page.height().value))
    }

    /// The page's text, in reading order as PDFium extracts it.
    ///
    /// # Errors
    ///
    /// Returns [`PdfError::Page`] if the page or its text cannot be read.
    pub fn text(&self, index: u32) -> Result<String, PdfError> {
        let page = self.page(index)?;
        let text = page
            .text()
            .map_err(|error| PdfError::Page(error.to_string()))?;
        Ok(text.all())
    }

    /// Renders a page to fit inside `max_width` x `max_height` pixels,
    /// keeping its aspect ratio.
    ///
    /// # Errors
    ///
    /// Returns [`PdfError::RenderTooLarge`] if the box exceeds
    /// [`MAX_RENDER_PIXELS`], [`PdfError::DegeneratePage`] for a page without
    /// a usable size, or [`PdfError::Page`] if PDFium fails.
    pub fn render(
        &self,
        index: u32,
        max_width: u32,
        max_height: u32,
    ) -> Result<PageImage, PdfError> {
        if max_width == 0
            || max_height == 0
            || u64::from(max_width) * u64::from(max_height) > MAX_RENDER_PIXELS
        {
            return Err(PdfError::RenderTooLarge);
        }
        let page = self.page(index)?;
        let (width, height) = (page.width().value, page.height().value);
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return Err(PdfError::DegeneratePage);
        }
        let box_width = i32::try_from(max_width).map_err(|_| PdfError::RenderTooLarge)?;
        let box_height = i32::try_from(max_height).map_err(|_| PdfError::RenderTooLarge)?;
        // Scale to the box width, then let the maxima shrink it to fit.
        let config = PdfRenderConfig::new()
            .scale_page_by_factor(max_width as f32 / width)
            .set_maximum_width(box_width)
            .set_maximum_height(box_height);
        let bitmap = page
            .render_with_config(&config)
            .map_err(|error| PdfError::Page(error.to_string()))?;
        let (out_width, out_height) = (
            u32::try_from(bitmap.width()).map_err(|_| PdfError::DegeneratePage)?,
            u32::try_from(bitmap.height()).map_err(|_| PdfError::DegeneratePage)?,
        );
        if out_width == 0 || out_height == 0 || out_width > max_width || out_height > max_height {
            return Err(PdfError::DegeneratePage);
        }
        Ok(PageImage {
            width: out_width,
            height: out_height,
            rgba: bitmap.as_rgba_bytes(),
        })
    }

    fn page(&self, index: u32) -> Result<pdfium_render::prelude::PdfPage<'_>, PdfError> {
        let index =
            i32::try_from(index).map_err(|_| PdfError::Page("page index out of range".into()))?;
        self.document
            .pages()
            .get(index)
            .map_err(|error| PdfError::Page(error.to_string()))
    }
}

/// What [`RenderProcessor`] observed, for reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderStats {
    /// Pages in the document.
    pub pages: u32,
    /// Pages rendered before the job finished or stopped.
    pub rendered: u32,
    /// Time to open the document.
    pub open_time: Duration,
    /// Total time spent rendering pages.
    pub render_time: Duration,
    /// Text of the first page, if it could be read.
    pub first_page_text: Option<String>,
    /// What the pre-check found: damage and active content to warn about.
    pub precheck: Option<Report>,
    /// Time the pre-check took.
    pub precheck_time: Duration,
}

/// Runs PaperWarden's pre-check over a whole PDF before PDFium opens it.
///
/// `checkpoint` is the job's cancellation and time check.
///
/// # Errors
///
/// [`EngineError::LimitExceeded`] when the document exceeds a bound,
/// [`EngineError::Interrupted`] when `checkpoint` stops it, and
/// [`EngineError::Failed`] for a document the pre-check refuses.
pub fn precheck(
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

/// Renders every page of the staged PDF inside the render box, checking for
/// cancellation and the time limit between pages, and writes the first page
/// as a binary PPM image (`P6`), the job's verifiable output.
#[derive(Debug)]
pub struct RenderProcessor<'e> {
    engine: &'e PdfEngine,
    max_width: u32,
    max_height: u32,
    limits: ScanLimits,
    stats: RefCell<RenderStats>,
}

impl<'e> RenderProcessor<'e> {
    /// Creates a processor rendering inside `max_width` x `max_height`.
    pub fn new(engine: &'e PdfEngine, max_width: u32, max_height: u32) -> Self {
        Self {
            engine,
            max_width,
            max_height,
            limits: ScanLimits::default(),
            stats: RefCell::new(RenderStats::default()),
        }
    }

    /// Uses `limits` for the pre-check instead of the defaults.
    #[must_use]
    pub fn with_limits(mut self, limits: ScanLimits) -> Self {
        self.limits = limits;
        self
    }

    /// What the last run observed.
    pub fn stats(&self) -> RenderStats {
        self.stats.borrow().clone()
    }
}

fn engine_failure(error: &PdfError) -> EngineError {
    EngineError::Failed(match error {
        PdfError::Library(_) => "PDF engine unavailable",
        PdfError::Open(_) => "the PDF could not be opened",
        PdfError::Page(_) => "a page could not be rendered",
        PdfError::DegeneratePage => "a page has no usable size",
        PdfError::RenderTooLarge => "the requested image is too large",
        PdfError::TooManyPages(_) => "the PDF has too many pages",
    })
}

/// Why [`render_pages`] stopped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderFailure {
    /// The checkpoint stopped the work between pages.
    Interrupted(Interruption),
    /// PDFium could not render a page.
    Pdf(PdfError),
    /// The document has no pages.
    NoPages,
}

impl From<RenderFailure> for EngineError {
    fn from(failure: RenderFailure) -> Self {
        match failure {
            RenderFailure::Interrupted(interruption) => Self::Interrupted(interruption),
            RenderFailure::Pdf(error) => engine_failure(&error),
            RenderFailure::NoPages => Self::Failed("the PDF has no pages"),
        }
    }
}

/// Renders every page of `document` inside `max_width` x `max_height`,
/// calling `checkpoint` before each page, and returns the first page.
/// `rendered` is called after each page with the time it took.
///
/// # Errors
///
/// [`RenderFailure`] for an empty document, a page PDFium cannot render, or
/// an interruption.
pub fn render_pages(
    document: &Document<'_>,
    max_width: u32,
    max_height: u32,
    mut checkpoint: impl FnMut() -> Result<(), Interruption>,
    mut rendered: impl FnMut(Duration),
) -> Result<PageImage, RenderFailure> {
    let mut first = None;
    for index in 0..document.page_count() {
        checkpoint().map_err(RenderFailure::Interrupted)?;
        let started = Instant::now();
        let image = document
            .render(index, max_width, max_height)
            .map_err(RenderFailure::Pdf)?;
        rendered(started.elapsed());
        if first.is_none() {
            first = Some(image);
        }
    }
    first.ok_or(RenderFailure::NoPages)
}

/// Writes `image` as a binary PPM (`P6`), the job's verifiable output.
///
/// # Errors
///
/// Any error from `output`.
pub fn write_ppm(image: &PageImage, output: &mut impl Write) -> std::io::Result<()> {
    write!(output, "P6\n{} {}\n255\n", image.width, image.height)?;
    let (pixels, _) = image.rgba.as_chunks::<4>();
    for [red, green, blue, _alpha] in pixels {
        output.write_all(&[*red, *green, *blue])?;
    }
    output.flush()
}

impl Processor for RenderProcessor<'_> {
    fn process(&self, context: &mut ProcessContext<'_>) -> Result<(), EngineError> {
        *self.stats.borrow_mut() = RenderStats::default();
        let mut bytes = Vec::new();
        std::fs::File::open(context.input_path())?.read_to_end(&mut bytes)?;
        context.checkpoint()?;

        let started = Instant::now();
        let report = precheck(&bytes, &self.limits, || context.checkpoint())?;
        for notice in report.notices() {
            context.notice(notice);
        }
        {
            let mut stats = self.stats.borrow_mut();
            stats.precheck_time = started.elapsed();
            stats.precheck = Some(report);
        }

        let started = Instant::now();
        let document = self
            .engine
            .open(&bytes)
            .map_err(|error| engine_failure(&error))?;
        let pages = document.page_count();
        {
            let mut stats = self.stats.borrow_mut();
            stats.open_time = started.elapsed();
            stats.pages = pages;
            stats.first_page_text = document.text(0).ok();
        }
        let image = render_pages(
            &document,
            self.max_width,
            self.max_height,
            || context.checkpoint(),
            |took| {
                let mut stats = self.stats.borrow_mut();
                stats.render_time += took;
                stats.rendered += 1;
            },
        )?;
        write_ppm(&image, &mut context.create_output()?)?;
        Ok(())
    }
}

/// Accepts a binary PPM whose header matches its length.
#[derive(Clone, Copy, Debug, Default)]
pub struct PpmVerifier;

impl OutputVerifier for PpmVerifier {
    fn verify(&self, output: &Path, len: u64) -> Result<(), Rejection> {
        let mut header = [0_u8; 32];
        let mut file =
            std::fs::File::open(output).map_err(|_| Rejection("the image cannot be read"))?;
        let read = file
            .read(&mut header)
            .map_err(|_| Rejection("the image cannot be read"))?;
        // The header is ASCII; pixel bytes after it can be anything.
        let text: String = header
            .get(..read)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| byte.is_ascii())
            .map(|&byte| char::from(byte))
            .collect();
        let mut fields = text.split_ascii_whitespace();
        let (Some("P6"), Some(width), Some(height), Some("255")) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(Rejection("the output is not a PPM image"));
        };
        let (Ok(width), Ok(height)) = (width.parse::<u64>(), height.parse::<u64>()) else {
            return Err(Rejection("the image size is not a number"));
        };
        let header_len = format!("P6\n{width} {height}\n255\n").len() as u64;
        if width == 0
            || height == 0
            || width * height > MAX_RENDER_PIXELS
            || header_len + width * height * 3 != len
        {
            return Err(Rejection("the image size does not match its data"));
        }
        Ok(())
    }
}
