//! Stream filters, decoded into a callback that sees output in chunks.
//!
//! Nothing here keeps decoded data: the callback counts it, or collects it
//! under a cap, and can stop the decoder at any chunk. Image codecs (DCT,
//! JPEG 2000, JBIG2, CCITT) are not decoded; their real dimensions are read
//! from their headers where that is cheap.

use crate::syntax::{is_whitespace, Dict, Object};
use crate::Limit;
use miniz_oxide::inflate::stream::{inflate, InflateState};
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};

/// Chunk size for decoder output.
const CHUNK: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Filter {
    Flate,
    Lzw {
        early_change: bool,
    },
    AsciiHex,
    Ascii85,
    RunLength,
    /// The `Crypt` filter with the identity handler, or no filter at all.
    Identity,
    Dct,
    Jpx,
    Jbig2,
    Ccitt,
    /// A filter PDFium does not know either; its data is not decoded.
    Unknown,
}

impl Filter {
    pub(crate) fn from_name(name: &[u8], parms: Option<&Dict>) -> Self {
        match name {
            b"FlateDecode" | b"Fl" => Self::Flate,
            b"LZWDecode" | b"LZW" => Self::Lzw {
                early_change: parms.and_then(|p| p.int(b"EarlyChange")) != Some(0),
            },
            b"ASCIIHexDecode" | b"AHx" => Self::AsciiHex,
            b"ASCII85Decode" | b"A85" => Self::Ascii85,
            b"RunLengthDecode" | b"RL" => Self::RunLength,
            b"Crypt" => Self::Identity,
            b"DCTDecode" | b"DCT" => Self::Dct,
            b"JPXDecode" => Self::Jpx,
            b"JBIG2Decode" => Self::Jbig2,
            b"CCITTFaxDecode" | b"CCF" => Self::Ccitt,
            _ => Self::Unknown,
        }
    }

    /// Image codecs and unknown filters end a chain: their output is not
    /// bytes PaperWarden decodes.
    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Dct | Self::Jpx | Self::Jbig2 | Self::Ccitt | Self::Unknown
        )
    }
}

/// A stream's filters and their parameters, in order. Chains longer than
/// eight filters are treated as unknown.
pub(crate) fn chain(dict: &Dict) -> Vec<Filter> {
    let parms_at = |index: usize| -> Option<&Dict> {
        match dict.get(b"DecodeParms").or_else(|| dict.get(b"DP")) {
            Some(Object::Dict(parms)) if index == 0 => Some(parms),
            Some(Object::Array(items)) => match items.get(index) {
                Some(Object::Dict(parms)) => Some(parms),
                _ => None,
            },
            _ => None,
        }
    };
    match dict.get(b"Filter").or_else(|| dict.get(b"F")) {
        Some(Object::Name(name)) => vec![Filter::from_name(name, parms_at(0))],
        Some(Object::Array(items)) if items.len() <= 8 => items
            .iter()
            .enumerate()
            .map(|(index, item)| match item {
                Object::Name(name) => Filter::from_name(name, parms_at(index)),
                _ => Filter::Unknown,
            })
            .collect(),
        Some(Object::Array(_)) => vec![Filter::Unknown],
        _ => Vec::new(),
    }
}

/// Why decoding stopped early.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    Limit(Limit),
    Interrupted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    /// The data is not valid for its filter. What decoded so far was counted.
    Corrupt,
    Stop(Stop),
}

impl From<Stop> for DecodeError {
    fn from(stop: Stop) -> Self {
        Self::Stop(stop)
    }
}

pub(crate) type Sink<'s> = dyn FnMut(&[u8]) -> Result<(), Stop> + 's;

