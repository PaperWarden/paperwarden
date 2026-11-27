//! Archive entry paths (`PRODUCT_INVARIANTS.md` 11).
//!
//! [`EntryPath::parse`] and [`EntryPath::parse_cp437`] decide whether a name
//! read from an archive may become a file inside a job workspace. They accept
//! only plain relative paths, so the result cannot escape the extraction
//! directory, and they refuse names that can disguise what a file is.
//!
//! The rules target the platforms PaperWarden ships on, Android and iOS.
//! Characters and device names that only Windows reserves (`<>"|?*`, `CON`,
//! trailing dots and spaces) are allowed; revisit them if a Windows build is
//! ever added. Because names that differ only in case can still collide on a
//! case-insensitive file system, extraction must create every file with
//! `create_new` and never overwrite.
//!
//! This is the path rule only. Extraction itself, links, nesting of archives
//! and decompression bounds arrive with the archive engine.

use std::fmt;
use std::path::PathBuf;

/// Longest entry name accepted, in bytes of UTF-8.
pub const MAX_PATH_BYTES: usize = 1024;
/// Longest single path component accepted, in bytes of UTF-8.
pub const MAX_COMPONENT_BYTES: usize = 255;
/// Deepest directory nesting accepted.
pub const MAX_DEPTH: usize = 32;

/// A validated archive entry path: relative and normalised.
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
    /// The name is not valid UTF-8. Archives that store legacy names (a ZIP
    /// entry without the UTF-8 flag) go through [`EntryPath::parse_cp437`].
    NotUtf8,
    /// The path starts at a root, such as `/etc/passwd`.
    Absolute,
    /// The path contains a `..` component.
    Traversal,
    /// The path contains an empty or `.` component, such as `a//b` or `./a`.
    NotNormalised,
    /// The path uses `\`, which is a separator on Windows and in archives
    /// written by some Windows tools.
    Backslash,
    /// The path contains `:`, which names a drive or an alternate data stream
    /// on Windows and is shown as `/` by macOS and iOS file browsers.
    Colon,
    /// The path contains a control character, including NUL.
    ControlCharacter,
    /// The path contains a bidirectional-text control that can disguise a
    /// file's real extension.
    BidiControl,
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
            Self::Colon => "archive entry name contains a colon",
            Self::ControlCharacter => "archive entry name contains a control character",
            Self::BidiControl => "archive entry name contains a text-direction control",
        })
    }
}

impl std::error::Error for EntryPathError {}

impl EntryPath {
    /// Validates a UTF-8 entry name. A single trailing `/` marks a directory.
    ///
    /// # Errors
    ///
    /// Returns the first rule the name breaks.
    pub fn parse(raw: &[u8]) -> Result<Self, EntryPathError> {
        if raw.len() > MAX_PATH_BYTES {
            return Err(EntryPathError::TooLong);
        }
        let text = std::str::from_utf8(raw).map_err(|_| EntryPathError::NotUtf8)?;
        Self::parse_text(text)
    }

    /// Decodes a legacy entry name as code page 437, the ZIP default when an
    /// entry's UTF-8 flag is not set, then validates it like [`parse`].
    /// Decoding never fails; every rule applies to the decoded name.
    ///
    /// # Errors
    ///
    /// Returns the first rule the decoded name breaks.
    ///
    /// [`parse`]: Self::parse
    pub fn parse_cp437(raw: &[u8]) -> Result<Self, EntryPathError> {
        if raw.len() > MAX_PATH_BYTES {
            return Err(EntryPathError::TooLong);
        }
        let text: String = raw.iter().map(|&byte| decode_cp437(byte)).collect();
        Self::parse_text(&text)
    }

    fn parse_text(text: &str) -> Result<Self, EntryPathError> {
        if text.is_empty() {
            return Err(EntryPathError::Empty);
        }
        if text.len() > MAX_PATH_BYTES {
            return Err(EntryPathError::TooLong);
        }
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
            match component {
                "" | "." => return Err(EntryPathError::NotNormalised),
                ".." => return Err(EntryPathError::Traversal),
                _ if component.len() > MAX_COMPONENT_BYTES => {
                    return Err(EntryPathError::ComponentTooLong)
                }
                _ => {}
            }
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
        ':' => Err(EntryPathError::Colon),
        '\u{200e}'
        | '\u{200f}'
        | '\u{061c}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}' => Err(EntryPathError::BidiControl),
        character if character.is_control() => Err(EntryPathError::ControlCharacter),
        _ => Ok(()),
    }
}

