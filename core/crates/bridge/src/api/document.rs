//! Checking a document before it is shown (#61): what the user must be told
//! about it, or why it was refused.
//!
//! Everything crosses the bridge as typed values, never text, so Dart owns
//! the wording and every screen shows the same warning for the same finding.
//! The check reads the file into memory under the job's input limit and
//! writes nothing.

use std::fs::File;
use std::io::Read;

use paperwarden_core::{Inert, JobLimits, Notice};
use paperwarden_pdf_scan::{scan, Interrupted, Limit, ScanError, ScanLimits};

use super::probe::{BridgeError, BridgeErrorKind, CancelHandle};

/// Something the user must be told about a document that opens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentNotice {
    /// The file was damaged and opened only after recovery; some content may
    /// be missing or look different.
    Repaired,
    /// Scripts were found and not run.
    Scripts,
    /// Actions that would start an app or open a file were found and not run.
    LaunchActions,
    /// Links to web addresses were found; none is opened automatically.
    WebLinks,
    /// A form that would send its data somewhere was found and not sent.
    FormSubmission,
    /// An action that would import data from another file was not run.
    DataImport,
    /// Links that would open other documents were not followed.
    OtherDocuments,
    /// Files embedded in the document were not opened.
    EmbeddedFiles,
    /// Movies, sounds or other media were not played.
    Media,
    /// An interactive form that needs a scripting engine was not run.
    InteractiveForms,
}

/// Why a document was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefusalReason {
    /// The file is not a PDF.
    NotPdf,
    /// The PDF has no content to show.
    NoContent,
    /// The PDF is encrypted, which is not supported yet.
    Encrypted,
    /// The file is larger than PaperWarden opens.
    TooLarge,
    /// The PDF nests its contents more deeply than real documents do.
    NestedTooDeeply,
    /// The PDF has more parts than PaperWarden opens.
    TooManyParts,
    /// The PDF has more saved revisions than PaperWarden opens.
    TooManyRevisions,
    /// Part of the PDF expands far beyond what real content does.
    ExpandsTooMuch,
    /// An image in the PDF is larger than PaperWarden opens.
    ImageTooLarge,
}

/// The result of checking a document: refused, or allowed with notices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentCheck {
    /// Why the document was refused, or `None` if it may be opened.
    pub refusal: Option<RefusalReason>,
    /// What the user must be told when it opens. Empty when refused.
    pub notices: Vec<DocumentNotice>,
}

pub(crate) fn document_notice(notice: Notice) -> DocumentNotice {
    match notice {
        Notice::Repaired => DocumentNotice::Repaired,
        Notice::NotRun(Inert::Scripts) => DocumentNotice::Scripts,
        Notice::NotRun(Inert::LaunchActions) => DocumentNotice::LaunchActions,
        Notice::NotRun(Inert::WebLinks) => DocumentNotice::WebLinks,
        Notice::NotRun(Inert::FormSubmission) => DocumentNotice::FormSubmission,
        Notice::NotRun(Inert::DataImport) => DocumentNotice::DataImport,
        Notice::NotRun(Inert::OtherDocuments) => DocumentNotice::OtherDocuments,
        Notice::NotRun(Inert::EmbeddedFiles) => DocumentNotice::EmbeddedFiles,
        Notice::NotRun(Inert::Media) => DocumentNotice::Media,
        Notice::NotRun(Inert::InteractiveForms) => DocumentNotice::InteractiveForms,
    }
}

fn refused(reason: RefusalReason) -> DocumentCheck {
    DocumentCheck {
        refusal: Some(reason),
        notices: Vec::new(),
    }
}

fn unreadable(error: &std::io::Error) -> BridgeError {
    BridgeError {
        kind: BridgeErrorKind::Unreadable,
        message: format!("cannot read the file: {}", error.kind()),
    }
}

