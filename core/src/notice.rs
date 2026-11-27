//! What the user must be told about a document that was processed anyway:
//! the corpus outcomes `repair` and `neutralize` (`docs/ci/corpus.md`).
//!
//! Notices are typed data, never text, so every layer up to the interface can
//! branch on them and the interface owns the wording.

/// Something about a processed document the user must be told.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Notice {
    /// The file was damaged and opened only after recovery. Some content may
    /// be missing or differ from what its author made.
    Repaired,
    /// Content that could act on its own was found and never run
    /// (`PRODUCT_INVARIANTS.md` 6 and 7).
    NotRun(Inert),
}

/// Kinds of content PaperWarden never runs or follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Inert {
    /// Scripts or macros.
    Scripts,
    /// Actions that would start an application or open a file.
    LaunchActions,
    /// Links to web addresses.
    WebLinks,
    /// Forms that would send their data somewhere.
    FormSubmission,
    /// Actions that would import data from another file.
    DataImport,
    /// Links that would open other documents.
    OtherDocuments,
    /// Files embedded in the document.
    EmbeddedFiles,
    /// Movies, sounds, rich media or 3D content.
    Media,
    /// Interactive forms that need a scripting engine to work.
    InteractiveForms,
}
