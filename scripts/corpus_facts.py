"""Verify structural facts about corpus fixtures without any document engine.

A fact is a property the manifest claims about a fixture's bytes, such as
"this ZIP holds exactly these entry names" or "this stream inflates to 16 MiB".
check_corpus.py computes each fact here, independently of the generator that
declared it, so a fixture cannot silently stop being what it claims to be.

Standard library only. XML is never parsed when it declares a DTD, so entity
expansion and external entities in security fixtures are never resolved.
Decompression is capped at MAX_INFLATE bytes.
"""

from __future__ import annotations

import csv
import io
import re
import stat
import struct
import zipfile
import zlib
from collections.abc import Callable
from typing import Any
from xml.etree import ElementTree

MAX_INFLATE = 64 * 1024 * 1024
XML_SUFFIXES = (".xml", ".rels")


class FactError(Exception):
    """The fixture could not be inspected for this fact."""


# --- Generic -----------------------------------------------------------------


def signature(data: bytes) -> str:
    return data[:8].hex()


def _decoded(data: bytes) -> str:
    if data.startswith((b"\xff\xfe", b"\xfe\xff")):
        return data.decode("utf-16")
    return data.decode("utf-8-sig")


def utf8(data: bytes) -> bool:
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return False
    return True


def contains(data: bytes, needles: list[str]) -> list[str]:
    return [needle for needle in needles if needle.encode("utf-8") in data]


def csv_shape(data: bytes) -> list[int]:
    rows = list(csv.reader(io.StringIO(data.decode("utf-8"))))
    return [len(rows), max((len(row) for row in rows), default=0)]


# --- PDF ---------------------------------------------------------------------

_STARTXREF = re.compile(rb"startxref\s+(\d+)")
_STREAM = re.compile(rb"/Length (\d+)([^>]*)>>\s*stream\r?\n")


def _last_startxref(data: bytes) -> int | None:
    found = _STARTXREF.findall(data)
    return int(found[-1]) if found else None


def pdf_xref_valid(data: bytes) -> bool:
    """Every in-use entry of the last xref section points at its object."""
    offset = _last_startxref(data)
    if offset is None or not data[offset:].startswith(b"xref"):
        return False
    lines = data[offset:].split(b"trailer", 1)[0].splitlines()[1:]
    checked = 0
    index = 0
    while index < len(lines):
        header = lines[index].split()
        index += 1
        if len(header) != 2:
            continue
        first, count = int(header[0]), int(header[1])
        for number in range(first, first + count):
            if index >= len(lines):
                return False
            entry = lines[index].split()
            index += 1
            if len(entry) == 3 and entry[2] == b"n":
                position = int(entry[0])
                if not re.match(rb"%d \d+ obj" % number, data[position:position + 24]):
                    return False
                checked += 1
    return checked > 0


def _streams(data: bytes) -> list[bytes]:
    streams = []
    for match in _STREAM.finditer(data):
        body = data[match.end():match.end() + int(match.group(1))]
        if b"FlateDecode" in match.group(2):
            inflater = zlib.decompressobj()
            try:
                body = inflater.decompress(body, MAX_INFLATE)
            except zlib.error:
                continue
        streams.append(body)
    return streams


def pdf_text(data: bytes) -> list[str]:
    return [
        text.decode("latin-1")
        for stream in _streams(data)
        for text in re.findall(rb"\((.*?)\)\s*Tj", stream)
    ]


def max_inflated_bytes(data: bytes) -> int:
    return max((len(stream) for stream in _streams(data)), default=0)


def pdf_prev_loop(data: bytes) -> bool:
    offset = _last_startxref(data)
    if offset is None:
        return False
    trailer = data[offset:].split(b"startxref", 1)[0]
    return re.search(rb"/Prev %d\b" % offset, trailer) is not None


def max_bracket_depth(data: bytes) -> int:
    depth = deepest = 0
    for byte in data:
        if byte == 0x5B:
            depth += 1
            deepest = max(deepest, depth)
        elif byte == 0x5D and depth:
            depth -= 1
    return deepest


# --- ZIP-based ---------------------------------------------------------------


