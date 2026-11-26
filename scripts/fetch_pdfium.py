"""Fetch a pinned, verified PDFium build for one platform.

PDFium comes from the bblanchon/pdfium-binaries project, which builds Google's
PDFium source in GitHub Actions and attests the build provenance. This script
downloads one release asset, checks its SHA-256 against the digest pinned here,
refuses builds with V8 (JavaScript) or XFA enabled (PRODUCT_INVARIANTS.md 6),
and extracts only the library and its license files.

The result goes to third_party/pdfium/<platform>/ (ignored by git). Updating
PDFium means changing RELEASE and every digest in one reviewed change, after
checking the new release's attestation:
    gh attestation verify <asset> --repo bblanchon/pdfium-binaries

Usage:
    python scripts/fetch_pdfium.py android-x64 [--dest DIR]
"""

from __future__ import annotations

import argparse
import hashlib
import io
import pathlib
import sys
import tarfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
RELEASE = "chromium/8125"  # PDFium 158.0.8125.0, published 2026-10-26
SOURCE = "https://github.com/bblanchon/pdfium-binaries/releases/download"

# Non-V8 assets only. Digests are the ones GitHub records for the release.
ASSETS = {
    "android-arm": "ac4ac5319ded88ba5d885747a6b2641e7f712c9797de0770465104abe4dbf7da",
    "android-arm64": "0495e57163429937caaee1bdd9b07924a7098dbd51d1b97645d124e349f097b4",
    "android-x64": "bd68707f29f2316c20f9d5c793b8731796a4a65408ac4d3c30bc606390354e41",
    "android-x86": "9d4f7edf5b46e1ab155d9da3e56520f2554900997c74e21951f524ea7928c622",
    "ios-device-arm64": "a9ab2295c65005d6fe81b11a172645160a5194ae55d1ffdf8444dd27124aad23",
    "ios-simulator-arm64": "b2b5f0c918f661b9fe8de7dba3d3fee7d7df61db3247cfb0f3f910077af54f1b",
    "linux-x64": "e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659",
    "win-x64": "9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b",
}

MAX_DOWNLOAD_BYTES = 64 * 1024 * 1024
KEEP_FILES = ("LICENSE", "VERSION", "args.gn")
KEEP_DIRS = ("lib/", "bin/", "licenses/")
REQUIRED_ARGS = ("pdf_enable_v8 = false", "pdf_enable_xfa = false")


def download(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=120) as response:  # noqa: S310 - pinned https URL
        data = response.read(MAX_DOWNLOAD_BYTES + 1)
    if len(data) > MAX_DOWNLOAD_BYTES:
        raise SystemExit(f"download exceeds {MAX_DOWNLOAD_BYTES} bytes")
    return data


def safe_members(archive: tarfile.TarFile) -> list[tarfile.TarInfo]:
    members = []
    for member in archive.getmembers():
        name = member.name
        if name.startswith(("/", "\\")) or ".." in pathlib.PurePosixPath(name).parts:
            raise SystemExit(f"unsafe path in archive: {name!r}")
        if not member.isfile():
            continue
        if name in KEEP_FILES or name.startswith(KEEP_DIRS):
            members.append(member)
    return members


def fetch(platform: str, dest: pathlib.Path) -> None:
    digest = ASSETS[platform]
    url = f"{SOURCE}/{RELEASE}/pdfium-{platform}.tgz"
    data = download(url)
    actual = hashlib.sha256(data).hexdigest()
    if actual != digest:
        raise SystemExit(f"digest mismatch for {url}: {actual}")

    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        members = safe_members(archive)
        args = archive.extractfile("args.gn")
        settings = args.read().decode("utf-8") if args else ""
        missing = [line for line in REQUIRED_ARGS if line not in settings]
        if missing:
            raise SystemExit(f"refusing a PDFium build without {missing} (PRODUCT_INVARIANTS.md 6)")
        dest.mkdir(parents=True, exist_ok=True)
        archive.extractall(dest, members=members, filter="data")
    print(f"PDFium {RELEASE} for {platform} verified and extracted to {dest}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Fetch a pinned, verified PDFium build.")
    parser.add_argument("platform", choices=sorted(ASSETS))
    parser.add_argument("--dest", type=pathlib.Path)
    arguments = parser.parse_args(argv)
    fetch(arguments.platform, arguments.dest or ROOT / "third_party" / "pdfium" / arguments.platform)
    return 0


if __name__ == "__main__":
    sys.exit(main())