/// Decodes `input` through one byte filter into `out`.
pub(crate) fn decode(filter: Filter, input: &[u8], out: &mut Sink<'_>) -> Result<(), DecodeError> {
    match filter {
        Filter::Flate => flate(input, out),
        Filter::Lzw { early_change } => lzw(input, early_change, out),
        Filter::AsciiHex => ascii_hex(input, out),
        Filter::Ascii85 => ascii85(input, out),
        Filter::RunLength => run_length(input, out),
        Filter::Identity => out(input).map_err(DecodeError::Stop),
        Filter::Dct | Filter::Jpx | Filter::Jbig2 | Filter::Ccitt | Filter::Unknown => Ok(()),
    }
}

fn flate(mut input: &[u8], out: &mut Sink<'_>) -> Result<(), DecodeError> {
    let mut state = InflateState::new_boxed(DataFormat::ZLibIgnoreChecksum);
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let result = inflate(&mut state, input, &mut buffer, MZFlush::None);
        input = input.get(result.bytes_consumed..).unwrap_or_default();
        if result.bytes_written > 0 {
            out(buffer.get(..result.bytes_written).unwrap_or_default())?;
        }
        match result.status {
            Ok(MZStatus::StreamEnd) => return Ok(()),
            Ok(_) if result.bytes_consumed == 0 && result.bytes_written == 0 => {
                // No progress: the data ended early, which PDFium tolerates.
                return Ok(());
            }
            Ok(_) => {}
            Err(MZError::Buf) => return Ok(()),
            Err(_) => return Err(DecodeError::Corrupt),
        }
    }
}

/// Collects bytes for `out` and flushes them in chunks.
struct Chunker<'o, 's> {
    buffer: Vec<u8>,
    out: &'o mut Sink<'s>,
}

impl<'o, 's> Chunker<'o, 's> {
    fn new(out: &'o mut Sink<'s>) -> Self {
        Self {
            buffer: Vec::with_capacity(CHUNK),
            out,
        }
    }

    fn push(&mut self, byte: u8) -> Result<(), Stop> {
        self.buffer.push(byte);
        if self.buffer.len() >= CHUNK {
            self.flush()?;
        }
        Ok(())
    }