def _zip(data: bytes) -> zipfile.ZipFile:
    try:
        return zipfile.ZipFile(io.BytesIO(data))
    except (zipfile.BadZipFile, OSError, ValueError, struct.error) as error:
        raise FactError(f"not a readable ZIP: {error}") from error


def zip_valid(data: bytes) -> bool:
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            return archive.testzip() is None
    except (zipfile.BadZipFile, OSError, EOFError, zlib.error, ValueError, struct.error, NotImplementedError):
        # Malformed archives make zipfile raise any of these; all mean invalid.
        return False


def zip_entries(data: bytes) -> list[str]:
    # orig_filename keeps backslashes that ZipInfo rewrites on Windows.
    return [info.orig_filename for info in _zip(data).infolist()]


def zip_utf8_flags(data: bytes) -> list[bool]:
    return [bool(info.flag_bits & 0x800) for info in _zip(data).infolist()]


def zip_symlinks(data: bytes) -> int:
    return sum(
        stat.S_ISLNK(info.external_attr >> 16)
        for info in _zip(data).infolist()
        if info.create_system == 3
    )


def zip_uncompressed_bytes(data: bytes) -> int:
    return sum(info.file_size for info in _zip(data).infolist())


def zip_ratio_at_least(data: bytes) -> int:
    """The largest whole compression ratio of any entry."""
    return max(
        (info.file_size // max(info.compress_size, 1) for info in _zip(data).infolist()),
        default=0,
    )


def zip_nesting_depth(data: bytes) -> int:
    depth = 1
    archive = _zip(data)
    while True:
        inner = [name for name in archive.namelist() if name.endswith(".zip")]
        if not inner:
            return depth
        archive = _zip(archive.read(inner[0]))
        depth += 1


def zip_first_stored(data: bytes) -> str:
    first = _zip(data).infolist()[0]
    return first.orig_filename if first.compress_type == zipfile.ZIP_STORED else ""


def zip_text_contains(data: bytes, expected: dict[str, str]) -> dict[str, str]:
    archive = _zip(data)
    found = {}
    for member, needle in expected.items():
        try:
            text = archive.read(member).decode("utf-8")
        except KeyError:
            continue
        if needle in text:
            found[member] = needle
    return found


def xml_doctype(data: bytes) -> bool:
    archive = _zip(data)
    return any(
        b"<!DOCTYPE" in archive.read(name)
        for name in archive.namelist()
        if name.endswith(XML_SUFFIXES)
    )


def xml_wellformed(data: bytes) -> bool:
    """Every XML part parses and none declares a DTD."""
    archive = _zip(data)
    for name in archive.namelist():
        if not name.endswith(XML_SUFFIXES):
            continue
        part = archive.read(name)
        if b"<!DOCTYPE" in part:
            return False
        try:
            ElementTree.fromstring(part)
        except ElementTree.ParseError:
            return False
    return True


# --- Images ------------------------------------------------------------------


def _png_chunks(data: bytes) -> list[tuple[bytes, bytes, bool]]:
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise FactError("not a PNG")
    chunks = []
    position = 8
    while position + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[position:position + 8])
        body = data[position + 8:position + 8 + length]
        crc = data[position + 8 + length:position + 12 + length]
        complete = len(body) == length and len(crc) == 4
        valid = complete and struct.unpack(">I", crc)[0] == zlib.crc32(kind + body) & 0xFFFFFFFF
        chunks.append((kind, body, valid))
        position += 12 + length
        if kind == b"IEND":
            break
    return chunks


def png_crc_valid(data: bytes) -> bool:
    return all(valid for _, _, valid in _png_chunks(data))


def png_complete(data: bytes) -> bool:
    chunks = _png_chunks(data)
    if not chunks or chunks[-1][0] != b"IEND":
        return False
    inflater = zlib.decompressobj()
    try:
        inflater.decompress(b"".join(body for kind, body, _ in chunks if kind == b"IDAT"), MAX_INFLATE)
    except zlib.error:
        return False
    return inflater.eof


