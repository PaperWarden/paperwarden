"""Check that the iOS privacy manifest declares every required-reason API the
app's binaries use (Apple's "Describing use of required reason API").

Apple requires a reason for each category of these APIs an app uses. This
script reads the symbols each binary imports and fails if it finds a category
that `apps/mobile/ios/Runner/PrivacyInfo.xcprivacy` does not declare. It also
fails if the manifest claims tracking, tracking domains or collected data,
which PaperWarden never has.

Binaries are checked from `nm -u` output, so the core of the check runs and is
tested anywhere. On macOS, `--app` lists a built `.app`'s binaries and reads
their symbols itself. Frameworks that ship their own `PrivacyInfo.xcprivacy`,
such as Flutter's, answer for themselves and are skipped.

Usage:
    python scripts/check_privacy_manifest.py --manifest M --app Runner.app [--binary B]...
    python scripts/check_privacy_manifest.py --manifest M --symbols nm-output.txt...
"""

from __future__ import annotations

import argparse
import pathlib
import plistlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "apps" / "mobile" / "ios" / "Runner" / "PrivacyInfo.xcprivacy"

# Imported C symbols for each category, from Apple's list. Apple names some
# APIs by Objective-C selector or class; the class symbols are matched, and
# selectors are searched for in the binary's strings by `selectors_in`.
CATEGORIES: dict[str, set[str]] = {
    "NSPrivacyAccessedAPICategoryFileTimestamp": {
        "stat", "fstat", "lstat", "fstatat", "getattrlist", "fgetattrlist", "getattrlistat", "getattrlistbulk",
    },
    "NSPrivacyAccessedAPICategorySystemBootTime": {"mach_absolute_time"},
    "NSPrivacyAccessedAPICategoryDiskSpace": {
        "statfs", "fstatfs", "statvfs", "fstatvfs", "getattrlist", "fgetattrlist", "getattrlistat",
    },
    "NSPrivacyAccessedAPICategoryUserDefaults": {"OBJC_CLASS_$_NSUserDefaults"},
}
SELECTORS: dict[str, set[bytes]] = {
    "NSPrivacyAccessedAPICategorySystemBootTime": {b"systemUptime"},
    "NSPrivacyAccessedAPICategoryActiveKeyboards": {b"activeInputModes"},
    "NSPrivacyAccessedAPICategoryDiskSpace": {
        b"volumeAvailableCapacityKey", b"volumeAvailableCapacityForImportantUsageKey",
        b"volumeAvailableCapacityForOpportunisticUsageKey", b"volumeTotalCapacityKey",
        b"systemFreeSize", b"systemSize",
    },
}

# Reasons Apple documents for each category; anything else is a typo.
REASONS: dict[str, set[str]] = {
    "NSPrivacyAccessedAPICategoryFileTimestamp": {"DDA9.1", "C617.1", "3B52.1", "0A2A.1"},
    "NSPrivacyAccessedAPICategorySystemBootTime": {"35F9.1", "8FFB.1", "3D61.1"},
    "NSPrivacyAccessedAPICategoryDiskSpace": {"85F4.1", "E174.1", "7D9E.1", "B728.1"},
    "NSPrivacyAccessedAPICategoryActiveKeyboards": {"3EC4.1", "54BD.1"},
    "NSPrivacyAccessedAPICategoryUserDefaults": {"CA92.1", "1C8F.1", "C56D.1", "AC6B.1"},
}


def symbol_names(nm_output: str) -> set[str]:
    """Undefined symbols from `nm -u` output, without the leading underscore
    and Darwin's `$INODE64`-style suffixes."""
    names = set()
    for line in nm_output.splitlines():
        token = line.strip().split()[-1] if line.strip() else ""
        if not token or token.endswith(":"):
            continue
        token = token.removeprefix("_")
        names.add(re.sub(r"\$(INODE64|UNIX2003|DARWIN_EXTSN|NOCANCEL)$", "", token))
    return names