    fn extend(&mut self, bytes: &[u8]) -> Result<(), Stop> {
        for &byte in bytes {
            self.push(byte)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Stop> {
        if !self.buffer.is_empty() {
            (self.out)(&self.buffer)?;
            self.buffer.clear();
        }
        Ok(())
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn ascii_hex(input: &[u8], out: &mut Sink<'_>) -> Result<(), DecodeError> {
    let mut chunker = Chunker::new(out);
    let mut high: Option<u8> = None;
    for &byte in input {
        if byte == b'>' {
            break;
        }
        if is_whitespace(byte) {
            continue;
        }
        let digit = hex_digit(byte).ok_or(DecodeError::Corrupt)?;
        match high.take() {
            Some(value) => chunker.push(value * 16 + digit)?,
            None => high = Some(digit),
        }
    }
    if let Some(value) = high {
        chunker.push(value * 16)?;
    }
    chunker.flush()?;
    Ok(())
}

fn ascii85(input: &[u8], out: &mut Sink<'_>) -> Result<(), DecodeError> {
    let mut chunker = Chunker::new(out);
    let mut group = [0u32; 5];
    let mut count = 0usize;
    for &byte in input {
        match byte {
            b'~' => break,
            b'z' if count == 0 => chunker.extend(&[0; 4])?,
            b'!'..=b'u' => {
                if let Some(slot) = group.get_mut(count) {
                    *slot = u32::from(byte - b'!');
                }
                count += 1;
                if count == 5 {
                    let value = group
                        .iter()
                        .try_fold(0u32, |acc, &digit| acc.checked_mul(85)?.checked_add(digit))
                        .ok_or(DecodeError::Corrupt)?;
                    chunker.extend(&value.to_be_bytes())?;
                    count = 0;
                }
            }
            byte if is_whitespace(byte) => {}
            _ => return Err(DecodeError::Corrupt),
        }
    }
    if count > 1 {
        for slot in group.iter_mut().skip(count) {
            *slot = 84;
        }
        let value = group
            .iter()
            .try_fold(0u32, |acc, &digit| acc.checked_mul(85)?.checked_add(digit))
            .ok_or(DecodeError::Corrupt)?;
        let bytes = value.to_be_bytes();
        chunker.extend(bytes.get(..count - 1).unwrap_or_default())?;
    }
    chunker.flush()?;
    Ok(())
}

fn run_length(input: &[u8], out: &mut Sink<'_>) -> Result<(), DecodeError> {
    let mut chunker = Chunker::new(out);
    let mut index = 0usize;
    while let Some(&length) = input.get(index) {
        index += 1;
        match length {
            128 => break,
            0..=127 => {
                let end = index + usize::from(length) + 1;
                chunker.extend(input.get(index..end.min(input.len())).unwrap_or_default())?;
                index = end;
            }
            _ => {
                let Some(&byte) = input.get(index) else { break };
                index += 1;
                for _ in 0..(257 - usize::from(length)) {
                    chunker.push(byte)?;
                }
            }
        }
    }
    chunker.flush()?;
    Ok(())
}

struct Bits<'a> {
    data: &'a [u8],
    bit: usize,
}

impl Bits<'_> {
    fn read(&mut self, width: u32) -> Option<u16> {
        let mut value: u16 = 0;
        for _ in 0..width {
            let byte = *self.data.get(self.bit / 8)?;
            let bit = (byte >> (7 - (self.bit % 8))) & 1;
            value = (value << 1) | u16::from(bit);
            self.bit += 1;
        }
        Some(value)
    }
}

fn lzw(input: &[u8], early_change: bool, out: &mut Sink<'_>) -> Result<(), DecodeError> {
    const CLEAR: u16 = 256;
    const END: u16 = 257;
    // Each entry: (prefix code or u16::MAX, last byte, first byte).
    let mut table: Vec<(u16, u8, u8)> = Vec::with_capacity(4096);
    let reset = |table: &mut Vec<(u16, u8, u8)>| {
        table.clear();
        table.extend((0..=255u8).map(|byte| (u16::MAX, byte, byte)));
        table.extend([(u16::MAX, 0, 0), (u16::MAX, 0, 0)]);
    };
    reset(&mut table);
    let mut chunker = Chunker::new(out);
    let mut bits = Bits {
        data: input,
        bit: 0,
    };
    let mut width = 9u32;
    let mut previous: Option<u16> = None;
    let mut scratch = Vec::with_capacity(4096);
    while let Some(code) = bits.read(width) {
        if code == CLEAR {
            reset(&mut table);
            width = 9;
            previous = None;
            continue;
        }
        if code == END {
            break;
        }
        let known = usize::from(code) < table.len();
        let first = if known {
            table.get(usize::from(code)).map(|entry| entry.2)
        } else {
            previous.and_then(|p| table.get(usize::from(p)).map(|entry| entry.2))
        }
        .ok_or(DecodeError::Corrupt)?;
        if let Some(prefix) = previous {
            if table.len() < 4096 {
                let prefix_first = table.get(usize::from(prefix)).map_or(0, |entry| entry.2);
                table.push((prefix, first, prefix_first));
            }
        }
        if usize::from(code) >= table.len() {
            return Err(DecodeError::Corrupt);
        }
        scratch.clear();
        let mut cursor = code;
        while cursor != u16::MAX && scratch.len() <= 4096 {
            let &(prefix, byte, _) = table.get(usize::from(cursor)).ok_or(DecodeError::Corrupt)?;
            scratch.push(byte);
            cursor = prefix;
        }
        scratch.reverse();
        chunker.extend(&scratch)?;
        previous = Some(code);
        let threshold = (1usize << width) - usize::from(early_change);
        if table.len() >= threshold && width < 12 {
            width += 1;
        }
    }
    chunker.flush()?;
    Ok(())
}

/// Undoes PNG predictors (`/Predictor` 10 to 15) for data PaperWarden must
/// read, such as cross-reference streams.
pub(crate) fn unpredict(data: &[u8], parms: Option<&Dict>) -> Option<Vec<u8>> {
    let predictor = parms.and_then(|p| p.int(b"Predictor")).unwrap_or(1);
    if predictor < 10 {
        return (predictor == 1).then(|| data.to_vec());
    }
    let columns = usize::try_from(parms.and_then(|p| p.int(b"Columns")).unwrap_or(1)).ok()?;
    let colors = usize::try_from(parms.and_then(|p| p.int(b"Colors")).unwrap_or(1)).ok()?;
    let bpc = usize::try_from(parms.and_then(|p| p.int(b"BitsPerComponent")).unwrap_or(8)).ok()?;
    let pixel = colors.checked_mul(bpc)?.div_ceil(8);
    let row = columns.checked_mul(colors)?.checked_mul(bpc)?.div_ceil(8);
    if row == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len());
    let mut previous = vec![0u8; row];
    for line in data.chunks(row + 1) {
        let (&kind, bytes) = line.split_first()?;
        let mut current = bytes.to_vec();
        current.resize(row, 0);
        for index in 0..row {
            let left = index
                .checked_sub(pixel)
                .and_then(|i| current.get(i).copied())
                .unwrap_or(0);
            let up = previous.get(index).copied().unwrap_or(0);
            let up_left = index
                .checked_sub(pixel)
                .and_then(|i| previous.get(i).copied())
                .unwrap_or(0);
            let add = match kind {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth(left, up, up_left),
                _ => return None,
            };
            if let Some(byte) = current.get_mut(index) {
                *byte = byte.wrapping_add(add);
            }
        }
        out.extend_from_slice(&current);
        previous = current;
    }
    Some(out)
}

fn paeth(left: u8, up: u8, up_left: u8) -> u8 {
    let estimate = i16::from(left) + i16::from(up) - i16::from(up_left);
    let to_left = (estimate - i16::from(left)).abs();
    let to_up = (estimate - i16::from(up)).abs();
    let to_up_left = (estimate - i16::from(up_left)).abs();
    if to_left <= to_up && to_left <= to_up_left {
        left
    } else if to_up <= to_up_left {
        up
    } else {
        up_left
    }
}

fn be16(data: &[u8], at: usize) -> Option<u64> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u64::from(u16::from_be_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
    ])))
}

