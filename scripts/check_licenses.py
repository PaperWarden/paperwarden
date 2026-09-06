from __future__ import annotations

import json
import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "third_party" / "dependencies.json"
ALLOWED_LICENSES = {
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "MIT",
    "MPL-2.0",
    "Unicode-3.0",
    "Zlib",
}


def flutter_lock_packages() -> dict[str, str]:
    text = (ROOT / "apps" / "mobile" / "pubspec.lock").read_text(encoding="utf-8")
    packages: dict[str, str] = {}
    current: str | None = None
    for line in text.splitlines():
        match = re.fullmatch(r"  ([A-Za-z0-9_]+):", line)
        if match:
            current = match.group(1)
            continue
        version = re.fullmatch(r'    version: "([^"]+)"', line)
        if current and version:
            packages[current] = version.group(1)
            current = None
    return packages


def cargo_lock_packages() -> dict[str, str]:
    with (ROOT / "core" / "Cargo.lock").open("rb") as lock_file:
        lock = tomllib.load(lock_file)
    return {
        package["name"]: package["version"]
        for package in lock.get("package", [])
        if package["name"] != "paperwarden-core"
    }


def main() -> int:
    entries = json.loads(INVENTORY.read_text(encoding="utf-8"))
    indexed = {(entry["ecosystem"], entry["name"]): entry for entry in entries}
    expected = {
        **{("dart", name): version for name, version in flutter_lock_packages().items()},
        **{("cargo", name): version for name, version in cargo_lock_packages().items()},
    }
    errors: list[str] = []
    for key, version in sorted(expected.items()):
        entry = indexed.get(key)
        if not entry:
            errors.append(f"missing inventory entry for {key[0]}:{key[1]} {version}")
            continue
        if entry["version"] != version:
            errors.append(
                f"version mismatch for {key[0]}:{key[1]}: "
                f"lock={version}, inventory={entry['version']}"
            )
        if entry["license"] not in ALLOWED_LICENSES:
            errors.append(f"unapproved license for {key[0]}:{key[1]}: {entry['license']}")
        if not entry.get("source") or not entry.get("scope"):
            errors.append(f"incomplete inventory entry for {key[0]}:{key[1]}")

    stale = sorted(set(indexed) - set(expected))
    for ecosystem, name in stale:
        errors.append(f"stale inventory entry for {ecosystem}:{name}")

    if errors:
        print("License inventory check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"License inventory check passed for {len(expected)} locked packages.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
