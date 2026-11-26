//! Input validation from content, never from file names
//! (`PRODUCT_INVARIANTS.md` 10).
//!
//! Validators see a copy of the input already staged in the job's workspace:
//! its length and its first [`HEADER_LEN`] bytes. They never see the original
//! path or name. Format-specific structure checks belong to engines and are
//! added with them.

use std::fmt;

/// How many leading bytes of the input validators can inspect.
pub const HEADER_LEN: usize = 4096;

/// What a validator knows about a staged input.
#[derive(Debug)]
pub struct StagedInput<'a> {
    header: &'a [u8],
    len: u64,
}

impl<'a> StagedInput<'a> {
    pub(crate) fn new(header: &'a [u8], len: u64) -> Self {
        Self { header, len }
    }

    /// The first bytes of the input, at most [`HEADER_LEN`].
    pub fn header(&self) -> &[u8] {
        self.header
    }

    /// The input's full length in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether the input is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Why a validator or verifier refused a file. The reason is shown to the user
/// and must not contain document content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rejection(pub &'static str);

impl fmt::Display for Rejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for Rejection {}

/// Decides whether a staged input may be processed.
pub trait InputValidator {
    /// Accepts or rejects `input`.
    ///
    /// # Errors
    ///
    /// Returns a [`Rejection`] for input the operation must not process.
    fn validate(&self, input: &StagedInput<'_>) -> Result<(), Rejection>;
}

/// Accepts inputs that carry `signature` at byte `offset`.
#[derive(Clone, Copy, Debug)]
pub struct MagicBytes {
    offset: usize,
    signature: &'static [u8],
}

impl MagicBytes {
    /// Creates a validator for `signature` at `offset`.
    pub const fn new(offset: usize, signature: &'static [u8]) -> Self {
        Self { offset, signature }
    }
}

impl InputValidator for MagicBytes {
    fn validate(&self, input: &StagedInput<'_>) -> Result<(), Rejection> {
        if self.signature.is_empty() {
            return Err(Rejection("validator has no signature to check"));
        }
        let found = self
            .offset
            .checked_add(self.signature.len())
            .and_then(|end| input.header().get(self.offset..end));
        if found == Some(self.signature) {
            Ok(())
        } else {
            Err(Rejection("file content does not match the expected type"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDF: MagicBytes = MagicBytes::new(0, b"%PDF-");

    #[test]
    fn matching_signature_is_accepted() {
        assert_eq!(PDF.validate(&StagedInput::new(b"%PDF-1.7\n", 9)), Ok(()));
        let shifted = MagicBytes::new(2, b"AB");
        assert_eq!(shifted.validate(&StagedInput::new(b"xxAB", 4)), Ok(()));
    }

    #[test]
    fn wrong_short_or_empty_input_is_rejected() {
        for header in [&b"PK\x03\x04"[..], b"%PD", b""] {
            let input = StagedInput::new(header, header.len() as u64);
            assert!(PDF.validate(&input).is_err(), "{header:?}");
        }
    }

    #[test]
    fn offset_overflow_is_rejected_not_panicking() {
        let validator = MagicBytes::new(usize::MAX, b"A");
        assert!(validator.validate(&StagedInput::new(b"A", 1)).is_err());
    }

    #[test]
    fn empty_signature_never_accepts() {
        let validator = MagicBytes::new(0, b"");
        assert!(validator
            .validate(&StagedInput::new(b"anything", 8))
            .is_err());
    }
}
