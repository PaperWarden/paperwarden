//! Archive entry paths (`PRODUCT_INVARIANTS.md` 11).
//!
//! [`EntryPath::parse`] decides whether a name read from an archive may become
//! a file inside a job workspace. It accepts only plain relative paths whose
//! every component is a portable file name, so the result cannot escape the
//! extraction directory or alias another file on any supported platform.
//!
//! This is the path rule only. Extraction itself, links, nesting of archives
//! and decompression bounds arrive with the archive engine.

use std::fmt;
use std::path::PathBuf;

/// Longest entry name accepted, in bytes.
pub const MAX_PATH_BYTES: usize = 1024;
/// Longest single path component accepted, in bytes.
pub const MAX_COMPONENT_BYTES: usize = 255;
/// Deepest directory nesting accepted.
pub const MAX_DEPTH: usize = 32;

/// A validated archive entry path: relative, normalised and portable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryPath {
    components: Vec<String>,
    is_directory: bool,
}

/// Why an entry name was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryPathError {
    /// The name is empty.
    Empty,
    /// The name exceeds [`MAX_PATH_BYTES`].
    TooLong,
    /// A component exceeds [`MAX_COMPONENT_BYTES`].
    ComponentTooLong,
    /// The path nests deeper than [`MAX_DEPTH`].
    TooDeep,
    /// The name is not valid UTF-8; callers decode legacy encodings first.
    NotUtf8,
    /// The path starts at a root, such as `/etc/passwd`.
    Absolute,
    /// The path contains a `..` component.
    Traversal,
    /// The path contains an empty or `.` component, such as `a//b` or `./a`.
    NotNormalised,
    /// The path uses `\`, which is a separator on Windows.
    Backslash,
    /// The path contains a control character, including NUL.
    ControlCharacter,
    /// The path contains a bidirectional-text control that can disguise a
    /// file's real extension.
    BidiControl,
    /// The path contains `:` or another character that Windows or Android
    /// storage reserves, such as a drive letter or an alternate data stream.
    ReservedCharacter,
    /// A component is a reserved device name, such as `CON` or `nul.txt`.
    ReservedName,
    /// A component ends with `.` or a space, which Windows strips, so two
    /// entries could name the same file.
    TrailingDotOrSpace,
}

impl fmt::Display for EntryPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "archive entry has an empty name",
            Self::TooLong => "archive entry name is too long",
            Self::ComponentTooLong => "archive entry has a name part that is too long",
            Self::TooDeep => "archive entry is nested too deeply",
            Self::NotUtf8 => "archive entry name is not valid UTF-8",
            Self::Absolute => "archive entry has an absolute path",
            Self::Traversal => "archive entry points outside the archive",
            Self::NotNormalised => "archive entry path has empty or '.' parts",
            Self::Backslash => "archive entry path contains a backslash",
            Self::ControlCharacter => "archive entry name contains a control character",
            Self::BidiControl => "archive entry name contains a text-direction control",
            Self::ReservedCharacter => "archive entry name contains a reserved character",
            Self::ReservedName => "archive entry uses a reserved device name",
            Self::TrailingDotOrSpace => "archive entry name ends with a dot or space",
        })
    }
}

impl std::error::Error for EntryPathError {}

impl EntryPath {
    /// Validates a raw entry name. A single trailing `/` marks a directory.
    ///
    /// # Errors
    ///
    /// Returns the first rule the name breaks.
    pub fn parse(raw: &[u8]) -> Result<Self, EntryPathError> {
        if raw.is_empty() {
            return Err(EntryPathError::Empty);
        }
        if raw.len() > MAX_PATH_BYTES {
            return Err(EntryPathError::TooLong);
        }
        let text = std::str::from_utf8(raw).map_err(|_| EntryPathError::NotUtf8)?;
        for character in text.chars() {
            check_character(character)?;
        }
        if text.starts_with('/') {
            return Err(EntryPathError::Absolute);
        }
        let (body, is_directory) = match text.strip_suffix('/') {
            Some(body) => (body, true),
            None => (text, false),
        };

        let mut components = Vec::new();
        for component in body.split('/') {
            check_component(component)?;
            if components.len() >= MAX_DEPTH {
                return Err(EntryPathError::TooDeep);
            }
            components.push(component.to_owned());
        }
        Ok(Self {
            components,
            is_directory,
        })
    }

    /// The path's components, outermost first. Never empty.
    pub fn components(&self) -> &[String] {
        &self.components
    }

    /// Whether the entry names a directory (its raw name ended with `/`).
    pub fn is_directory(&self) -> bool {
        self.is_directory
    }

    /// The path relative to an extraction directory.
    pub fn to_relative_path(&self) -> PathBuf {
        self.components.iter().collect()
    }
}

fn check_character(character: char) -> Result<(), EntryPathError> {
    match character {
        '\\' => Err(EntryPathError::Backslash),
        ':' | '<' | '>' | '"' | '|' | '?' | '*' => Err(EntryPathError::ReservedCharacter),
        '\u{200e}'
        | '\u{200f}'
        | '\u{061c}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}' => Err(EntryPathError::BidiControl),
        character if character.is_control() => Err(EntryPathError::ControlCharacter),
        _ => Ok(()),
    }
}

