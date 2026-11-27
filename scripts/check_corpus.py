"""Check tests/corpus against its manifest (schema 2).

docs/ci/corpus.md describes the schema, the outcome vocabulary, what this check
proves and what it leaves to engine runners.

For every fixture it checks: a safe, portable path in the right category
directory; the hash and size; a redistributable license; provenance, purpose
and expected behaviour; a known format, outcome and capability; references to
other fixtures; and every structural fact, computed independently in
corpus_facts.py. Files not in the manifest fail the check.

No engine exists yet, so every fixture's engine outcome is reported as pending.
A pending fixture is never evidence that a format is supported
(PRODUCT_INVARIANTS.md 12).

Usage:
    python scripts/check_corpus.py [--report PATH]
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import corpus_facts  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[1]
CORPUS = ROOT / "tests" / "corpus"
SCHEMA = 2

CATEGORIES = ("valid", "malformed", "security", "conversion", "ocr")
FORMATS = ("pdf", "office", "archive", "image", "text", "html", "svg")
OUTCOMES = {
    "accept": "Open or process it and produce what `expected` describes",
    "reject": "Refuse it as invalid, produce no output and clean up",
    "neutralize": "Never perform the dangerous effect (execute, fetch, escape, overwrite); flag it",
    "bound": "Stop at a resource or nesting bound before exhausting the device",
    "reference": "Ground truth for another fixture, not an input",
}
CAPABILITIES = ("pdf", "office", "archive", "image", "text", "html", "svg", "ocr", "conversion")

# Licenses that allow redistribution in this repository.
LICENSES = ("CC0-1.0", "CC-BY-4.0", "MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause")

MAX_FIXTURE_BYTES = 1024 * 1024
MAX_CORPUS_BYTES = 8 * 1024 * 1024

REQUIRED_KEYS = {
    "path", "sha256", "bytes", "license", "provenance", "purpose", "expected",
    "category", "format", "media_type", "outcome", "requires", "facts",
}
OPTIONAL_KEYS = {"references"}
UNSAFE_NAME = re.compile(r"[\x00-\x1f\x7f\\:\u200e\u200f\u061c\u202a-\u202e\u2066-\u2069]")
NOT_FIXTURES = {"README.md", "manifest.json"}


@dataclasses.dataclass
class Result:
    errors: list[str] = dataclasses.field(default_factory=list)
    report: list[dict[str, object]] = dataclasses.field(default_factory=list)
    facts: int = 0
    total_bytes: int = 0


def check_entry(corpus: pathlib.Path, entry: dict, result: Result) -> None:
    raw_path = entry.get("path", "")
    label = raw_path or "<entry without path>"
    keys = set(entry)
    for missing in sorted(REQUIRED_KEYS - keys):
        result.errors.append(f"{label} is missing {missing}")
    for unknown in sorted(keys - REQUIRED_KEYS - OPTIONAL_KEYS):
        result.errors.append(f"{label} has unknown field {unknown}")

    relative = pathlib.PurePosixPath(raw_path)
    if (not raw_path or relative.is_absolute() or ".." in relative.parts
            or UNSAFE_NAME.search(raw_path)):
        result.errors.append(f"unsafe fixture path: {label!r}")
        return
    category = entry.get("category")
    if category not in CATEGORIES:
        result.errors.append(f"{label} has unknown category {category!r}")
    elif relative.parts[0] != category:
        result.errors.append(f"{label} is not under its category directory {category}/")
    elif (category in ("valid", "malformed", "security") and len(relative.parts) > 2
            and relative.parts[1] != entry.get("format")):
        result.errors.append(f"{label} is not in its format directory {entry.get('format')}/")
    if entry.get("format") not in FORMATS:
        result.errors.append(f"{label} has unknown format {entry.get('format')!r}")
    outcome = entry.get("outcome")
    if outcome not in OUTCOMES:
        result.errors.append(f"{label} has unknown outcome {outcome!r}")
    requires = entry.get("requires")
    if not isinstance(requires, list) or any(item not in CAPABILITIES for item in requires):
        result.errors.append(f"{label} requires unknown capabilities {requires!r}")
    elif (outcome == "reference") != (requires == []):
        result.errors.append(f"{label}: reference fixtures require nothing; inputs require a capability")
    if entry.get("license") not in LICENSES:
        result.errors.append(f"{label} has a license not cleared for redistribution: {entry.get('license')!r}")
    for key in ("provenance", "purpose", "expected", "media_type"):
        if key in entry and not str(entry[key]).strip():
            result.errors.append(f"{label} has an empty {key}")

    path = corpus.joinpath(*relative.parts)
    if not path.is_file():
        result.errors.append(f"missing fixture: {label}")
        return
    data = path.read_bytes()
    result.total_bytes += len(data)
    if entry.get("bytes") != len(data):
        result.errors.append(f"size mismatch: {label} is {len(data)} bytes")
    if len(data) > MAX_FIXTURE_BYTES:
        result.errors.append(f"{label} exceeds the {MAX_FIXTURE_BYTES}-byte fixture limit")
    if hashlib.sha256(data).hexdigest() != entry.get("sha256"):
        result.errors.append(f"hash mismatch: {label}")

    facts = entry.get("facts")
    verified = 0
    if not isinstance(facts, dict):
        result.errors.append(f"{label} facts must be an object")
    else:
        for kind, expected in facts.items():
            problem = corpus_facts.verify(kind, expected, data)
            if problem:
                result.errors.append(f"fact {kind} does not hold for {label}: {problem}")
            else:
                verified += 1
    result.facts += verified
    result.report.append({
        "path": raw_path,
        "outcome": outcome,
        "requires": requires,
        "integrity": "verified",
        "facts_verified": verified,
        "engine": "not applicable" if outcome == "reference" else "pending: no engine implements "
        + ", ".join(requires or []),
    })


def check_corpus(corpus: pathlib.Path) -> Result:
    result = Result()
    try:
        manifest = json.loads((corpus / "manifest.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        result.errors.append(f"cannot read manifest: {error}")
        return result
    if manifest.get("schema") != SCHEMA:
        result.errors.append(f"manifest schema must be {SCHEMA}")
    entries = manifest.get("fixtures", [])

    seen: dict[str, dict] = {}
    for entry in entries:
        path = entry.get("path", "")
        if path in seen:
            result.errors.append(f"duplicate fixture path: {path}")
            continue
        seen[path] = entry
        check_entry(corpus, entry, result)

    for path, entry in seen.items():
        for reference in entry.get("references", []):
            target = seen.get(reference)
            if target is None:
                result.errors.append(f"{path} references a fixture not in the manifest: {reference}")
            elif target.get("outcome") != "reference":
                result.errors.append(f"{path} references {reference}, which is not a reference fixture")

    if result.total_bytes > MAX_CORPUS_BYTES:
        result.errors.append(f"corpus is {result.total_bytes} bytes; the budget is {MAX_CORPUS_BYTES}")

    on_disk = {
        path.relative_to(corpus).as_posix()
        for path in corpus.rglob("*")
        if path.is_file() and path.name not in NOT_FIXTURES
    }
    for untracked in sorted(on_disk - set(seen)):
        result.errors.append(f"fixture is not in manifest: {untracked}")
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check the corpus against its manifest.")
    parser.add_argument("--report", type=pathlib.Path, help="write a per-fixture JSON report here")
    parser.add_argument("--corpus", type=pathlib.Path, default=CORPUS, help=argparse.SUPPRESS)
    arguments = parser.parse_args(argv)

    result = check_corpus(arguments.corpus)
    if arguments.report:
        arguments.report.write_text(
            json.dumps({"schema": SCHEMA, "errors": result.errors, "fixtures": result.report}, indent=2) + "\n",
            encoding="utf-8",
        )
    if result.errors:
        print("Corpus check failed:", file=sys.stderr)
        for error in result.errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    inputs = sum(1 for row in result.report if row["outcome"] != "reference")
    print(
        f"Corpus check passed for {len(result.report)} reviewed fixtures "
        f"({result.total_bytes} bytes): hashes, licenses and {result.facts} structural facts verified. "
        f"Engine outcomes pending for {inputs} inputs; no engine exists yet."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
