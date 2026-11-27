//! A bounded PDF object parser for hostile input.
//!
//! It parses one object at a time with an explicit nesting limit, so recursion
//! depth is bounded no matter what the file contains. It keeps at most
//! `budget` container elements per object and drops the rest after parsing
//! them, so a huge array cannot exhaust memory. It decodes `#xx` escapes in
//! names, so `/Java#53cript` cannot hide from the active-content checks.
//! String contents are skipped, never kept.

use crate::{ActiveContent, Limit};

/// Longest name kept, in bytes. PDF's own limit is 127; longer names are
/// truncated, which only matters for names no check looks at.
const MAX_NAME: usize = 256;

/// Longest string kept, in bytes: enough for encryption keys and IDs.
pub(crate) const MAX_STRING: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Object {
    Null,
    Bool(bool),
    Int(i64),
    Real,
    Name(Vec<u8>),
    /// A string's bytes when at most [`MAX_STRING`] long, otherwise `None`:
    /// long strings are skipped, never kept.
    Str(Option<Vec<u8>>),
    Array(Vec<Object>),
    Dict(Dict),
    Ref(u32),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Dict(pub(crate) Vec<(Vec<u8>, Object)>);

impl Dict {
    pub(crate) fn get(&self, key: &[u8]) -> Option<&Object> {
        self.0
            .iter()
            .rev()
            .find(|(name, _)| name.as_slice() == key)
            .map(|(_, value)| value)
    }

    pub(crate) fn int(&self, key: &[u8]) -> Option<i64> {
        match self.get(key) {
            Some(Object::Int(value)) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn name(&self, key: &[u8]) -> Option<&[u8]> {
        match self.get(key) {
            Some(Object::Name(name)) => Some(name),
            _ => None,
        }
    }

    pub(crate) fn has(&self, key: &[u8]) -> bool {
        self.get(key).is_some()
    }
}

/// What one parse step produced.
#[derive(Debug, PartialEq)]
pub(crate) enum Token {
    Object(Object),
    /// A bare keyword such as `obj`, `stream` or `endobj`.
    Keyword(Vec<u8>),
    /// `]`, `>>` or a stray `)`, `>`, `{` or `}`.
    Close,
    Eof,
}

pub(crate) fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

pub(crate) fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_regular(byte: u8) -> bool {
    !is_whitespace(byte) && !is_delimiter(byte)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Active content an object announced while it was parsed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Seen(u16);

impl Seen {
    pub(crate) fn add(&mut self, kind: ActiveContent) {
        self.0 |= 1 << (kind as u16);
    }

    pub(crate) fn merge(&mut self, other: Seen) {
        self.0 |= other.0;
    }

    pub(crate) fn contains(self, kind: ActiveContent) -> bool {
        self.0 & (1 << (kind as u16)) != 0
    }
}

fn classify(key: &[u8], value: &Object, seen: &mut Seen) {
    match (key, value) {
        (b"S", Object::Name(action)) => {
            let kind = match action.as_slice() {
                b"JavaScript" => ActiveContent::JavaScript,
                b"Launch" => ActiveContent::Launch,
                b"URI" => ActiveContent::Uri,
                b"SubmitForm" => ActiveContent::SubmitForm,
                b"ImportData" => ActiveContent::ImportData,
                b"GoToR" | b"GoToE" => ActiveContent::RemoteGoTo,
                b"Rendition" | b"Movie" | b"Sound" | b"RichMediaExecute" => {
                    ActiveContent::RichMedia
                }
                _ => return,
            };
            seen.add(kind);
        }
        (b"JS", _) => seen.add(ActiveContent::JavaScript),
        (b"URI", Object::Str(_)) => seen.add(ActiveContent::Uri),
        (b"EmbeddedFiles", _) => seen.add(ActiveContent::EmbeddedFile),
        (b"Type", Object::Name(kind)) if kind.as_slice() == b"EmbeddedFile" => {
            seen.add(ActiveContent::EmbeddedFile);
        }
        (b"Subtype", Object::Name(kind))
            if matches!(
                kind.as_slice(),
                b"RichMedia" | b"Movie" | b"Sound" | b"Screen" | b"3D"
            ) =>
        {
            seen.add(ActiveContent::RichMedia);
        }
        (b"XFA", _) => seen.add(ActiveContent::Xfa),
        _ => {}
    }
}

pub(crate) struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    max_depth: u32,
    budget: usize,
    pub(crate) seen: Seen,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(data: &'a [u8], pos: usize, max_depth: u32, budget: usize) -> Self {
        Self {
            data,
            pos,
            max_depth,
            budget,
            seen: Seen::default(),
        }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.pos
            .checked_add(offset)
            .and_then(|index| self.data.get(index).copied())
    }

    pub(crate) fn skip_whitespace(&mut self) {
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) {
                self.pos += 1;
            } else if byte == b'%' {
                while let Some(byte) = self.peek() {
                    if byte == b'\r' || byte == b'\n' {
                        break;
                    }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Parses the next token, following nested containers up to the depth
    /// limit.
    pub(crate) fn next(&mut self) -> Result<Token, Limit> {
        self.parse(0)
    }

    fn parse(&mut self, depth: u32) -> Result<Token, Limit> {
        self.skip_whitespace();
        let Some(byte) = self.peek() else {
            return Ok(Token::Eof);
        };
        match byte {
            b'/' => {
                self.pos += 1;
                Ok(Token::Object(Object::Name(self.name())))
            }
            b'(' => Ok(Token::Object(Object::Str(self.literal_string()))),
            b'<' if self.peek_at(1) == Some(b'<') => {
                self.pos += 2;
                self.dict(depth)
                    .map(|dict| Token::Object(Object::Dict(dict)))
            }
            b'<' => {
                self.pos += 1;
                Ok(Token::Object(Object::Str(self.hex_string())))
            }
            b'[' => {
                self.pos += 1;
                self.array(depth)
                    .map(|items| Token::Object(Object::Array(items)))
            }
            b']' | b')' | b'{' | b'}' => {
                self.pos += 1;
                Ok(Token::Close)
            }
            b'>' => {
                self.pos += if self.peek_at(1) == Some(b'>') { 2 } else { 1 };
                Ok(Token::Close)
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => Ok(Token::Object(self.number_or_ref())),
            _ => {
                let start = self.pos;
                while self.peek().is_some_and(is_regular) {
                    self.pos += 1;
                }
                let word = self.data.get(start..self.pos).unwrap_or_default();
                Ok(match word {
                    b"true" => Token::Object(Object::Bool(true)),
                    b"false" => Token::Object(Object::Bool(false)),
                    b"null" => Token::Object(Object::Null),
                    _ => Token::Keyword(word.to_vec()),
                })
            }
        }
    }

    fn enter(&self, depth: u32) -> Result<u32, Limit> {
        let next = depth.saturating_add(1);
        if next > self.max_depth {
            Err(Limit::Nesting)
        } else {
            Ok(next)
        }
    }

    fn keep(&mut self) -> bool {
        if self.budget == 0 {
            false
        } else {
            self.budget -= 1;
            true
        }
    }

    fn array(&mut self, depth: u32) -> Result<Vec<Object>, Limit> {
        let inner = self.enter(depth)?;
        let mut items = Vec::new();
        loop {
            match self.parse(inner)? {
                Token::Object(object) => {
                    if self.keep() {
                        items.push(object);
                    }
                }
                Token::Keyword(_) => {}
                Token::Close | Token::Eof => return Ok(items),
            }
        }
    }

    fn dict(&mut self, depth: u32) -> Result<Dict, Limit> {
        let inner = self.enter(depth)?;
        let mut entries = Vec::new();
        loop {
            match self.parse(inner)? {
                Token::Object(Object::Name(key)) => {
                    let value = match self.parse(inner)? {
                        Token::Object(value) => value,
                        Token::Keyword(_) => Object::Null,
                        Token::Close | Token::Eof => return Ok(Dict(entries)),
                    };
                    classify(&key, &value, &mut self.seen);
                    if self.keep() {
                        entries.push((key, value));
                    }
                }
                // A key that is not a name: skip it, as PDFium does.
                Token::Object(_) | Token::Keyword(_) => {}
                Token::Close | Token::Eof => return Ok(Dict(entries)),
            }
        }
    }

    fn name(&mut self) -> Vec<u8> {
        let mut name = Vec::new();
        while let Some(byte) = self.peek() {
            if !is_regular(byte) {
                break;
            }
            self.pos += 1;
            let decoded = if byte == b'#' {
                match (
                    self.peek().and_then(hex_value),
                    self.peek_at(1).and_then(hex_value),
                ) {
                    (Some(high), Some(low)) => {
                        self.pos += 2;
                        high * 16 + low
                    }
                    _ => byte,
                }
            } else {
                byte
            };
            if name.len() < MAX_NAME {
                name.push(decoded);
            }
        }
        name
    }

    /// Reads a literal string, decoding escapes, and returns its bytes if it
    /// is at most [`MAX_STRING`] long.
    fn literal_string(&mut self) -> Option<Vec<u8>> {
        self.pos += 1;
        let mut open = 1usize;
        let mut out = Vec::new();
        let keep = |out: &mut Vec<u8>, byte: u8| {
            if out.len() <= MAX_STRING {
                out.push(byte);
            }
        };
        while let Some(byte) = self.peek() {
            self.pos += 1;
            match byte {
                b'\\' => {
                    let Some(escaped) = self.peek() else { break };
                    self.pos += 1;
                    match escaped {
                        b'n' => keep(&mut out, b'\n'),
                        b'r' => keep(&mut out, b'\r'),
                        b't' => keep(&mut out, b'\t'),
                        b'b' => keep(&mut out, 0x08),
                        b'f' => keep(&mut out, 0x0C),
                        b'0'..=b'7' => {
                            let mut value = u32::from(escaped - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(digit @ b'0'..=b'7') => {
                                        self.pos += 1;
                                        value = value * 8 + u32::from(digit - b'0');
                                    }
                                    _ => break,
                                }
                            }
                            keep(&mut out, (value & 0xFF) as u8);
                        }
                        // A backslash before an end of line joins the lines.
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        other => keep(&mut out, other),
                    }
                }
                b'(' => {
                    open += 1;
                    keep(&mut out, byte);
                }
                b')' => {
                    open -= 1;
                    if open == 0 {
                        break;
                    }
                    keep(&mut out, byte);
                }
                _ => keep(&mut out, byte),
            }
        }
        self.pos = self.pos.min(self.data.len());
        (out.len() <= MAX_STRING).then_some(out)
    }

    /// Reads a hexadecimal string after its `<`, returning its bytes if it is
    /// at most [`MAX_STRING`] long. An odd final digit counts as if followed
    /// by 0.
    fn hex_string(&mut self) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        let mut high: Option<u8> = None;
        while let Some(byte) = self.peek() {
            self.pos += 1;
            if byte == b'>' {
                break;
            }
            let Some(digit) = hex_value(byte) else {
                continue;
            };
            match high.take() {
                Some(value) => {
                    if out.len() <= MAX_STRING {
                        out.push(value * 16 + digit);
                    }
                }
                None => high = Some(digit),
            }
        }
        if let Some(value) = high {
            if out.len() <= MAX_STRING {
                out.push(value * 16);
            }
        }
        (out.len() <= MAX_STRING).then_some(out)
    }

    /// Reads an unsigned integer at the current position without signs.
    fn digits(&mut self) -> Option<u64> {
        let start = self.pos;
        let mut value: u64 = 0;
        while let Some(byte) = self.peek().filter(u8::is_ascii_digit) {
            value = value
                .saturating_mul(10)
                .saturating_add(u64::from(byte - b'0'));
            self.pos += 1;
        }
        (self.pos > start).then_some(value)
    }

    fn number_or_ref(&mut self) -> Object {
        let negative = self.peek() == Some(b'-');
        let signed = matches!(self.peek(), Some(b'+' | b'-'));
        if signed {
            self.pos += 1;
        }
        let whole = self.digits();
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.digits();
            self.skip_regular();
            return Object::Real;
        }
        if self.peek().is_some_and(is_regular) {
            // Something like `12abc`: not a number.
            self.skip_regular();
            return Object::Null;
        }
        let Some(whole) = whole else {
            return Object::Null;
        };
        let Ok(value) = i64::try_from(whole) else {
            return Object::Real;
        };
        let value = if negative { -value } else { value };
        if signed {
            // A signed number is never an object number.
            return Object::Int(value);
        }
        // `N G R` is a reference; anything else leaves the position after N.
        let after_first = self.pos;
        if let Ok(number) = u32::try_from(value) {
            self.skip_whitespace();
            if self.digits().is_some() && self.peek().is_some_and(is_whitespace) {
                self.skip_whitespace();
                if self.peek() == Some(b'R') && !self.peek_at(1).is_some_and(is_regular) {
                    self.pos += 1;
                    return Object::Ref(number);
                }
            }
        }
        self.pos = after_first;
        Object::Int(value)
    }

    fn skip_regular(&mut self) {
        while self.peek().is_some_and(is_regular) {
            self.pos += 1;
        }
    }

    /// Whether the next keyword is `word`, consuming it if so.
    pub(crate) fn keyword(&mut self, word: &[u8]) -> bool {
        self.skip_whitespace();
        let end = self.pos.saturating_add(word.len());
        if self.data.get(self.pos..end) == Some(word)
            && !self.data.get(end).copied().is_some_and(is_regular)
        {
            self.pos = end;
            true
        } else {
            false
        }
    }

    /// Reads `N G obj` at the current position, returning N.
    pub(crate) fn object_header(&mut self) -> Option<u32> {
        let start = self.pos;
        self.skip_whitespace();
        let header = (|| {
            let number = u32::try_from(self.digits()?).ok()?;
            if !self.peek().is_some_and(is_whitespace) {
                return None;
            }
            self.skip_whitespace();
            self.digits()?;
            self.keyword(b"obj").then_some(number)
        })();
        if header.is_none() {
            self.pos = start;
        }
        header
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &[u8]) -> Result<Token, Limit> {
        Parser::new(text, 0, 32, 1000).next()
    }

    #[test]
    fn parses_dicts_arrays_refs_and_numbers() {
        let Ok(Token::Object(Object::Dict(dict))) =
            parse(b"<< /Type /Page /Kids [1 0 R 2 0 R] /Count 2 /Rotate -90 /W 1.5 >>")
        else {
            panic!("not a dict");
        };
        assert_eq!(dict.name(b"Type"), Some(&b"Page"[..]));
        assert_eq!(
            dict.get(b"Kids"),
            Some(&Object::Array(vec![Object::Ref(1), Object::Ref(2)]))
        );
        assert_eq!(dict.int(b"Count"), Some(2));
        assert_eq!(dict.int(b"Rotate"), Some(-90));
        assert_eq!(dict.get(b"W"), Some(&Object::Real));
    }

    #[test]
    fn integers_followed_by_other_tokens_are_not_refs() {
        let mut parser = Parser::new(b"12 0 obj", 0, 32, 10);
        assert_eq!(parser.next(), Ok(Token::Object(Object::Int(12))));
        assert_eq!(parser.next(), Ok(Token::Object(Object::Int(0))));
        assert_eq!(parser.next(), Ok(Token::Keyword(b"obj".to_vec())));
        let mut parser = Parser::new(b"7 0 obj", 0, 32, 10);
        assert_eq!(parser.object_header(), Some(7));
    }

    #[test]
    fn names_decode_hex_escapes() {
        let mut parser = Parser::new(b"<< /S /Java#53cript /JS (x) >>", 0, 32, 10);
        assert!(matches!(parser.next(), Ok(Token::Object(Object::Dict(_)))));
        assert!(parser.seen.contains(ActiveContent::JavaScript));
    }

    #[test]
    fn nesting_beyond_the_limit_is_a_limit_error() {
        let deep = [b"[".repeat(33), b"]".repeat(33)].concat();
        assert_eq!(parse(&deep), Err(Limit::Nesting));
        let fine = [b"[".repeat(32), b"]".repeat(32)].concat();
        assert!(matches!(parse(&fine), Ok(Token::Object(Object::Array(_)))));
    }

    #[test]
    fn strings_with_nesting_and_escapes_are_skipped_whole() {
        let mut parser = Parser::new(b"(a (b) \\) c) /Next", 0, 32, 10);
        assert_eq!(
            parser.next(),
            Ok(Token::Object(Object::Str(Some(b"a (b) ) c".to_vec()))))
        );
        assert_eq!(
            parser.next(),
            Ok(Token::Object(Object::Name(b"Next".to_vec())))
        );
    }

    #[test]
    fn strings_decode_escapes_octal_hex_and_skip_long_ones() {
        let mut parser = Parser::new(b"(\\101\\n\\\\x\\\ny) <48 65 6C6c 6F> <ABC>", 0, 32, 10);
        assert_eq!(
            parser.next(),
            Ok(Token::Object(Object::Str(Some(b"A\n\\xy".to_vec()))))
        );
        assert_eq!(
            parser.next(),
            Ok(Token::Object(Object::Str(Some(b"Hello".to_vec()))))
        );
        assert_eq!(
            parser.next(),
            Ok(Token::Object(Object::Str(Some(vec![0xAB, 0xC0]))))
        );
        let long = [
            b"(".to_vec(),
            vec![b'x'; MAX_STRING + 1],
            b") /Next".to_vec(),
        ]
        .concat();
        let mut parser = Parser::new(&long, 0, 32, 10);
        assert_eq!(parser.next(), Ok(Token::Object(Object::Str(None))));
        assert_eq!(
            parser.next(),
            Ok(Token::Object(Object::Name(b"Next".to_vec())))
        );
    }

    #[test]
    fn the_budget_drops_elements_but_keeps_parsing() {
        let mut parser = Parser::new(b"[1 2 3 4 5] /After", 0, 32, 2);
        assert_eq!(
            parser.next(),
            Ok(Token::Object(Object::Array(vec![
                Object::Int(1),
                Object::Int(2)
            ])))
        );
        assert_eq!(
            parser.next(),
            Ok(Token::Object(Object::Name(b"After".to_vec())))
        );
    }

    #[test]
    fn unterminated_input_ends_without_panicking() {
        for text in [
            &b"<< /A [1 2"[..],
            b"(abc",
            b"<abc",
            b"/",
            b"-",
            b"1 0",
            b"<<",
        ] {
            let mut parser = Parser::new(text, 0, 32, 10);
            for _ in 0..10 {
                if parser.next() == Ok(Token::Eof) {
                    break;
                }
            }
        }
    }
}
