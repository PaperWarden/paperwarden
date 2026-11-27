//! PaperWarden's PDF pre-check (ADR-0007): inspects a PDF's structure itself,
//! before PDFium sees it, and bounds what PDFium would otherwise decode
//! unbounded.
//!
//! - **Size is measured by decoding, never by file size.** Every stream is
//!   decoded into a counter that discards the output, against a per-stream
//!   cap, a compression-ratio cap, a document total and, for images, the size
//!   the image's own dimensions imply. JPEG and JPEG 2000 dimensions are read
//!   from the codestream, so a small header cannot hide a huge image.
//! - **Structure is bounded:** object nesting, object count and the number of
//!   cross-reference sections. `/Prev` cycles are detected.
//! - **Damage is detected**, for the corpus `repair` outcome: cross-reference
//!   offsets that do not point at their objects, a missing `%%EOF`, a `/Prev`
//!   cycle, a `/Length` that does not match the data, and corrupt or truncated
//!   streams.
//! - **Active content is flagged**, for the `neutralize` outcome, including
//!   inside compressed object streams: JavaScript, Launch, URI, SubmitForm,
//!   ImportData, remote go-to actions, embedded files, rich media and XFA.
//!
//! Objects are found the way PDFium rebuilds a damaged file, by scanning for
//! `N G obj` headers rather than trusting the cross-reference table, so damage
//! cannot hide objects from the checks.
//!
//! **Not covered yet:** encrypted PDFs are refused, because their streams
//! cannot be measured without decrypting them. Inline images in content
//! streams, sampled functions and Type 3 fonts are not bounded here.

mod filters;
mod scan;
mod syntax;
mod xref;

use paperwarden_core::{Inert, Notice};
use std::fmt;

/// Bounds for one document. The defaults are provisional; see each field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanLimits {
    /// Deepest nesting of arrays and dictionaries. PDFium silently truncates
    /// beyond 64; real documents stay far below 32.
    pub max_nesting: u32,
    /// Most indirect objects, including those inside object streams.
    pub max_objects: u32,
    /// Most cross-reference sections in the `/Prev` chain.
    pub max_sections: u32,
    /// Largest decoded size of one non-image stream, in bytes.
    pub max_stream_bytes: u64,
    /// Largest ratio of decoded to encoded size for non-image streams, applied
    /// once a stream has decoded `ratio_floor` bytes.
    pub max_ratio: u64,
    /// Decoded size below which the ratio is not checked.
    pub ratio_floor: u64,
    /// Largest total decoded size across the document, in bytes.
    pub max_total_bytes: u64,
    /// Most pixels in one image. 40 million covers an A4 page scanned at 600
    /// dpi (35 million) with room to spare.
    pub max_image_pixels: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_nesting: 32,
            max_objects: 1_000_000,
            max_sections: 64,
            max_stream_bytes: 64 << 20,
            max_ratio: 256,
            ratio_floor: 1 << 20,
            max_total_bytes: 1 << 30,
            max_image_pixels: 40_000_000,
        }
    }
}

/// A bound the document exceeded. The job stops with a limit error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    /// Arrays or dictionaries nested too deeply.
    Nesting,
    /// Too many objects.
    Objects,
    /// Too many cross-reference sections.
    Sections,
    /// A stream decodes to more than the per-stream cap.
    StreamSize,
    /// A stream expands far more than real content does.
    CompressionRatio,
    /// The document decodes to more than the total cap.
    TotalDecoded,
    /// An image is larger than the pixel cap, or its data exceeds what its
    /// dimensions allow.
    ImageSize,
}

impl Limit {
    /// A short reason for the user. It contains no document content.
    pub fn reason(self) -> &'static str {
        match self {
            Self::Nesting => "the PDF nests objects too deeply",
            Self::Objects => "the PDF has too many objects",
            Self::Sections => "the PDF has too many revisions",
            Self::StreamSize => "part of the PDF expands beyond the size limit",
            Self::CompressionRatio => "part of the PDF expands far more than real content does",
            Self::TotalDecoded => "the PDF expands beyond the total size limit",
            Self::ImageSize => "an image in the PDF is too large",
        }
    }
}

impl fmt::Display for Limit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