def image_size(data: bytes) -> list[int]:
    if data.startswith(b"\x89PNG"):
        return list(struct.unpack(">II", data[16:24]))
    if data.startswith(b"BM"):
        width, height = struct.unpack("<ii", data[18:26])
        return [width, abs(height)]
    if data.startswith(b"\xff\xd8"):
        position = 2
        while position + 9 < len(data):
            marker, length = data[position + 1], struct.unpack(">H", data[position + 2:position + 4])[0]
            if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
                height, width = struct.unpack(">HH", data[position + 5:position + 9])
                return [width, height]
            position += 2 + length
        raise FactError("JPEG without a frame header")
    if data.startswith(b"RIFF") and data[8:12] == b"WEBP":
        chunk = data[12:16]
        if chunk == b"VP8L":
            bits = int.from_bytes(data[21:25], "little")
            return [(bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1]
        if chunk == b"VP8X":
            return [int.from_bytes(data[24:27], "little") + 1, int.from_bytes(data[27:30], "little") + 1]
        raise FactError(f"unsupported WebP chunk {chunk!r}")
    if data[:4] in (b"II*\0", b"MM\0*"):
        order = "<" if data[:2] == b"II" else ">"
        (ifd,) = struct.unpack(order + "I", data[4:8])
        (count,) = struct.unpack(order + "H", data[ifd:ifd + 2])
        size = {}
        for index in range(count):
            entry = data[ifd + 2 + 12 * index:ifd + 14 + 12 * index]
            tag, kind = struct.unpack(order + "HH", entry[:4])
            value = struct.unpack(order + ("H" if kind == 3 else "I"), entry[8:10] if kind == 3 else entry[8:12])[0]
            size[tag] = value
        return [size[256], size[257]]
    raise FactError("unknown image format")


# --- Registry ----------------------------------------------------------------

# Facts whose expected value is compared with what the function returns.
EQUALITY: dict[str, Callable[[bytes], Any]] = {
    "utf8": utf8,
    "text": _decoded,
    "csv_shape": csv_shape,
    "pdf_xref_valid": pdf_xref_valid,
    "pdf_text": pdf_text,
    "pdf_prev_loop": pdf_prev_loop,
    "max_inflated_bytes": max_inflated_bytes,
    "max_bracket_depth": max_bracket_depth,
    "zip_valid": zip_valid,
    "zip_entries": zip_entries,
    "zip_utf8_flags": zip_utf8_flags,
    "zip_symlinks": zip_symlinks,
    "zip_uncompressed_bytes": zip_uncompressed_bytes,
    "zip_nesting_depth": zip_nesting_depth,
    "zip_first_stored": zip_first_stored,
    "xml_doctype": xml_doctype,
    "xml_wellformed": xml_wellformed,
    "png_crc_valid": png_crc_valid,
    "png_complete": png_complete,
    "image_size": image_size,
}


def verify(kind: str, expected: Any, data: bytes) -> str | None:
    """Return None if the fact holds, otherwise why not."""
    try:
        if kind == "signature":
            return None if data.startswith(bytes.fromhex(expected)) else f"starts with {signature(data)}"
        if kind == "contains":
            missing = sorted(set(expected) - set(contains(data, expected)))
            return f"missing {missing}" if missing else None
        if kind == "absent":
            present = contains(data, expected)
            return f"present {present}" if present else None
        if kind == "zip_text_contains":
            found = zip_text_contains(data, expected)
            missing = {key: value for key, value in expected.items() if found.get(key) != value}
            return f"missing {missing}" if missing else None
        if kind == "zip_ratio_at_least":
            actual = zip_ratio_at_least(data)
            return None if actual >= expected else f"ratio {actual}"
        function = EQUALITY.get(kind)
        if function is None:
            return f"unknown fact kind {kind!r}"
        actual = function(data)
    except (FactError, UnicodeDecodeError, ValueError, KeyError, IndexError, struct.error) as error:
        return f"cannot inspect: {error}"
    return None if actual == expected else f"is {actual!r}"


KINDS = frozenset(EQUALITY) | {"signature", "contains", "absent", "zip_text_contains", "zip_ratio_at_least"}