/// Code page 437, bytes 0x80 to 0xFF. Bytes below 0x80 are ASCII; in names,
/// 0x00 to 0x1F stay control characters and are rejected.
const CP437_HIGH: [char; 128] = [
    '\u{00c7}', '\u{00fc}', '\u{00e9}', '\u{00e2}', '\u{00e4}', '\u{00e0}', '\u{00e5}', '\u{00e7}',
    '\u{00ea}', '\u{00eb}', '\u{00e8}', '\u{00ef}', '\u{00ee}', '\u{00ec}', '\u{00c4}', '\u{00c5}',
    '\u{00c9}', '\u{00e6}', '\u{00c6}', '\u{00f4}', '\u{00f6}', '\u{00f2}', '\u{00fb}', '\u{00f9}',
    '\u{00ff}', '\u{00d6}', '\u{00dc}', '\u{00a2}', '\u{00a3}', '\u{00a5}', '\u{20a7}', '\u{0192}',
    '\u{00e1}', '\u{00ed}', '\u{00f3}', '\u{00fa}', '\u{00f1}', '\u{00d1}', '\u{00aa}', '\u{00ba}',
    '\u{00bf}', '\u{2310}', '\u{00ac}', '\u{00bd}', '\u{00bc}', '\u{00a1}', '\u{00ab}', '\u{00bb}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255d}', '\u{255c}', '\u{255b}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252c}', '\u{251c}', '\u{2500}', '\u{253c}', '\u{255e}', '\u{255f}',
    '\u{255a}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256c}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256b}',
    '\u{256a}', '\u{2518}', '\u{250c}', '\u{2588}', '\u{2584}', '\u{258c}', '\u{2590}', '\u{2580}',
    '\u{03b1}', '\u{00df}', '\u{0393}', '\u{03c0}', '\u{03a3}', '\u{03c3}', '\u{00b5}', '\u{03c4}',
    '\u{03a6}', '\u{0398}', '\u{03a9}', '\u{03b4}', '\u{221e}', '\u{03c6}', '\u{03b5}', '\u{2229}',
    '\u{2261}', '\u{00b1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00f7}', '\u{2248}',
    '\u{00b0}', '\u{2219}', '\u{00b7}', '\u{221a}', '\u{207f}', '\u{00b2}', '\u{25a0}', '\u{00a0}',
];

fn decode_cp437(byte: u8) -> char {
    match byte.checked_sub(0x80) {
        None => char::from(byte),
        Some(index) => CP437_HIGH
            .get(usize::from(index))
            .copied()
            .unwrap_or(char::REPLACEMENT_CHARACTER),
    }
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
        for raw in [".hidden", "archive..tar", "résumé/日本語.txt", "...", ".x."] {
            assert!(parse(raw).is_ok(), "{raw:?}");
        }
    }

    #[test]
    fn names_only_windows_reserves_are_accepted() {
        for raw in [
            "CON",
            "docs/nul.txt",
            "lpt9.log",
            "what?.txt",
            "a<b>|\"*",
            "trailing.",
            "trailing ",
        ] {
            assert!(parse(raw).is_ok(), "{raw:?}");
        }
    }

    #[test]
    fn escaping_paths_are_rejected() {
        let cases = [
            ("../etc/passwd", EntryPathError::Traversal),
            ("a/../../b", EntryPathError::Traversal),
            ("..", EntryPathError::Traversal),
            ("/etc/passwd", EntryPathError::Absolute),
            ("//server/share", EntryPathError::Absolute),
            ("/", EntryPathError::Absolute),
            ("C:/Windows/x", EntryPathError::Colon),
            ("..\\..\\x", EntryPathError::Backslash),
            ("\\\\server\\share", EntryPathError::Backslash),
            ("a/./b", EntryPathError::NotNormalised),
            ("a//b", EntryPathError::NotNormalised),
            ("./a", EntryPathError::NotNormalised),
        ];
        for (raw, expected) in cases {
            assert_eq!(parse(raw), Err(expected), "{raw:?}");
        }
    }

    #[test]
    fn disguising_names_are_rejected() {
        let cases = [
            ("file\0.pdf", EntryPathError::ControlCharacter),
            ("line\nbreak", EntryPathError::ControlCharacter),
            ("invoice\u{202e}fdp.exe", EntryPathError::BidiControl),
            ("a\u{2067}b", EntryPathError::BidiControl),
            ("report.pdf:stream", EntryPathError::Colon),
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

    #[test]
    fn legacy_names_are_decoded_then_validated() {
        // "Résumé/Ünïcode.txt" as an old ZIP tool would store it.
        let raw = b"R\x82sum\x82/\x9an\x8bcode.txt";
        let path = EntryPath::parse_cp437(raw).expect("decoded");
        assert_eq!(path.components(), ["Résumé", "Ünïcode.txt"]);
        assert_eq!(
            EntryPath::parse_cp437(&[0xe1, 0xff]).map(|p| p.components()[0].clone()),
            Ok("ß\u{a0}".to_owned())
        );
        assert_eq!(
            EntryPath::parse_cp437(b"../x"),
            Err(EntryPathError::Traversal)
        );
        assert_eq!(
            EntryPath::parse_cp437(b"a\\b"),
            Err(EntryPathError::Backslash)
        );
        assert_eq!(
            EntryPath::parse_cp437(b"a\x01b"),
            Err(EntryPathError::ControlCharacter)
        );
        // Decoding can grow a name past the limit even if the raw bytes fit.
        assert_eq!(
            EntryPath::parse_cp437(&[0xb0; MAX_PATH_BYTES]),
            Err(EntryPathError::TooLong)
        );
        // ASCII names decode to themselves.
        assert_eq!(EntryPath::parse_cp437(b"docs/a.pdf"), parse("docs/a.pdf"));
    }

    #[test]
    fn cp437_table_matches_known_points() {
        assert_eq!(decode_cp437(0x41), 'A');
        assert_eq!(decode_cp437(0x80), 'Ç');
        assert_eq!(decode_cp437(0x9e), '₧');
        assert_eq!(decode_cp437(0xe1), 'ß');
        assert_eq!(decode_cp437(0xff), '\u{a0}');
    }
}
