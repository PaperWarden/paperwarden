from __future__ import annotations

import hashlib
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
CORPUS = ROOT / "tests" / "corpus"


def main() -> int:
    manifest = json.loads((CORPUS / "manifest.json").read_text(encoding="utf-8"))
    errors: list[str] = []
    seen: set[str] = set()
    for fixture in manifest["fixtures"]:
        relative = pathlib.PurePosixPath(fixture["path"])
        if relative.is_absolute() or ".." in relative.parts:
            errors.append(f"unsafe fixture path: {relative}")
            continue
        if str(relative) in seen:
            errors.append(f"duplicate fixture path: {relative}")
            continue
        seen.add(str(relative))
        path = CORPUS.joinpath(*relative.parts)
        if not path.is_file():
            errors.append(f"missing fixture: {relative}")
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest != fixture["sha256"]:
            errors.append(f"hash mismatch: {relative}")
        for key in ("license", "provenance", "purpose", "expected"):
            if not fixture.get(key):
                errors.append(f"{relative} is missing {key}")

    tracked = {
        path.relative_to(CORPUS).as_posix()
        for path in CORPUS.rglob("*")
        if path.is_file() and path.name not in {"README.md", "manifest.json"}
    }
    for untracked in sorted(tracked - seen):
        errors.append(f"fixture is not in manifest: {untracked}")

    if errors:
        print("Corpus check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"Corpus check passed for {len(seen)} reviewed fixtures.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
