"""Make the encrypted corpus PDFs with pypdf, an encryption implementation
independent of PaperWarden's own (PW-042, #67).

AES encryption uses random IVs, so the output differs on every run. The
committed fixtures are therefore originals (`ORIGINALS` in generate_corpus.py),
and this script records how they were made. Rerunning it is only needed to
replace them; then refresh the manifest with `python scripts/generate_corpus.py`.

Made with pypdf 6.19.0 and cryptography 50.0.1:
    python -I scripts/make_encrypted_fixtures.py
"""

from __future__ import annotations

import pathlib
import zlib

from pypdf import PdfWriter
from pypdf.generic import DecodedStreamObject, DictionaryObject, NameObject

ROOT = pathlib.Path(__file__).resolve().parents[1]
CORPUS = ROOT / "tests" / "corpus"
TEXT = b"BT /F1 18 Tf 20 40 Td (PaperWarden corpus) Tj ET"


def page_writer(content: bytes) -> PdfWriter:
    writer = PdfWriter()
    page = writer.add_blank_page(300, 100)
    font = DictionaryObject({
        NameObject("/Type"): NameObject("/Font"),
        NameObject("/Subtype"): NameObject("/Type1"),
        NameObject("/BaseFont"): NameObject("/Helvetica"),
    })
    page[NameObject("/Resources")] = DictionaryObject({
        NameObject("/Font"): DictionaryObject({NameObject("/F1"): writer._add_object(font)}),
    })
    stream = DecodedStreamObject()
    stream.set_data(content)
    page[NameObject("/Contents")] = writer._add_object(stream)
    page.compress_content_streams()
    writer.add_metadata({"/Producer": "PaperWarden corpus (pypdf)"})
    return writer


def write(path: str, writer: PdfWriter, algorithm: str, user: str = "") -> None:
    writer.encrypt(user_password=user, owner_password="paperwarden-owner", algorithm=algorithm)
    target = CORPUS / path
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open("wb") as handle:
        writer.write(handle)
    print(f"{path}: {target.stat().st_size} bytes")


def main() -> None:
    write("valid/pdf/encrypted-rc4-128.pdf", page_writer(TEXT), "RC4-128")
    write("valid/pdf/encrypted-aes-128.pdf", page_writer(TEXT), "AES-128")
    write("valid/pdf/encrypted-aes-256.pdf", page_writer(TEXT), "AES-256")

    # 16 MiB of spaces after the text: inert content, about 1000:1 compressed.
    bomb = page_writer(TEXT + b"\n" + b" " * (16 << 20))
    write("security/pdf/encrypted-flate-bomb.pdf", bomb, "AES-128")

    script = page_writer(TEXT)
    script.add_js("// PaperWarden corpus: inert comment, never run")
    write("security/pdf/encrypted-javascript.pdf", script, "AES-256")

    write("security/pdf/encrypted-needs-password.pdf", page_writer(TEXT), "AES-256", user="paperwarden")

    # Sanity: the bomb really expands.
    data = (CORPUS / "security/pdf/encrypted-flate-bomb.pdf").read_bytes()
    assert len(data) < 1 << 20 and len(zlib.compress(b" " * (16 << 20), 9)) < 64 << 10


if __name__ == "__main__":
    main()
