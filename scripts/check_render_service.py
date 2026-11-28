"""Check that a built Android app keeps PDFium in its isolated service (#59).

PDFium must run only in RenderService, declared with android:isolatedProcess
in its own process, not exported, and the release app must request no
permissions beyond its own signature permissions. The merged manifest is what ships, so this reads it
rather than the source manifest (libraries can add entries when merging).

Optionally, --apk checks that every ABI carries both PaperWarden's PDFium
(libpaperwarden_pdfium.so) and the Rust bridge that loads it.

Usage:
    python scripts/check_render_service.py --manifest MERGED_MANIFEST
        [--apk APP.apk --abi arm64-v8a --abi x86_64 ...]
"""

from __future__ import annotations

import argparse
import pathlib
import sys
import xml.etree.ElementTree as ET
import zipfile

ANDROID = "{http://schemas.android.com/apk/res/android}"
SERVICE = "dev.paperwarden.paperwarden.RenderService"
PROCESS = ":pdfium"
LIBRARIES = ("libpaperwarden_pdfium.so", "libpaperwarden_bridge.so")
PERMISSION_TAGS = (
    "uses-permission",
    "uses-permission-sdk-23",
    "uses-permission-sdk-m",
)


def _name(element: ET.Element, package: str) -> str:
    name = element.get(f"{ANDROID}name", "")
    return package + name if name.startswith(".") else name


def check_manifest(text: str) -> list[str]:
    """Every problem with the merged manifest `text`."""
    try:
        root = ET.fromstring(text)
    except ET.ParseError as error:
        return [f"manifest is not XML: {error}"]
    package = root.get("package", "dev.paperwarden.paperwarden")
    # The app's own signature permissions grant nothing outside the app
    # (AndroidX core declares DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION this
    # way); any other requested permission fails.
    own = {
        _name(element, package)
        for element in root.iter("permission")
        if element.get(f"{ANDROID}protectionLevel") == "signature"
        and _name(element, package).startswith(package + ".")
    }
    errors = []
    for tag in PERMISSION_TAGS:
        for element in root.iter(tag):
            name = _name(element, package)
            if name not in own:
                errors.append(f"release app requests {name}")

    services = [
        element
        for element in root.iter("service")
        if _name(element, package) == SERVICE
    ]
    if len(services) != 1:
        return errors + [f"expected one {SERVICE}, found {len(services)}"]
    service = services[0]
    if service.get(f"{ANDROID}isolatedProcess") != "true":
        errors.append("RenderService is not an isolatedProcess")
    if service.get(f"{ANDROID}exported") != "false":
        errors.append("RenderService must not be exported")
    if service.get(f"{ANDROID}process") != PROCESS:
        errors.append(f"RenderService must run in its own process ({PROCESS})")
    if service.find("intent-filter") is not None:
        errors.append("RenderService must have no intent filter")

    for element in root.iter():
        if element is service or element.tag not in ("activity", "service", "receiver", "provider"):
            continue
        if element.get(f"{ANDROID}process") == PROCESS:
            errors.append(f"{_name(element, package)} shares the PDFium process")
    return errors


def check_apk(apk: pathlib.Path, abis: list[str]) -> list[str]:
    """Every library missing from `apk` for `abis`."""
    with zipfile.ZipFile(apk) as archive:
        names = set(archive.namelist())
    errors = [
        f"{library} missing for {abi}"
        for abi in abis
        for library in LIBRARIES
        if f"lib/{abi}/{library}" not in names
    ]
    # Shipped as libpdfium.so, a load by name could pick the system's copy.
    errors += [
        f"{name} must ship as libpaperwarden_pdfium.so"
        for name in sorted(names)
        if name.startswith("lib/") and name.endswith("/libpdfium.so")
    ]
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--manifest", type=pathlib.Path, required=True)
    parser.add_argument("--apk", type=pathlib.Path)
    parser.add_argument("--abi", action="append", default=[])
    args = parser.parse_args(argv)

    errors = check_manifest(args.manifest.read_text(encoding="utf-8"))
    if args.apk:
        errors += check_apk(args.apk, args.abi)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        return 1
    checked = f" PDFium and the bridge present for {', '.join(args.abi)}." if args.apk else ""
    print(f"RenderService is isolated, unexported and alone in {PROCESS}; "
          f"no permissions requested beyond the app's own.{checked}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
