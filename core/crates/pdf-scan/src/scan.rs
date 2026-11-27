//! The scan itself: find every object, then measure every stream.

use crate::filters::{self, DecodeError, Filter, Stop};
use crate::syntax::{is_whitespace, Dict, Object, Parser, Seen, Token};
use crate::{ActiveContent, Damage, Interrupted, Limit, Report, ScanError, ScanLimits};
use std::collections::HashMap;

/// Container elements kept per object; the rest are parsed and dropped.
const BUDGET: usize = 65_536;
/// Slack allowed over an image's computed data size, in bytes.
const IMAGE_SLACK: u64 = 64 * 1024;
/// How many decoded bytes pass between checkpoints.
const CHECK_EVERY: u64 = 1 << 20;

/// An object header found by the index: where `N` starts and where the object
/// body starts after `obj`.
#[derive(Clone, Copy, Debug)]
struct Header {
    start: usize,
    body: usize,
    number: u32,
}

struct StreamRef {
    dict: Dict,
    start: usize,
    end: usize,
}

pub(crate) struct Scanner<'a, 'c> {
    pub(crate) data: &'a [u8],
    pub(crate) limits: ScanLimits,
    checkpoint: &'c mut dyn FnMut() -> Result<(), Interrupted>,
    /// The last header for each object number.
    pub(crate) index: HashMap<u32, usize>,
    pub(crate) damage: Vec<Damage>,
    seen: Seen,
    pub(crate) encrypted: bool,
    objects: u32,
    streams: u32,
    decoded: u64,
    since_check: u64,
    /// Components of colour spaces defined as indirect objects.
    colour_spaces: HashMap<u32, u64>,
}

impl<'a, 'c> Scanner<'a, 'c> {
    pub(crate) fn new(
        data: &'a [u8],
        limits: &ScanLimits,
        checkpoint: &'c mut dyn FnMut() -> Result<(), Interrupted>,
    ) -> Self {
        Self {
            data,
            limits: *limits,
            checkpoint,
            index: HashMap::new(),
            damage: Vec::new(),
            seen: Seen::default(),
            encrypted: false,
            objects: 0,
            streams: 0,
            decoded: 0,
            since_check: 0,
            colour_spaces: HashMap::new(),
        }
    }

    pub(crate) fn run(mut self) -> Result<Report, ScanError> {
        let head = self
            .data
            .get(..self.data.len().min(1024))
            .unwrap_or_default();
        if !head.windows(5).any(|window| window == b"%PDF-") {
            return Err(ScanError::NotPdf);
        }
        let headers = self.index_headers()?;
        self.check_cross_references()?;
        let streams = self.read_objects(&headers)?;
        if self.objects == 0 {
            return Err(ScanError::NoObjects);
        }
        if self.encrypted {
            return Err(ScanError::Encrypted);
        }
        for stream in &streams {
            self.measure(stream)?;
        }
        self.damage.sort();
        self.damage.dedup();
        Ok(Report {
            damage: self.damage,
            active_content: ActiveContent::ALL
                .into_iter()
                .filter(|kind| self.seen.contains(*kind))
                .collect(),
            objects: self.objects,
            streams: self.streams,
            decoded_bytes: self.decoded,
        })
    }

    pub(crate) fn check(&mut self) -> Result<(), ScanError> {
        (self.checkpoint)().map_err(|Interrupted| ScanError::Interrupted)
    }

    pub(crate) fn damaged(&mut self, damage: Damage) {
        self.damage.push(damage);
    }