fn check_component(component: &str) -> Result<(), EntryPathError> {
    match component {
        "" | "." => return Err(EntryPathError::NotNormalised),
        ".." => return Err(EntryPathError::Traversal),
        _ => {}
    }
    if component.len() > MAX_COMPONENT_BYTES {
        return Err(EntryPathError::ComponentTooLong);
    }
    if component.ends_with('.') || component.ends_with(' ') {
        return Err(EntryPathError::TrailingDotOrSpace);
    }
    if is_reserved_device_name(component) {
        return Err(EntryPathError::ReservedName);
    }
    Ok(())
}

/// Windows device names, reserved with or without an extension and in any
/// case, including the superscript-digit forms Windows also treats as devices.
fn is_reserved_device_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let mut characters = stem.chars();
    let prefix: String = characters.by_ref().take(3).collect();
    let rest: String = characters.collect();
    (prefix == "COM" || prefix == "LPT")
        && matches!(
            rest.as_str(),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "\u{b9}" | "\u{b2}" | "\u{b3}"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<EntryPath, EntryPathError> {
        EntryPath::parse(raw.as_bytes())
    }

    #[test]
    fn plain_relative_paths_are_accepted() {
        let path = parse("docs/report final.pdf").expect("valid");
        assert_eq!(path.components(), ["docs", "report final.pdf"]);
        assert!(!path.is_directory());
        assert_eq!(
            path.to_relative_path(),
            PathBuf::from("docs").join("report final.pdf")
        );
        let directory = parse("docs/images/").expect("valid directory");
        assert!(directory.is_directory());
        assert_eq!(directory.components(), ["docs", "images"]);
        assert!(parse(".hidden").is_ok());
        assert!(parse("archive..tar").is_ok());
        assert!(parse("résumé/日本語.txt").is_ok());
        assert!(parse("console.txt").is_ok());
        assert!(parse("COM10").is_ok());
    }

    #[test]
    fn escaping_paths_are_rejected() {
        let cases = [
            ("../etc/passwd", EntryPathError::Traversal),
            ("a/../../b", EntryPathError::Traversal),
            ("..", EntryPathError::Traversal),
            ("/etc/passwd", EntryPathError::Absolute),
            ("//server/share", EntryPathError::Absolute),
            ("C:/Windows/x", EntryPathError::ReservedCharacter),
            ("..\\..\\x", EntryPathError::Backslash),
            ("\\\\server\\share", EntryPathError::Backslash),
            ("a/./b", EntryPathError::NotNormalised),
            ("a//b", EntryPathError::NotNormalised),
            ("./a", EntryPathError::NotNormalised),
            ("/", EntryPathError::Absolute),
        ];
        for (raw, expected) in cases {
            assert_eq!(parse(raw), Err(expected), "{raw:?}");
        }
    }

    #[test]
    fn non_portable_names_are_rejected() {
        let cases = [
            ("file\0.pdf", EntryPathError::ControlCharacter),
            ("line\nbreak", EntryPathError::ControlCharacter),
            ("invoice\u{202e}fdp.exe", EntryPathError::BidiControl),
            ("a\u{2067}b", EntryPathError::BidiControl),
            ("report.pdf:stream", EntryPathError::ReservedCharacter),
            ("what?.txt", EntryPathError::ReservedCharacter),
            ("CON", EntryPathError::ReservedName),
            ("docs/nul.txt", EntryPathError::ReservedName),
            ("lpt9.log", EntryPathError::ReservedName),
            ("COM\u{b9}", EntryPathError::ReservedName),
            ("con .txt", EntryPathError::ReservedName),
            ("trailing.", EntryPathError::TrailingDotOrSpace),
            ("trailing ", EntryPathError::TrailingDotOrSpace),
        ];
        for (raw, expected) in cases {
            assert_eq!(parse(raw), Err(expected), "{raw:?}");
        }
    }

    #[test]
    fn size_and_depth_are_bounded() {
        assert_eq!(EntryPath::parse(b""), Err(EntryPathError::Empty));
        assert_eq!(
            EntryPath::parse(&[b'a'; MAX_PATH_BYTES + 1]),
            Err(EntryPathError::TooLong)
        );
        let long_component = "a".repeat(MAX_COMPONENT_BYTES + 1);
        assert_eq!(
            parse(&long_component),
            Err(EntryPathError::ComponentTooLong)
        );
        let at_limit = vec!["d"; MAX_DEPTH].join("/");
        assert!(parse(&at_limit).is_ok());
        let too_deep = vec!["d"; MAX_DEPTH + 1].join("/");
        assert_eq!(parse(&too_deep), Err(EntryPathError::TooDeep));
        assert_eq!(
            EntryPath::parse(&[0xff, 0xfe]),
            Err(EntryPathError::NotUtf8)
        );
    }
}
