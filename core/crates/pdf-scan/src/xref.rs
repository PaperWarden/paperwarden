//! Cross-reference checks: does the file's own index point at its objects?
//!
//! The scan never relies on the index to find objects (see `scan.rs`); these
//! checks only decide whether the file is damaged, and find the encryption
//! dictionary through the trailers.

use crate::filters::{self, DecodeError, Stop};
use crate::scan::{find, rfind, Scanner};
use crate::syntax::{Dict, Object, Token};
use crate::{Damage, Limit, ScanError};
use std::collections::HashSet;

/// Where `%%EOF` must appear, counted back from the end of the file.
const EOF_WINDOW: usize = 1024;

impl Scanner<'_, '_> {
    pub(crate) fn check_cross_references(&mut self) -> Result<(), ScanError> {
        self.walk_chain()?;
        self.scan_trailers()
    }

    /// Any trailer may name the encryption dictionary, even one the chain
    /// cannot reach; the chain's newest trailer was recorded first.
    fn scan_trailers(&mut self) -> Result<(), ScanError> {
        let data = self.data;
        let mut at = 0usize;
        let mut trailers = 0u32;
        while let Some(found) = data.get(at..).and_then(|rest| find(rest, b"trailer")) {
            let mut parser = self.parser(at + found + b"trailer".len());
            at += found + 1;
            trailers += 1;
            if trailers > self.limits.max_sections.saturating_mul(4) {
                return Err(ScanError::Limit(Limit::Sections));
            }
            if let Ok(Token::Object(Object::Dict(dict))) = parser.next() {
                self.note_encryption(&dict);
            }
        }

        Ok(())
    }

    fn walk_chain(&mut self) -> Result<(), ScanError> {
        let data = self.data;
        let tail = data
            .get(data.len().saturating_sub(EOF_WINDOW)..)
            .unwrap_or_default();
        if find(tail, b"%%EOF").is_none() {
            self.damaged(Damage::MissingEndOfFile);
        }
        let Some(keyword) = rfind(data, b"startxref") else {
            self.damaged(Damage::CrossReference);
            return Ok(());
        };
        let mut parser = self.parser(keyword + b"startxref".len());
        let Ok(Token::Object(Object::Int(first))) = parser.next() else {
            self.damaged(Damage::CrossReference);
            return Ok(());
        };

        let mut visited = HashSet::new();
        let mut next = usize::try_from(first).ok();
        while let Some(offset) = next.take() {
            if !visited.insert(offset) {
                self.damaged(Damage::PrevCycle);
                break;
            }
            if visited.len() > self.limits.max_sections as usize {
                return Err(ScanError::Limit(Limit::Sections));
            }
            self.check()?;
            let Some(trailer) = self.section(offset)? else {
                self.damaged(Damage::CrossReference);
                break;
            };
            self.note_encryption(&trailer);
            if let Some(stream_offset) = trailer
                .int(b"XRefStm")
                .and_then(|v| usize::try_from(v).ok())
            {
                if self.xref_stream(stream_offset)?.is_none() {
                    self.damaged(Damage::CrossReference);
                }
            }
            if let Some(previous) = trailer.int(b"Prev") {
                match usize::try_from(previous) {
                    Ok(previous) if previous < data.len() => next = Some(previous),
                    _ => self.damaged(Damage::CrossReference),
                }
            }
        }
        Ok(())
    }

    /// Checks the cross-reference section at `offset` and returns its trailer
    /// dictionary, or `None` if no section is there.
    fn section(&mut self, offset: usize) -> Result<Option<Dict>, ScanError> {
        if offset >= self.data.len() {
            return Ok(None);
        }
        let mut parser = self.parser(offset);
        if parser.keyword(b"xref") {
            return self.xref_table(parser.pos());
        }
        self.xref_stream(offset)
    }

    /// Whether the object at `offset` has the header `N G obj` for `number`.
    fn points_at(&self, offset: u64, number: u64) -> bool {
        let Ok(offset) = usize::try_from(offset) else {
            return false;
        };
        let mut parser = self.parser(offset);
        parser.object_header().map(u64::from) == Some(number)
    }

