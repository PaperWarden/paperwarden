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



def license_allowed(expression: str) -> bool:
    """Return whether every term of an inventory license expression is allowed.

    Record the license PaperWarden elects: for "MIT OR Apache-2.0" write one
    allowed choice. A package whose terms all apply ("A AND B") is recorded
    with AND, and every term must be allowed. Parentheses and OR are rejected,
    so an election is always explicit.
    """
    terms = expression.split(" AND ")
    return all(term in ALLOWED_LICENSES for term in terms)


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


def cargo_lock_packages() -> set[tuple[str, str]]:
    """Return (name, version) for every registry crate; a crate may lock twice."""
    with (ROOT / "core" / "Cargo.lock").open("rb") as lock_file:
        lock = tomllib.load(lock_file)
    return {
        (package["name"], package["version"])
        for package in lock.get("package", [])
        # Local workspace crates (no registry source) are PaperWarden's own code.
        if "source" in package
    }


def main() -> int:
    entries = json.loads(INVENTORY.read_text(encoding="utf-8"))
    errors: list[str] = []
    indexed: dict[tuple[str, str, str], dict] = {}
    for entry in entries:
        key = (entry["ecosystem"], entry["name"], entry["version"])
        if key in indexed:
            errors.append(f"duplicate inventory entry for {key[0]}:{key[1]} {key[2]}")
        indexed[key] = entry
    expected = {
        *{("dart", name, version) for name, version in flutter_lock_packages().items()},
        *{("cargo", name, version) for name, version in cargo_lock_packages()},
    }

    for key in sorted(expected):
        ecosystem, name, version = key
        entry = indexed.get(key)
        if not entry:
            others = sorted(v for e, n, v in indexed if (e, n) == (ecosystem, name))
            detail = f" (inventory has {', '.join(others)})" if others else ""
            errors.append(f"missing inventory entry for {ecosystem}:{name} {version}{detail}")
            continue
        if not license_allowed(entry["license"]):
            errors.append(f"unapproved license for {ecosystem}:{name} {version}: {entry['license']}")
        if not entry.get("source") or not entry.get("scope"):
            errors.append(f"incomplete inventory entry for {ecosystem}:{name} {version}")

    for ecosystem, name, version in sorted(set(indexed) - expected):
        errors.append(f"stale inventory entry for {ecosystem}:{name} {version}")

    if errors:
        print("License inventory check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"License inventory check passed for {len(expected)} locked packages.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