/// Checks the PDF at `path` with PaperWarden's pre-check (ADR-0007).
///
/// Not `sync`: Dart gets a `Future`, so a large file never blocks the UI.
///
/// # Errors
///
/// - [`BridgeErrorKind::Unreadable`] if the file cannot be read.
/// - [`BridgeErrorKind::Cancelled`] if `cancel` is cancelled.
pub fn check_pdf(path: String, cancel: &CancelHandle) -> Result<DocumentCheck, BridgeError> {
    let file = File::open(&path).map_err(|error| unreadable(&error))?;
    let metadata = file.metadata().map_err(|error| unreadable(&error))?;
    if !metadata.is_file() {
        return Err(BridgeError {
            kind: BridgeErrorKind::Unreadable,
            message: "not a regular file".to_owned(),
        });
    }
    let limit = JobLimits::default().max_input_bytes();
    if metadata.len() > limit {
        return Ok(refused(RefusalReason::TooLarge));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| unreadable(&error))?;
    if bytes.len() as u64 > limit {
        return Ok(refused(RefusalReason::TooLarge));
    }
    let token = cancel.token();
    token.check()?;

    let result = scan(&bytes, &ScanLimits::default(), &mut || {
        token.check().map_err(|_| Interrupted)
    });
    let reason = match result {
        Ok(report) => {
            return Ok(DocumentCheck {
                refusal: None,
                notices: report.notices().into_iter().map(document_notice).collect(),
            });
        }
        Err(ScanError::Interrupted) => {
            token.check()?;
            return Err(BridgeError::invalid_input("the check was interrupted"));
        }
        Err(ScanError::NotPdf) => RefusalReason::NotPdf,
        Err(ScanError::NoObjects) => RefusalReason::NoContent,
        Err(ScanError::Encrypted) => RefusalReason::Encrypted,
        Err(ScanError::Limit(Limit::Nesting)) => RefusalReason::NestedTooDeeply,
        Err(ScanError::Limit(Limit::Objects)) => RefusalReason::TooManyParts,
        Err(ScanError::Limit(Limit::Sections)) => RefusalReason::TooManyRevisions,
        Err(ScanError::Limit(
            Limit::StreamSize | Limit::CompressionRatio | Limit::TotalDecoded,
        )) => RefusalReason::ExpandsTooMuch,
        Err(ScanError::Limit(Limit::ImageSize)) => RefusalReason::ImageTooLarge,
    };
    Ok(refused(reason))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn corpus(relative: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/corpus")
            .join(relative)
            .to_string_lossy()
            .into_owned()
    }

    fn check(relative: &str) -> DocumentCheck {
        check_pdf(corpus(relative), &CancelHandle::new()).expect("checked")
    }

    #[test]
    fn a_clean_pdf_has_no_notices() {
        assert_eq!(
            check("valid/pdf/one-page.pdf"),
            DocumentCheck {
                refusal: None,
                notices: vec![]
            }
        );
    }

    #[test]
    fn repairs_and_inert_content_become_typed_notices() {
        assert_eq!(
            check("malformed/pdf/no-eof-marker.pdf").notices,
            [DocumentNotice::Repaired]
        );
        assert_eq!(
            check("security/pdf/launch-and-uri-actions.pdf").notices,
            [DocumentNotice::LaunchActions, DocumentNotice::WebLinks]
        );
    }

    #[test]
    fn hostile_and_invalid_files_are_refused_with_a_typed_reason() {
        for (fixture, reason) in [
            ("security/pdf/flate-bomb.pdf", RefusalReason::ExpandsTooMuch),
            (
                "security/pdf/deep-nesting.pdf",
                RefusalReason::NestedTooDeeply,
            ),
            ("malformed/pdf/header-only.pdf", RefusalReason::NoContent),
            ("valid/minimal.txt", RefusalReason::NotPdf),
        ] {
            assert_eq!(check(fixture), refused(reason), "{fixture}");
        }
    }

    #[test]
    fn a_missing_file_is_unreadable_and_cancel_stops_the_check() {
        let missing = check_pdf(corpus("no/such.pdf"), &CancelHandle::new()).unwrap_err();
        assert_eq!(missing.kind, BridgeErrorKind::Unreadable);
        assert!(!missing.message.contains("such"), "{}", missing.message);

        let cancel = CancelHandle::new();
        cancel.cancel();
        let error = check_pdf(corpus("valid/pdf/one-page.pdf"), &cancel).unwrap_err();
        assert_eq!(error.kind, BridgeErrorKind::Cancelled);
    }
}