    fn xref_table(&mut self, pos: usize) -> Result<Option<Dict>, ScanError> {
        let mut parser = self.parser(pos);
        let mut entries = 0u64;
        let mut wrong = false;
        loop {
            if parser.keyword(b"trailer") {
                break;
            }
            let (Ok(Token::Object(Object::Int(first))), Ok(Token::Object(Object::Int(count)))) =
                (parser.next(), parser.next())
            else {
                return Ok(None);
            };
            let (Ok(first), Ok(count)) = (u64::try_from(first), u64::try_from(count)) else {
                return Ok(None);
            };
            for index in 0..count {
                entries += 1;
                if entries > u64::from(self.limits.max_objects) {
                    return Err(ScanError::Limit(Limit::Objects));
                }
                let (
                    Ok(Token::Object(Object::Int(offset))),
                    Ok(Token::Object(Object::Int(_))),
                    Ok(Token::Keyword(kind)),
                ) = (parser.next(), parser.next(), parser.next())
                else {
                    return Ok(None);
                };
                if !wrong && kind == b"n" {
                    let number = first.saturating_add(index);
                    if !u64::try_from(offset).is_ok_and(|offset| self.points_at(offset, number)) {
                        wrong = true;
                    }
                }
            }
        }
        if wrong {
            self.damaged(Damage::CrossReference);
        }
        match parser.next() {
            Ok(Token::Object(Object::Dict(trailer))) => Ok(Some(trailer)),
            _ => Ok(None),
        }
    }

    /// Checks a cross-reference stream at `offset` and returns its dictionary.
    fn xref_stream(&mut self, offset: usize) -> Result<Option<Dict>, ScanError> {
        let mut parser = self.parser(offset);
        if parser.object_header().is_none() {
            return Ok(None);
        }
        let Ok(Token::Object(Object::Dict(dict))) = parser.next() else {
            return Ok(None);
        };
        if dict.name(b"Type") != Some(&b"XRef"[..]) || !parser.keyword(b"stream") {
            return Ok(None);
        }
        let (start, end) = self.stream_data(&dict, parser.pos());
        let encoded = self.data.get(start..end).unwrap_or_default();
        let chain = filters::chain(&dict);
        if chain.iter().any(|filter| filter.is_terminal()) {
            return Ok(None);
        }
        let bound = self.limits.max_stream_bytes;
        let raw = match self.decode_chain(&chain, encoded, bound, true, true, false) {
            Ok(decoded) => decoded.unwrap_or_else(|| encoded.to_vec()),
            Err(DecodeError::Corrupt) => return Ok(None),
            Err(DecodeError::Stop(Stop::Limit(limit))) => return Err(ScanError::Limit(limit)),
            Err(DecodeError::Stop(Stop::Interrupted)) => return Err(ScanError::Interrupted),
        };
        let parms = match dict.get(b"DecodeParms") {
            Some(Object::Dict(parms)) => Some(parms),
            Some(Object::Array(items)) => match items.first() {
                Some(Object::Dict(parms)) => Some(parms),
                _ => None,
            },
            _ => None,
        };
        let Some(rows) = filters::unpredict(&raw, parms) else {
            return Ok(None);
        };
        let widths: Vec<usize> = match dict.get(b"W") {
            Some(Object::Array(items)) if items.len() == 3 => items
                .iter()
                .filter_map(|item| match item {
                    Object::Int(width) => usize::try_from(*width).ok().filter(|w| *w <= 8),
                    _ => None,
                })
                .collect(),
            _ => return Ok(None),
        };
        let (&[kind_width, offset_width, gen_width], size) = (widths.as_slice(), dict.int(b"Size"))
        else {
            return Ok(None);
        };
        let entry = kind_width + offset_width + gen_width;
        if entry == 0 {
            return Ok(None);
        }
        let ranges: Vec<(u64, u64)> = match dict.get(b"Index") {
            Some(Object::Array(items)) => items
                .chunks(2)
                .filter_map(|pair| match pair {
                    [Object::Int(first), Object::Int(count)] => {
                        Some((u64::try_from(*first).ok()?, u64::try_from(*count).ok()?))
                    }
                    _ => None,
                })
                .collect(),
            _ => vec![(0, u64::try_from(size.unwrap_or(0)).unwrap_or(0))],
        };
        let field = |bytes: &[u8]| bytes.iter().fold(0u64, |acc, &b| (acc << 8) | u64::from(b));
        let mut records = rows.chunks_exact(entry);
        let mut wrong = false;
        let mut entries = 0u64;
        'ranges: for (first, count) in ranges {
            for index in 0..count {
                entries += 1;
                if entries > u64::from(self.limits.max_objects) {
                    return Err(ScanError::Limit(Limit::Objects));
                }
                let Some(record) = records.next() else {
                    break 'ranges;
                };
                let (kind, rest) = record.split_at(kind_width);
                let kind = if kind_width == 0 { 1 } else { field(kind) };
                let (offset, _) = rest.split_at(offset_width);
                if kind == 1 && !self.points_at(field(offset), first.saturating_add(index)) {
                    wrong = true;
                    break 'ranges;
                }
            }
        }
        if wrong {
            self.damaged(Damage::CrossReference);
        }
        Ok(Some(dict))
    }
}