/// Damage a viewer can recover from; the file opens with a warning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Damage {
    /// The cross-reference table is missing or points at the wrong places.
    CrossReference,
    /// The file does not end with `%%EOF`.
    MissingEndOfFile,
    /// The `/Prev` chain of cross-reference sections loops.
    PrevCycle,
    /// A stream's `/Length` does not match its data.
    StreamLength,
    /// A stream's compressed data is invalid.
    CorruptStream,
    /// The file ends inside an object or stream.
    Truncated,
}

/// Content that could act on its own, which PaperWarden never runs and
/// reports to the user (`PRODUCT_INVARIANTS.md` 6 and 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActiveContent {
    /// JavaScript, anywhere in the document.
    JavaScript,
    /// An action that launches an application or file.
    Launch,
    /// A link or action to a web address.
    Uri,
    /// A form that submits its data somewhere.
    SubmitForm,
    /// An action that imports form data from a file.
    ImportData,
    /// An action that opens another document.
    RemoteGoTo,
    /// Files embedded in the document.
    EmbeddedFile,
    /// Movies, sounds, rich media or 3D content.
    RichMedia,
    /// An XFA form, which PaperWarden's PDFium build cannot run.
    Xfa,
}

impl ActiveContent {
    /// The kind of content the user is told was not run.
    pub fn inert(self) -> Inert {
        match self {
            Self::JavaScript => Inert::Scripts,
            Self::Launch => Inert::LaunchActions,
            Self::Uri => Inert::WebLinks,
            Self::SubmitForm => Inert::FormSubmission,
            Self::ImportData => Inert::DataImport,
            Self::RemoteGoTo => Inert::OtherDocuments,
            Self::EmbeddedFile => Inert::EmbeddedFiles,
            Self::RichMedia => Inert::Media,
            Self::Xfa => Inert::InteractiveForms,
        }
    }

    const ALL: [Self; 9] = [
        Self::JavaScript,
        Self::Launch,
        Self::Uri,
        Self::SubmitForm,
        Self::ImportData,
        Self::RemoteGoTo,
        Self::EmbeddedFile,
        Self::RichMedia,
        Self::Xfa,
    ];
}

/// Why the pre-check refused a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanError {
    /// No `%PDF-` signature near the start.
    NotPdf,
    /// No objects at all.
    NoObjects,
    /// The document is encrypted; its streams cannot be measured.
    Encrypted,
    /// A bound was exceeded.
    Limit(Limit),
    /// The checkpoint asked the scan to stop.
    Interrupted,
}

impl ScanError {
    /// A short reason for the user. It contains no document content.
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotPdf => "the file is not a PDF",
            Self::NoObjects => "the PDF has no content",
            Self::Encrypted => "encrypted PDFs are not supported yet",
            Self::Limit(limit) => limit.reason(),
            Self::Interrupted => "the check was stopped",
        }
    }
}

impl fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for ScanError {}

/// What the pre-check found in a document it allows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Recoverable damage, each kind once, in a fixed order.
    pub damage: Vec<Damage>,
    /// Active content found, each kind once, in a fixed order.
    pub active_content: Vec<ActiveContent>,
    /// Indirect objects found, including inside object streams.
    pub objects: u32,
    /// Streams found.
    pub streams: u32,
    /// Bytes decoded while measuring streams.
    pub decoded_bytes: u64,
}

impl Report {
    /// Whether the document needs the damaged-file warning.
    pub fn is_damaged(&self) -> bool {
        !self.damage.is_empty()
    }

    /// What the user must be told, as the job runner's notices: one
    /// [`Notice::Repaired`] for any damage, and what content was not run.
    pub fn notices(&self) -> Vec<Notice> {
        let mut notices: Vec<Notice> = self
            .active_content
            .iter()
            .map(|kind| Notice::NotRun(kind.inert()))
            .collect();
        if self.is_damaged() {
            notices.push(Notice::Repaired);
        }
        notices.sort();
        notices.dedup();
        notices
    }
}

/// Returned by a checkpoint to stop the scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interrupted;

/// Inspects `data` as a PDF.
///
/// `checkpoint` is called regularly; returning [`Interrupted`] stops the scan
/// with [`ScanError::Interrupted`], so cancellation and the job's time limit
/// apply.
///
/// # Errors
///
/// Returns a [`ScanError`] for a document PaperWarden must not open.
pub fn scan(
    data: &[u8],
    limits: &ScanLimits,
    checkpoint: &mut dyn FnMut() -> Result<(), Interrupted>,
) -> Result<Report, ScanError> {
    scan::Scanner::new(data, limits, checkpoint).run()
}
