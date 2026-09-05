from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

REQUIRED = {
    "ARCHITECTURE.md",
    "PRIVACY.md",
    "PRODUCT_INVARIANTS.md",
    "SUPPORTED_FORMATS.md",
    "THREAT_MODEL.md",
    "THIRD_PARTY_LICENSES.md",
    "VISION.md",
}

FORBIDDEN_DEPENDENCY_MARKERS = {
    "firebase_analytics",
    "google_mobile_ads",
    "appsflyer",
    "mixpanel",
    "amplitude_flutter",
}


def text_files() -> list[pathlib.Path]:
    ignored = {
        ".dart_tool",
        ".git",
        ".gradle",
        "DerivedData",
        "Pods",
        "build",
        "target",
    }
    return [
        path
        for path in ROOT.rglob("*")
        if path.is_file() and not ignored.intersection(path.parts)
    ]


def main() -> int:
    errors: list[str] = []
    for required in sorted(REQUIRED):
        if not (ROOT / required).is_file():
            errors.append(f"missing required policy: {required}")

    manifest = ROOT / "apps" / "mobile" / "android" / "app" / "src" / "main" / "AndroidManifest.xml"
    if manifest.is_file() and "android.permission.INTERNET" in manifest.read_text(encoding="utf-8"):
        errors.append("Android INTERNET permission violates PRODUCT_INVARIANTS.md")

    for path in text_files():
        try:
            content = path.read_text(encoding="utf-8").lower()
        except (OSError, UnicodeDecodeError):
            continue
        if path.name in {"PRODUCT_INVARIANTS.md", "check_invariants.py"}:
            continue
        for marker in FORBIDDEN_DEPENDENCY_MARKERS:
            if re.search(rf"(^|[^a-z0-9_]){re.escape(marker)}([^a-z0-9_]|$)", content):
                errors.append(f"forbidden telemetry/ad dependency marker {marker!r} in {path.relative_to(ROOT)}")

    if errors:
        print("Product invariant check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Product invariant check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