    pub(crate) fn parser(&self, pos: usize) -> Parser<'a> {
        Parser::new(self.data, pos, self.limits.max_nesting, BUDGET)
    }

    /// Finds every `N G obj` header, the way PDFium rebuilds a damaged file.
    fn index_headers(&mut self) -> Result<Vec<Header>, ScanError> {
        let data = self.data;
        let mut headers = Vec::new();
        let mut at = 0usize;
        while let Some(found) = data.get(at..).and_then(|rest| find(rest, b"obj")) {
            let keyword = at + found;
            at = keyword + 3;
            if data.get(at).copied().is_some_and(is_regular) {
                continue;
            }
            if let Some((start, number)) = header_before(data, keyword) {
                headers.push(Header {
                    start,
                    body: at,
                    number,
                });
                if headers.len() > self.limits.max_objects as usize {
                    return Err(ScanError::Limit(Limit::Objects));
                }
                self.index.insert(number, start);
            }
        }
        Ok(headers)
    }

    /// Resolves an integer that may be given by reference.
    pub(crate) fn resolve_int(&self, object: Option<&Object>) -> Option<i64> {
        match object? {
            Object::Int(value) => Some(*value),
            Object::Ref(number) => {
                let start = *self.index.get(number)?;
                let mut parser = self.parser(start);
                parser.object_header()?;
                match parser.next() {
                    Ok(Token::Object(Object::Int(value))) => Some(value),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Finds a stream's data after the `stream` keyword at `pos`. Records
    /// damage when `/Length` is wrong or the data never ends.
    pub(crate) fn stream_data(&mut self, dict: &Dict, pos: usize) -> (usize, usize) {
        let data = self.data;
        let mut start = pos;
        if data.get(start) == Some(&b'\r') {
            start += 1;
        }
        if data.get(start) == Some(&b'\n') {
            start += 1;
        }
        let declared = self.resolve_int(dict.get(b"Length"));
        if let Some(length) = declared.and_then(|length| usize::try_from(length).ok()) {
            if let Some(end) = start.checked_add(length).filter(|end| *end <= data.len()) {
                let mut parser = self.parser(end);
                if parser.keyword(b"endstream") {
                    return (start, end);
                }
            }
        }
        // `/Length` is missing, wrong or indirect to nothing: find the end.
        match data.get(start..).and_then(|rest| find(rest, b"endstream")) {
            Some(found) => {
                let mut end = start + found;
                if end > start && data.get(end - 1) == Some(&b'\n') {
                    end -= 1;
                }
                if end > start && data.get(end - 1) == Some(&b'\r') {
                    end -= 1;
                }
                if declared.is_some() || dict.has(b"Length") {
                    self.damaged(Damage::StreamLength);
                }
                (start, end)
            }
            None => {
                self.damaged(Damage::Truncated);
                (start, data.len())
            }
        }
    }

    /// Phase A: parse every object once, in file order, collecting streams.
    fn read_objects(&mut self, headers: &[Header]) -> Result<Vec<StreamRef>, ScanError> {
        let mut streams = Vec::new();
        let mut consumed = 0usize;
        let mut ordered = headers.to_vec();
        ordered.sort_by_key(|header| header.start);
        for header in ordered {
            if header.start < consumed {
                // Inside the previous object or stream data: not a real header.
                continue;
            }
            if self.objects.is_multiple_of(256) {
                self.check()?;
            }
            let mut parser = self.parser(header.body);
            let token = parser.next().map_err(ScanError::Limit)?;
            self.seen.merge(parser.seen);
            self.count_object()?;
            consumed = parser.pos();
            match token {
                Token::Object(Object::Dict(dict)) => {
                    self.note_encryption(&dict);
                    if parser.keyword(b"stream") {
                        let (start, end) = self.stream_data(&dict, parser.pos());
                        consumed = end;
                        // An ICC profile's component count, for images that
                        // refer to it.
                        if let Some(n) = dict.int(b"N").and_then(|n| u64::try_from(n).ok()) {
                            self.colour_spaces.insert(header.number, n.max(1));
                        }
                        streams.push(StreamRef { dict, start, end });
                    }
                }
                Token::Object(Object::Array(items)) => {
                    if let Some(components) = self.components_of_array(&items) {
                        self.colour_spaces.insert(header.number, components);
                    }
                }
                Token::Eof => self.damaged(Damage::Truncated),
                _ => {}
            }
        }
        Ok(streams)
    }

    fn count_object(&mut self) -> Result<(), ScanError> {
        self.objects = self.objects.saturating_add(1);
        if self.objects > self.limits.max_objects {
            return Err(ScanError::Limit(Limit::Objects));
        }
        Ok(())
    }

    pub(crate) fn note_encryption(&mut self, dict: &Dict) {
        let standard_handler =
            dict.name(b"Filter") == Some(&b"Standard"[..]) && dict.has(b"O") && dict.has(b"U");
        if dict.has(b"Encrypt") || standard_handler {
            self.encrypted = true;
        }
    }

    fn components_of_name(name: &[u8]) -> Option<u64> {
        match name {
            b"DeviceGray" | b"G" | b"CalGray" | b"Indexed" | b"I" | b"Separation" | b"Pattern" => {
                Some(1)
            }
            b"DeviceRGB" | b"RGB" | b"CalRGB" | b"Lab" => Some(3),
            b"DeviceCMYK" | b"CMYK" => Some(4),
            _ => None,
        }
    }

    fn components_of_array(&self, items: &[Object]) -> Option<u64> {
        match items.first()? {
            Object::Name(family) => match family.as_slice() {
                b"ICCBased" => match items.get(1) {
                    Some(Object::Ref(number)) => self.colour_spaces.get(number).copied(),
                    _ => None,
                },
                b"DeviceN" => match items.get(1) {
                    Some(Object::Array(names)) => u64::try_from(names.len()).ok(),
                    _ => None,
                },
                other => Self::components_of_name(other),
            },
            _ => None,
        }
    }

    /// Colour components of an image, or 4 when unknown (most permissive
    /// common value).
    fn components(&self, dict: &Dict) -> u64 {
        let resolved = match dict.get(b"ColorSpace").or_else(|| dict.get(b"CS")) {
            Some(Object::Name(name)) => Self::components_of_name(name),
            Some(Object::Array(items)) => self.components_of_array(items),
            Some(Object::Ref(number)) => self.colour_spaces.get(number).copied(),
            _ => None,
        };
        resolved.unwrap_or(4).clamp(1, 32)
    }

    /// Phase B: decode one stream under the limits.
    fn measure(&mut self, stream: &StreamRef) -> Result<(), ScanError> {
        self.streams = self.streams.saturating_add(1);
        self.check()?;
        let dict = &stream.dict;
        let encoded = self.data.get(stream.start..stream.end).unwrap_or_default();
        let is_image = dict.name(b"Subtype") == Some(&b"Image"[..]);
        let is_object_stream = dict.name(b"Type") == Some(&b"ObjStm"[..]);
        let chain = filters::chain(dict);
        let split = chain
            .iter()
            .position(|filter| filter.is_terminal())
            .unwrap_or(chain.len());
        let (bytes_filters, terminal) = chain.split_at(split);
        let terminal = terminal.first().copied();

        let mut bound = self.limits.max_stream_bytes;
        let mut ratio_applies = !is_image;
        if is_image {
            let width = dict.int(b"Width").or_else(|| dict.int(b"W"));
            let height = dict.int(b"Height").or_else(|| dict.int(b"H"));
            if let (Some(width), Some(height)) = (width, height) {
                let width = u64::try_from(width).unwrap_or(0);
                let height = u64::try_from(height).unwrap_or(0);
                self.check_pixels(width, height)?;
                if terminal.is_none() {
                    let mask = matches!(
                        dict.get(b"ImageMask").or_else(|| dict.get(b"IM")),
                        Some(Object::Bool(true))
                    );
                    let bits = if mask {
                        1
                    } else {
                        dict.int(b"BitsPerComponent")
                            .or_else(|| dict.int(b"BPC"))
                            .and_then(|bits| u64::try_from(bits).ok())
                            .unwrap_or(8)
                            .clamp(1, 16)
                    };
                    let components = if mask { 1 } else { self.components(dict) };
                    // One extra byte per row allows for PNG predictors.
                    let row = width
                        .saturating_mul(components)
                        .saturating_mul(bits)
                        .saturating_add(7)
                        / 8
                        + 1;
                    bound = row.saturating_mul(height).saturating_add(IMAGE_SLACK);
                }
            } else {
                // Without dimensions the image gets the generic bounds.
                ratio_applies = true;
            }
        }

        let needs_bytes = is_object_stream || matches!(terminal, Some(Filter::Dct | Filter::Jpx));
        let decoded = match self.decode_chain(
            bytes_filters,
            encoded,
            bound,
            ratio_applies,
            needs_bytes,
            is_image,
        ) {
            Ok(decoded) => decoded,
            Err(DecodeError::Corrupt) => {
                self.damaged(Damage::CorruptStream);
                return Ok(());
            }
            Err(DecodeError::Stop(Stop::Limit(limit))) => return Err(ScanError::Limit(limit)),
            Err(DecodeError::Stop(Stop::Interrupted)) => return Err(ScanError::Interrupted),
        };
        let bytes: &[u8] = decoded.as_deref().unwrap_or(encoded);
        match terminal {
            Some(Filter::Dct) => {
                if let Some((width, height, _)) = filters::jpeg_size(bytes) {
                    self.check_pixels(width, height)?;
                }
            }
            Some(Filter::Jpx) => {
                if let Some((width, height, _)) = filters::jpx_size(bytes) {
                    self.check_pixels(width, height)?;
                }
            }
            _ => {}
        }
        if is_object_stream && terminal.is_none() {
            self.read_object_stream(dict, bytes)?;
        }
        Ok(())
    }

    fn check_pixels(&self, width: u64, height: u64) -> Result<(), ScanError> {
        if width.saturating_mul(height) > self.limits.max_image_pixels {
            Err(ScanError::Limit(Limit::ImageSize))
        } else {
            Ok(())
        }
    }

    /// Decodes `input` through `chain`. Returns the final bytes when
    /// `keep_last` is set or a later stage needs them, and `None` when the
    /// chain is empty.
    pub(crate) fn decode_chain(
        &mut self,
        chain: &[Filter],
        input: &[u8],
        bound: u64,
        ratio_applies: bool,
        keep_last: bool,
        is_image: bool,
    ) -> Result<Option<Vec<u8>>, DecodeError> {
        let mut current: Option<Vec<u8>> = None;
        for (position, filter) in chain.iter().enumerate() {
            let keep = keep_last || position + 1 < chain.len();
            let stage_input: &[u8] = current.as_deref().unwrap_or(input);
            let encoded_len = stage_input.len() as u64;
            let mut written: u64 = 0;
            let mut kept: Vec<u8> = Vec::new();
            let limits = self.limits;
            let mut decoded = self.decoded;
            let mut since_check = self.since_check;
            let checkpoint = &mut *self.checkpoint;
            let size_limit = if is_image {
                Limit::ImageSize
            } else {
                Limit::StreamSize
            };
            let result = filters::decode(*filter, stage_input, &mut |chunk: &[u8]| {
                let length = chunk.len() as u64;
                written = written.saturating_add(length);
                decoded = decoded.saturating_add(length);
                since_check = since_check.saturating_add(length);
                if written > bound {
                    return Err(Stop::Limit(size_limit));
                }
                if ratio_applies
                    && written > limits.ratio_floor
                    && written > encoded_len.saturating_mul(limits.max_ratio)
                {
                    return Err(Stop::Limit(Limit::CompressionRatio));
                }
                if decoded > limits.max_total_bytes {
                    return Err(Stop::Limit(Limit::TotalDecoded));
                }
                if since_check >= CHECK_EVERY {
                    since_check = 0;
                    checkpoint().map_err(|Interrupted| Stop::Interrupted)?;
                }
                if keep {
                    kept.extend_from_slice(chunk);
                }
                Ok(())
            });
            self.decoded = decoded;
            self.since_check = since_check;
            result?;
            current = keep.then_some(kept);
        }
        Ok(current)
    }

    /// Parses the objects inside an object stream, for nesting and active
    /// content.
    fn read_object_stream(&mut self, dict: &Dict, bytes: &[u8]) -> Result<(), ScanError> {
        let count = dict
            .int(b"N")
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0);
        let first = dict
            .int(b"First")
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0);
        if first > bytes.len() {
            self.damaged(Damage::CorruptStream);
            return Ok(());
        }
        let mut parser = Parser::new(bytes, first, self.limits.max_nesting, BUDGET);
        for _ in 0..count {
            match parser.next().map_err(ScanError::Limit)? {
                Token::Eof => break,
                Token::Object(Object::Dict(dict)) => self.note_encryption(&dict),
                _ => {}
            }
            self.count_object()?;
        }
        self.seen.merge(parser.seen);
        Ok(())
    }
}

fn is_regular(byte: u8) -> bool {
    !is_whitespace(byte) && !crate::syntax::is_delimiter(byte)
}

/// The first position of `needle` in `haystack`.
pub(crate) fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let (&first, rest) = needle.split_first()?;
    let mut at = 0usize;
    while let Some(offset) = haystack.get(at..)?.iter().position(|&byte| byte == first) {
        let candidate = at + offset;
        let end = candidate + needle.len();
        if haystack.get(candidate + 1..end) == Some(rest) {
            return Some(candidate);
        }
        at = candidate + 1;
    }
    None
}

