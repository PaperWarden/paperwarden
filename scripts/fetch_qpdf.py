"""Fetch the pinned, verified sources that the qpdf prototype builds from (PW-012).

qpdf is built from upstream source, never from a third party's vendored copy,
together with the two libraries it needs: zlib and IJG libjpeg. Each release
archive is downloaded over HTTPS only, checked against the SHA-256 pinned
here, and only the files the build uses are extracted (no symlinks, no paths
outside the destination).

Where each digest comes from:
- qpdf: the release's own qpdf-<version>.sha256 (also signed with Sigstore),
  and the digest GitHub records for the asset.
- zlib: the digest GitHub records for the release asset.
- libjpeg: IJG publishes no checksum; the digest was pinned on first download
  (2026-11-28) and matches the one common package recipes pin for 9f.

Updating a source means changing its entry below in one reviewed change.

The result goes to third_party/qpdf/src/ (ignored by git), which
core/crates/qpdf/build.rs compiles when present.

Usage:
    python scripts/fetch_qpdf.py [--dest DIR] [--archives DIR]

--archives reads the release archives from DIR (named as in their URLs)
instead of downloading them; they are verified the same way.
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import io
import pathlib
import sys
import tarfile
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
MAX_DOWNLOAD_BYTES = 64 * 1024 * 1024


@dataclasses.dataclass(frozen=True)
class Source:
    name: str
    version: str
    url: str
    sha256: str
    # Top-level directory inside the archive.
    root: str
    # Paths kept, relative to `root`: a directory prefix ending in "/", or a
    # file name. "*" keeps every regular file directly inside `root`.
    keep: tuple[str, ...]


SOURCES = (
    Source(
        name="qpdf",
        version="12.4.2",
        url="https://github.com/qpdf/qpdf/releases/download/v12.4.2/qpdf-12.4.2.tar.gz",
        sha256="8a58af5b6141319287c1883bec8bd1bd545b7567b7fc5e6ce5d25a1c85f36397",
        root="qpdf-12.4.2",
        keep=("libqpdf/", "include/", "LICENSE.txt", "NOTICE.md"),
    ),
    Source(
        name="zlib",
        version="1.3.2",
        url="https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz",
        sha256="bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16",
        root="zlib-1.3.2",
        keep=("*",),
    ),
    Source(
        name="jpeg",
        version="9f",
        url="https://www.ijg.org/files/jpegsrc.v9f.tar.gz",
        sha256="04705c110cb2469caa79fb71fba3d7bf834914706e9641a4589485c1f832565b",
        root="jpeg-9f",
        keep=("*",),
    ),
)

ALLOWED_PREFIXES = (
    "https://github.com/qpdf/qpdf/releases/download/",
    "https://github.com/madler/zlib/releases/download/",
    "https://www.ijg.org/files/",
)


class HttpsOnlyRedirects(urllib.request.HTTPRedirectHandler):
    """Follow GitHub's redirect to its asset host, but never off HTTPS."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: ANN001, ANN201
        if urllib.parse.urlsplit(newurl).scheme != "https":
            raise SystemExit(f"refusing a non-HTTPS redirect: {newurl}")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def https_opener() -> urllib.request.OpenerDirector:
    # Built by hand rather than with build_opener() or urlopen(), so there is
    # no file://, ftp:// or plain http:// handler to reach at all.
    opener = urllib.request.OpenerDirector()
    for handler in (
        urllib.request.HTTPSHandler(),
        HttpsOnlyRedirects(),
        urllib.request.HTTPErrorProcessor(),
        urllib.request.UnknownHandler(),  # any other scheme is an error
    ):
        opener.add_handler(handler)
    return opener


def download(url: str) -> bytes:
    if not url.startswith(ALLOWED_PREFIXES):
        raise SystemExit(f"refusing a URL outside the pinned sources: {url}")
    request = urllib.request.Request(url, headers={"User-Agent": "paperwarden-fetch"})
    # The URL is one of the pinned SOURCES and the opener only speaks HTTPS,
    # so no caller can choose the scheme or host.
    with https_opener().open(request, timeout=120) as response:  # nosemgrep: python.lang.security.audit.dynamic-urllib-use-detected.dynamic-urllib-use-detected
        data = response.read(MAX_DOWNLOAD_BYTES + 1)
    if len(data) > MAX_DOWNLOAD_BYTES:
        raise SystemExit(f"download exceeds {MAX_DOWNLOAD_BYTES} bytes")
    return data


def kept(source: Source, relative: str) -> bool:
    for rule in source.keep:
        if rule == "*" and "/" not in relative:
            return True
        if rule.endswith("/") and relative.startswith(rule):
            return True
        if relative == rule:
            return True
    return False


def safe_members(archive: tarfile.TarFile, source: Source) -> list[tarfile.TarInfo]:
    members = []
    for member in archive.getmembers():
        name = member.name
        parts = pathlib.PurePosixPath(name).parts
        if name.startswith(("/", "\\")) or ".." in parts or "\\" in name:
            raise SystemExit(f"unsafe path in {source.name} archive: {name!r}")
        if not member.isfile() or not parts or parts[0] != source.root:
            continue
        relative = "/".join(parts[1:])
        if relative and kept(source, relative):
            members.append(member)
    return members


def fetch(source: Source, dest: pathlib.Path, archives: pathlib.Path | None = None) -> int:
    if archives is None:
        data = download(source.url)
    else:
        local = archives / urllib.parse.urlsplit(source.url).path.rsplit("/", 1)[-1]
        if local.stat().st_size > MAX_DOWNLOAD_BYTES:
            raise SystemExit(f"{local} exceeds {MAX_DOWNLOAD_BYTES} bytes")
        data = local.read_bytes()
    actual = hashlib.sha256(data).hexdigest()
    if actual != source.sha256:
        raise SystemExit(f"digest mismatch for {source.url}: {actual}")
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        members = safe_members(archive, source)
        if not members:
            raise SystemExit(f"nothing to extract from {source.url}")
        target = dest / source.name
        target.mkdir(parents=True, exist_ok=True)
        for member in members:
            relative = "/".join(pathlib.PurePosixPath(member.name).parts[1:])
            member = member.replace(name=relative)
            archive.extract(member, target, filter="data")
    (dest / source.name / "PAPERWARDEN_SOURCE").write_text(
        f"{source.name} {source.version}\n{source.url}\nsha256 {source.sha256}\n",
        encoding="utf-8",
    )
    return len(members)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Fetch the pinned qpdf, zlib and libjpeg sources.")
    parser.add_argument("--dest", type=pathlib.Path, default=ROOT / "third_party" / "qpdf" / "src")
    parser.add_argument("--archives", type=pathlib.Path, help="read the archives from here instead")
    arguments = parser.parse_args(argv)
    for source in SOURCES:
        count = fetch(source, arguments.dest, arguments.archives)
        print(f"{source.name} {source.version} verified; {count} files extracted to {arguments.dest / source.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
