"""Tests for scripts/fetch_pdfium.py, offline: no test touches the network.

The fetch must speak only HTTPS to the pinned source, refuse a PDFium build
with V8 or XFA (PRODUCT_INVARIANTS.md 6), and never extract outside its
destination.
"""

from __future__ import annotations

import hashlib
import io
import pathlib
import shutil
import sys
import tarfile
import tempfile
import unittest
import urllib.error
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import fetch_pdfium  # noqa: E402

SAFE_ARGS = b"is_debug = false\npdf_enable_v8 = false\npdf_enable_xfa = false\n"


def archive(entries: dict[str, bytes]) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
        for name, data in entries.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


class Transport(unittest.TestCase):
    def test_opener_has_no_handler_for_other_schemes(self) -> None:
        opener = fetch_pdfium.https_opener()
        for url in ("file:///etc/passwd", "http://example.com/x", "ftp://example.com/x", "data:,x"):
            with self.subTest(url=url), self.assertRaises(urllib.error.URLError):
                opener.open(url)

    def test_redirects_off_https_are_refused(self) -> None:
        handler = fetch_pdfium.HttpsOnlyRedirects()
        with self.assertRaises(SystemExit):
            handler.redirect_request(None, None, 302, "Found", {}, "http://example.com/x")

    def test_download_refuses_urls_outside_the_pinned_source(self) -> None:
        for url in ("file:///etc/passwd", "https://example.com/pdfium.tgz", fetch_pdfium.SOURCE + "evil/x"):
            with self.subTest(url=url), self.assertRaises(SystemExit):
                fetch_pdfium.download(url)


class Fetch(unittest.TestCase):
    def run_fetch(self, data: bytes) -> pathlib.Path:
        dest = pathlib.Path(tempfile.mkdtemp()) / "out"
        self.addCleanup(shutil.rmtree, dest.parent, True)
        digest = hashlib.sha256(data).hexdigest()
        with (
            mock.patch.dict(fetch_pdfium.ASSETS, {"test": digest}),
            mock.patch.object(fetch_pdfium, "download", return_value=data),
        ):
            fetch_pdfium.fetch("test", dest)
        return dest

    def test_a_safe_build_extracts_only_the_library_and_licenses(self) -> None:
        dest = self.run_fetch(
            archive(
                {
                    "args.gn": SAFE_ARGS,
                    "LICENSE": b"license",
                    "lib/libpdfium.so": b"\x7fELF",
                    "licenses/zlib.txt": b"zlib",
                    "include/fpdfview.h": b"header",
                }
            )
        )
        files = sorted(str(path.relative_to(dest)).replace("\\", "/") for path in dest.rglob("*") if path.is_file())
        self.assertEqual(files, ["LICENSE", "args.gn", "lib/libpdfium.so", "licenses/zlib.txt"])

    def test_v8_and_xfa_builds_are_refused(self) -> None:
        for args in (
            b"pdf_enable_v8 = true\npdf_enable_xfa = false\n",
            b"pdf_enable_v8 = false\npdf_enable_xfa = true\n",
            b"is_debug = false\n",
        ):
            with self.subTest(args=args), self.assertRaises(SystemExit):
                self.run_fetch(archive({"args.gn": args, "lib/libpdfium.so": b"x"}))

    def test_a_digest_mismatch_is_refused(self) -> None:
        dest = pathlib.Path(tempfile.mkdtemp())
        with (
            mock.patch.dict(fetch_pdfium.ASSETS, {"test": "0" * 64}),
            mock.patch.object(fetch_pdfium, "download", return_value=archive({"args.gn": SAFE_ARGS})),
            self.assertRaises(SystemExit),
        ):
            fetch_pdfium.fetch("test", dest / "out")
        self.assertFalse((dest / "out").exists())

    def test_traversal_and_absolute_paths_are_refused(self) -> None:
        for name in ("../escape", "lib/../../escape", "/etc/passwd"):
            with self.subTest(name=name), self.assertRaises(SystemExit):
                self.run_fetch(archive({"args.gn": SAFE_ARGS, name: b"x"}))


if __name__ == "__main__":
    unittest.main()