/// The last position of `needle` in `haystack`.
pub(crate) fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

/// Parses `N G` backwards from the `obj` keyword at `keyword`.
fn header_before(data: &[u8], keyword: usize) -> Option<(usize, u32)> {
    let mut at = keyword;
    let back_over = |at: &mut usize, test: fn(u8) -> bool| -> usize {
        let end = *at;
        while *at > 0 && data.get(*at - 1).copied().is_some_and(test) {
            *at -= 1;
        }
        end - *at
    };
    back_over(&mut at, is_whitespace);
    if back_over(&mut at, |byte| byte.is_ascii_digit()) == 0 {
        return None;
    }
    if back_over(&mut at, is_whitespace) == 0 {
        return None;
    }
    let number_end = at;
    if back_over(&mut at, |byte| byte.is_ascii_digit()) == 0 {
        return None;
    }
    if at > 0 && data.get(at - 1).copied().is_some_and(is_regular) {
        return None;
    }
    let digits = data.get(at..number_end)?;
    let number = std::str::from_utf8(digits).ok()?.parse::<u32>().ok()?;
    Some((at, number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_are_found_backwards_from_obj() {
        let data = b"%PDF-1.7\n12 0 obj\n<<>>\nendobj\nx13 0 obj\n";
        let keyword = find(data, b"obj").expect("obj");
        assert_eq!(header_before(data, keyword), Some((9, 12)));
        let second = rfind(data, b"obj").expect("obj");
        assert_eq!(header_before(data, second), None, "x13 is not a number");
    }

    #[test]
    fn find_and_rfind_agree_with_std() {
        let data = b"aaendstreambbendstream";
        assert_eq!(find(data, b"endstream"), Some(2));
        assert_eq!(rfind(data, b"endstream"), Some(13));
        assert_eq!(find(data, b"zz"), None);
    }
}