fn be32(data: &[u8], at: usize) -> Option<u64> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    let mut array = [0u8; 4];
    array.copy_from_slice(bytes);
    Some(u64::from(u32::from_be_bytes(array)))
}

/// Width, height and components from a JPEG's frame header.
pub(crate) fn jpeg_size(data: &[u8]) -> Option<(u64, u64, u64)> {
    if data.get(..2) != Some(&[0xFF, 0xD8][..]) {
        return None;
    }
    let mut at = 2usize;
    loop {
        // Skip fill bytes before a marker.
        while data.get(at) == Some(&0xFF) && data.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if data.get(at) != Some(&0xFF) {
            return None;
        }
        let marker = *data.get(at + 1)?;
        at += 2;
        match marker {
            0xD8 | 0x01 | 0xD0..=0xD7 => continue,
            0xD9 | 0xDA => return None,
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                let height = be16(data, at + 3)?;
                let width = be16(data, at + 5)?;
                let components = u64::from(*data.get(at + 7)?);
                return Some((width, height, components));
            }
            _ => at = at.checked_add(usize::try_from(be16(data, at)?).ok()?)?,
        }
    }
}

/// Width, height and components from a JPEG 2000 codestream's `SIZ` marker.
pub(crate) fn jpx_size(data: &[u8]) -> Option<(u64, u64, u64)> {
    let window = data.get(..data.len().min(1 << 16))?;
    let start = window
        .windows(4)
        .position(|w| w == [0xFF, 0x4F, 0xFF, 0x51])?
        + 4;
    let width = be32(data, start + 4)?.checked_sub(be32(data, start + 12)?)?;
    let height = be32(data, start + 8)?.checked_sub(be32(data, start + 16)?)?;
    let components = be16(data, start + 36)?;
    Some((width, height, components))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(filter: Filter, input: &[u8]) -> Result<Vec<u8>, DecodeError> {
        let mut out = Vec::new();
        decode(filter, input, &mut |chunk| {
            out.extend_from_slice(chunk);
            Ok(())
        })?;
        Ok(out)
    }

    #[test]
    fn flate_round_trips() {
        let data = b"PaperWarden ".repeat(10_000);
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&data, 6);
        assert_eq!(collect(Filter::Flate, &compressed), Ok(data));
    }

    #[test]
    fn flate_garbage_is_corrupt_not_a_panic() {
        assert_eq!(
            collect(Filter::Flate, b"\x78\x9cnot deflate at all"),
            Err(DecodeError::Corrupt)
        );
        assert_eq!(collect(Filter::Flate, b""), Ok(Vec::new()));
    }

    #[test]
    fn a_sink_can_stop_a_bomb_early() {
        let bomb = miniz_oxide::deflate::compress_to_vec_zlib(&vec![0u8; 8 << 20], 9);
        let mut seen = 0usize;
        let result = decode(Filter::Flate, &bomb, &mut |chunk| {
            seen += chunk.len();
            if seen > 1 << 20 {
                Err(Stop::Limit(Limit::CompressionRatio))
            } else {
                Ok(())
            }
        });
        assert_eq!(
            result,
            Err(DecodeError::Stop(Stop::Limit(Limit::CompressionRatio)))
        );
        assert!(seen <= (1 << 20) + CHUNK);
    }

    #[test]
    fn ascii_filters_and_run_length_decode() {
        assert_eq!(collect(Filter::AsciiHex, b"50 61 7>"), Ok(b"Pap".to_vec()));
        assert_eq!(
            collect(Filter::Ascii85, b"87cURD]j7BEbo7~>"),
            Ok(b"Hello world".to_vec())
        );
        assert_eq!(collect(Filter::Ascii85, b"z~>"), Ok(vec![0; 4]));
        assert_eq!(
            collect(Filter::RunLength, &[2, b'a', b'b', b'c', 253, b'x', 128]),
            Ok(b"abcxxxx".to_vec())
        );
        assert_eq!(collect(Filter::AsciiHex, b"zz"), Err(DecodeError::Corrupt));
    }

    #[test]
    fn lzw_decodes_the_spec_example() {
        // PDF 32000-1, 7.4.4.2: 45 45 45 45 45 65 45 45 45 66 encodes to
        // 80 0B 60 50 22 0C 0C 85 01.
        let encoded = [0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
        assert_eq!(
            collect(Filter::Lzw { early_change: true }, &encoded),
            Ok(vec![45, 45, 45, 45, 45, 65, 45, 45, 45, 66])
        );
    }

    #[test]
    fn png_up_predictor_is_undone() {
        let rows = [2u8, 1, 2, 2, 1, 1];
        let mut parms = Dict::default();
        parms.0.push((b"Predictor".to_vec(), Object::Int(12)));
        parms.0.push((b"Columns".to_vec(), Object::Int(2)));
        assert_eq!(unpredict(&rows, Some(&parms)), Some(vec![1, 2, 2, 3]));
    }

    #[test]
    fn jpeg_frame_size_is_read_from_the_header() {
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x10,
            0x00, 0x20, 0x00, 0x03,
        ];
        assert_eq!(jpeg_size(&jpeg), Some((0x2000, 0x1000, 3)));
        assert_eq!(jpeg_size(b"not a jpeg"), None);
    }
}