def categories_used(symbols: set[str], binary: bytes = b"") -> dict[str, set[str]]:
    """Each category the symbols (or selector strings) use, with the evidence."""
    used: dict[str, set[str]] = {}
    for category, names in CATEGORIES.items():
        found = symbols & names
        if found:
            used.setdefault(category, set()).update(found)
    for category, selectors in SELECTORS.items():
        found = {selector.decode() for selector in selectors if selector in binary}
        if found:
            used.setdefault(category, set()).update(found)
    return used


def manifest_problems(manifest: dict) -> list[str]:
    problems = []
    if manifest.get("NSPrivacyTracking") is not False:
        problems.append("NSPrivacyTracking must be false")
    if manifest.get("NSPrivacyTrackingDomains") != []:
        problems.append("NSPrivacyTrackingDomains must be empty")
    if manifest.get("NSPrivacyCollectedDataTypes") != []:
        problems.append("NSPrivacyCollectedDataTypes must be empty")
    for entry in manifest.get("NSPrivacyAccessedAPITypes", []):
        category = entry.get("NSPrivacyAccessedAPIType")
        reasons = entry.get("NSPrivacyAccessedAPITypeReasons") or []
        if category not in REASONS:
            problems.append(f"unknown category {category!r}")
        elif not reasons or any(reason not in REASONS[category] for reason in reasons):
            problems.append(f"{category} needs documented reasons, has {reasons}")
    return problems


def declared(manifest: dict) -> set[str]:
    return {entry.get("NSPrivacyAccessedAPIType") for entry in manifest.get("NSPrivacyAccessedAPITypes", [])}


def check(manifest: dict, binaries: dict[str, tuple[set[str], bytes]]) -> list[str]:
    problems = manifest_problems(manifest)
    allowed = declared(manifest)
    for name, (symbols, data) in sorted(binaries.items()):
        for category, evidence in sorted(categories_used(symbols, data).items()):
            if category not in allowed:
                problems.append(f"{name} uses {category} ({', '.join(sorted(evidence))}) but the manifest does not declare it")
    return problems


def app_binaries(app: pathlib.Path) -> list[pathlib.Path]:
    """The app's executable and every embedded framework without its own manifest."""
    binaries = [app / app.stem]
    for framework in sorted((app / "Frameworks").glob("*.framework")):
        if not (framework / "PrivacyInfo.xcprivacy").exists():
            binaries.append(framework / framework.stem)
    return [binary for binary in binaries if binary.is_file()]


def read_binary(path: pathlib.Path) -> tuple[set[str], bytes]:
    output = subprocess.run(["nm", "-u", str(path)], check=True, capture_output=True, text=True).stdout
    return symbol_names(output), path.read_bytes()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    parser.add_argument("--manifest", type=pathlib.Path, default=MANIFEST)
    parser.add_argument("--app", type=pathlib.Path, help="a built .app (macOS only)")
    parser.add_argument("--binary", type=pathlib.Path, action="append", default=[], help="another Mach-O binary")
    parser.add_argument("--symbols", type=pathlib.Path, action="append", default=[], help="saved `nm -u` output")
    arguments = parser.parse_args(argv)

    with arguments.manifest.open("rb") as handle:
        manifest = plistlib.load(handle)
    binaries: dict[str, tuple[set[str], bytes]] = {}
    paths = list(arguments.binary)
    if arguments.app:
        if not (arguments.app / "PrivacyInfo.xcprivacy").is_file():
            print(f"{arguments.app} does not contain PrivacyInfo.xcprivacy", file=sys.stderr)
            return 1
        paths = app_binaries(arguments.app) + paths
    for path in paths:
        binaries[path.name] = read_binary(path)
    for path in arguments.symbols:
        binaries[path.name] = (symbol_names(path.read_text(encoding="utf-8")), b"")
    if not binaries:
        print("no binaries to check", file=sys.stderr)
        return 1

    problems = check(manifest, binaries)
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        return 1
    for name, (symbols, data) in sorted(binaries.items()):
        used = categories_used(symbols, data)
        summary = ", ".join(f"{c.removeprefix('NSPrivacyAccessedAPICategory')} ({', '.join(sorted(e))})" for c, e in sorted(used.items()))
        print(f"{name}: {summary or 'no required-reason APIs'}")
    print(f"Privacy manifest covers {len(binaries)} binaries.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
