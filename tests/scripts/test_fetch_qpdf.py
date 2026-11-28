"""Tests for scripts/fetch_qpdf.py, offline: no test touches the network.

The fetch must speak only HTTPS to the pinned sources, refuse an archive
whose digest differs, keep only the files the build uses, and never extract
outside its destination.
"""

from __future__ import annotations

import dataclasses
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

import fetch_qpdf  # noqa: E402


def archive(entries: dict[str, bytes], links: tuple[str, ...] = ()) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
        for name, data in entries.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
        for name in links:
            info = tarfile.TarInfo(name)
            info.type = tarfile.SYMTYPE
            info.linkname = "/etc/passwd"
            tar.addfile(info)
    return buffer.getvalue()


def files(dest: pathlib.Path) -> list[str]:
    return sorted(
        str(path.relative_to(dest)).replace("\\", "/") for path in dest.rglob("*") if path.is_file()
    )


class Pins(unittest.TestCase):
    def test_every_source_is_pinned_to_an_allowed_https_url(self) -> None:
        self.assertEqual([source.name for source in fetch_qpdf.SOURCES], ["qpdf", "zlib", "jpeg"])
        for source in fetch_qpdf.SOURCES:
            with self.subTest(source.name):
                self.assertTrue(source.url.startswith(fetch_qpdf.ALLOWED_PREFIXES))
                self.assertRegex(source.sha256, r"^[0-9a-f]{64}$")
                self.assertIn(source.version, source.url)


class Transport(unittest.TestCase):
    def test_opener_has_no_handler_for_other_schemes(self) -> None:
        opener = fetch_qpdf.https_opener()
        for url in ("file:///etc/passwd", "http://example.com/x", "ftp://example.com/x", "data:,x"):
            with self.subTest(url=url), self.assertRaises(urllib.error.URLError):
                opener.open(url)

    def test_redirects_off_https_are_refused(self) -> None:
        handler = fetch_qpdf.HttpsOnlyRedirects()
        with self.assertRaises(SystemExit):
            handler.redirect_request(None, None, 302, "Found", {}, "http://example.com/x")

    def test_download_refuses_urls_outside_the_pinned_sources(self) -> None:
        for url in (
            "file:///etc/passwd",
            "https://example.com/qpdf.tar.gz",
            "https://github.com/evil/qpdf/releases/download/x.tar.gz",
            "http://www.ijg.org/files/jpegsrc.v9f.tar.gz",
        ):
            with self.subTest(url=url), self.assertRaises(SystemExit):
                fetch_qpdf.download(url)


TEST_SOURCE = fetch_qpdf.Source(
    name="qpdf",
    version="1",
    url="https://github.com/qpdf/qpdf/releases/download/v1/qpdf-1.tar.gz",
    sha256="",
    root="qpdf-1",
    keep=("libqpdf/", "include/", "LICENSE.txt"),
)


class Fetch(unittest.TestCase):
    def run_fetch(self, data: bytes, source: fetch_qpdf.Source = TEST_SOURCE) -> pathlib.Path:
        dest = pathlib.Path(tempfile.mkdtemp()) / "src"
        self.addCleanup(shutil.rmtree, dest.parent, True)
        source = dataclasses.replace(source, sha256=hashlib.sha256(data).hexdigest())
        with mock.patch.object(fetch_qpdf, "download", return_value=data):
            fetch_qpdf.fetch(source, dest)
        return dest

    def test_only_the_files_the_build_uses_are_extracted(self) -> None:
        dest = self.run_fetch(
            archive({
                "qpdf-1/LICENSE.txt": b"license",
                "qpdf-1/libqpdf/QPDF.cc": b"code",
                "qpdf-1/include/qpdf/QPDF.hh": b"header",
                "qpdf-1/qtest/huge-test.pdf": b"test data",
                "qpdf-1/README.md": b"readme",
                "other/stray.c": b"outside the root",
            })
        )
        self.assertEqual(
            files(dest),
            [
                "qpdf/LICENSE.txt",
                "qpdf/PAPERWARDEN_SOURCE",
                "qpdf/include/qpdf/QPDF.hh",
                "qpdf/libqpdf/QPDF.cc",
            ],
        )
        self.assertIn("sha256", (dest / "qpdf" / "PAPERWARDEN_SOURCE").read_text(encoding="utf-8"))

    def test_star_keeps_only_top_level_files(self) -> None:
        source = dataclasses.replace(TEST_SOURCE, name="zlib", root="zlib-1", keep=("*",))
        dest = self.run_fetch(
            archive({"zlib-1/inflate.c": b"c", "zlib-1/LICENSE": b"l", "zlib-1/contrib/x.c": b"c"}),
            source,
        )
        self.assertEqual(files(dest), ["zlib/LICENSE", "zlib/PAPERWARDEN_SOURCE", "zlib/inflate.c"])

    def test_links_are_never_extracted(self) -> None:
        dest = self.run_fetch(
            archive({"qpdf-1/libqpdf/QPDF.cc": b"code"}, links=("qpdf-1/libqpdf/link.cc",))
        )
        self.assertNotIn("qpdf/libqpdf/link.cc", files(dest))

    def test_a_digest_mismatch_is_refused_and_writes_nothing(self) -> None:
        dest = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, dest, True)
        source = dataclasses.replace(TEST_SOURCE, sha256="0" * 64)
        with (
            mock.patch.object(fetch_qpdf, "download", return_value=archive({"qpdf-1/LICENSE.txt": b"x"})),
            self.assertRaises(SystemExit),
        ):
            fetch_qpdf.fetch(source, dest / "src")
        self.assertFalse((dest / "src").exists())

    def test_traversal_and_absolute_paths_are_refused(self) -> None:
        for name in ("../escape", "qpdf-1/libqpdf/../../../escape", "/etc/passwd", "qpdf-1\\..\\x"):
            with self.subTest(name=name), self.assertRaises(SystemExit):
                self.run_fetch(archive({name: b"x"}))

    def test_local_archives_are_verified_like_downloads(self) -> None:
        directory = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory, True)
        data = archive({"qpdf-1/LICENSE.txt": b"license"})
        (directory / "qpdf-1.tar.gz").write_bytes(data)
        good = dataclasses.replace(TEST_SOURCE, sha256=hashlib.sha256(data).hexdigest())
        with mock.patch.object(fetch_qpdf, "download", side_effect=AssertionError("downloaded")):
            fetch_qpdf.fetch(good, directory / "src", directory)
            self.assertTrue((directory / "src" / "qpdf" / "LICENSE.txt").is_file())
            with self.assertRaises(SystemExit):
                fetch_qpdf.fetch(dataclasses.replace(good, sha256="0" * 64), directory / "bad", directory)

    def test_an_archive_with_nothing_to_keep_is_refused(self) -> None:
        with self.assertRaises(SystemExit):
            self.run_fetch(archive({"qpdf-1/README.md": b"x"}))


if __name__ == "__main__":
    unittest.main()
