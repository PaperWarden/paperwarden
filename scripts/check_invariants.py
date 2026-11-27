"""Check repository state that PRODUCT_INVARIANTS.md depends on.

What this can and cannot prove is stated in docs/ci/invariant-checks.md. In
short: it inspects files in the repository (tracked files when run inside a
git checkout). It cannot see dependencies or permissions that only appear at
build time, so CI also runs it in --android-merged-manifest mode against the
manifest Gradle actually merges for release builds.

Usage:
    python scripts/check_invariants.py
    python scripts/check_invariants.py --android-merged-manifest PATH
"""

from __future__ import annotations

import argparse
import dataclasses
import fnmatch
import json
import pathlib
import re
import subprocess
import sys
import tomllib
import xml.etree.ElementTree as ElementTree

ROOT = pathlib.Path(__file__).resolve().parents[1]

# --- Required policies ------------------------------------------------------

REQUIRED_POLICIES = (
    "ARCHITECTURE.md",
    "PRIVACY.md",
    "PRODUCT_INVARIANTS.md",
    "SECURITY.md",
    "SUPPORTED_FORMATS.md",
    "THREAT_MODEL.md",
    "THIRD_PARTY_LICENSES.md",
    "VISION.md",
)

MINIMUM_INVARIANTS = 14

# Statements the CI checks below enforce. Rewording one needs an ADR
# (PRODUCT_INVARIANTS.md) and an update here in the same change.
INVARIANT_ANCHORS = (
    "android.permission.INTERNET",
    "analytics, or telemetry",
    "never executed",
    "never fetched automatically",
    "Security checks are fixed, not removed or weakened",
)

# Workflows that run a release-blocking gate, and the commands they must keep.
REQUIRED_WORKFLOW_COMMANDS = {
    "invariants.yml": (
        "python scripts/check_invariants.py",
        "python -m unittest discover -s tests/scripts",
    ),
    "licenses.yml": ("python scripts/check_licenses.py",),
    "corpus.yml": ("python scripts/check_corpus.py",),
    "security.yml": ("cargo audit", "cargo-deny-action"),
    "dependency-review.yml": ("dependency-review-action",),
}

ACTION_REFERENCE = re.compile(r"uses:\s*[^@\s]+@([^\s#]+)")

# --- Android permissions ----------------------------------------------------

PRODUCTION_MANIFEST = "apps/mobile/android/app/src/main/AndroidManifest.xml"

# Source sets that never reach a release build. Flutter needs INTERNET there
# for hot reload and the Dart VM service.
DEVELOPMENT_SOURCE_SETS = ("/src/debug/", "/src/profile/")

ANDROID_NAMESPACE = "{http://schemas.android.com/apk/res/android}"

# Every permission a release build may request. Adding one is a reviewed
# change to this mapping, with the reason as the value.
ALLOWED_ANDROID_PERMISSIONS: dict[str, str] = {}

# Permissions that would break invariant 5 or open a network or tracking path.
FORBIDDEN_ANDROID_PERMISSIONS = {
    "android.permission.INTERNET",
    "android.permission.ACCESS_NETWORK_STATE",
    "android.permission.ACCESS_WIFI_STATE",
    "android.permission.CHANGE_NETWORK_STATE",
    "android.permission.CHANGE_WIFI_STATE",
    "com.google.android.gms.permission.AD_ID",
}

# --- Network capability -----------------------------------------------------

IOS_NETWORK_ENTITLEMENT = re.compile(
    r"com\.apple\.developer\.networking|com\.apple\.security\.network"
)

JVM_NETWORK_API = re.compile(
    r"\b(?:HttpURLConnection|HttpsURLConnection|URLConnection|OkHttpClient)\b"
    r"|\bjava\.net\.(?:Socket|URL)\b|\bokhttp3\b|\bretrofit2\b"
)

# (directory prefix, file suffix, pattern) for shipped application code.
NETWORK_APIS = (
    (
        "apps/mobile/lib/",
        ".dart",
        re.compile(
            r"\b(?:HttpClient|HttpServer|RawSocket|SecureSocket|RawSecureSocket"
            r"|RawDatagramSocket|WebSocket|NetworkImage)\b"
            r"|\bSocket\s*\.\s*(?:connect|startConnect)"
            r"|\bImage\s*\.\s*network\b"
            r"|package:(?:http|dio|web_socket_channel|grpc|socket_io_client"
            r"|cached_network_image)/"
        ),
    ),
    (
        "core/",
        ".rs",
        re.compile(
            r"\bstd\s*::\s*net\b|\btokio\s*::\s*net\b|\basync_std\s*::\s*net\b"
            r"|\b(?:reqwest|hyper|ureq|isahc|surf|attohttpc|curl)\s*::"
        ),
    ),
    ("apps/mobile/android/", ".kt", JVM_NETWORK_API),
    ("apps/mobile/android/", ".java", JVM_NETWORK_API),
    (
        "apps/mobile/ios/",
        ".swift",
        re.compile(r"\b(?:URLSession|NWConnection|NWBrowser|CFNetwork|Alamofire)\b"),
    ),
    (
        "apps/mobile/ios/",
        ".m",
        re.compile(r"\b(?:NSURLSession|NSURLConnection|CFStream)\w*"),
    ),
)

# Test and build-tool code is not shipped and may mention these APIs.
NETWORK_SCAN_EXCLUDED = ("/test/", "/tests/", "/integration_test/", "/benches/", "/build_tool/")

# --- Dependencies -----------------------------------------------------------

# Lockfiles whose packages ship in the app; check_licenses.py inventories them.
SHIPPED_LOCKFILES = ("apps/mobile/pubspec.lock", "core/Cargo.lock")

# Lockfiles for code that never ships, with the reason.
NOT_SHIPPED_LOCKFILES = {
    "fuzz/Cargo.lock": "cargo-fuzz harnesses; built only for fuzzing",
}

LICENSE_INVENTORY = "third_party/dependencies.json"

# Exact names and prefixes (ending in "*") of packages that would add ads,
# analytics, telemetry, crash reporting, purchases, cloud AI, a web view or a
# network client. Matched against every package in every lockfile, including
# development dependencies, because a lockfile does not record which
# dependencies reach a release build.
FORBIDDEN_DART_PACKAGES = (
    "firebase_*", "cloud_firestore", "google_mobile_ads", "gma_mediation_*",
    "sentry*", "appsflyer_sdk", "adjust_sdk", "mixpanel_flutter", "amplitude_flutter",
    "posthog_flutter", "segment_analytics*", "datadog_*", "bugsnag_*", "newrelic_mobile",
    "instabug_flutter", "facebook_app_events", "flutter_branch_sdk", "onesignal_flutter",
    "in_app_purchase*", "purchases_flutter", "unity_ads_plugin", "applovin_max",
    "google_generative_ai", "openai_dart", "dart_openai", "anthropic_sdk_dart",
    "langchain*", "supabase*", "appwrite", "http", "dio", "chopper", "retrofit",
    "web_socket_channel", "grpc", "graphql*", "socket_io_client",
    "cached_network_image", "flutter_cache_manager", "webview_flutter*",
    "flutter_inappwebview*",
)

FORBIDDEN_RUST_CRATES = (
    "reqwest", "hyper", "hyper-*", "ureq", "isahc", "surf", "attohttpc", "curl",
    "curl-sys", "tungstenite", "tokio-tungstenite", "h2", "h3", "quinn", "sentry*",
    "opentelemetry*", "posthog*", "segment", "aws-sdk-*", "google-cloud-*",
    "async-openai", "openai*", "anthropic*",
)

GRADLE_AND_POD_FILES = (
    "build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts",
    "Podfile", "Package.swift",
)

FORBIDDEN_NATIVE_DEPENDENCIES = re.compile(
    r"com\.google\.firebase|com\.google\.gms\.google-services"
    r"|com\.google\.android\.gms:play-services-(?:ads|analytics|measurement)"
    r"|com\.android\.billingclient|com\.squareup\.okhttp3|com\.squareup\.retrofit2"
    r"|io\.sentry|com\.crashlytics|com\.appsflyer|com\.mixpanel"
    r"|\b(?:Firebase\w*|GoogleMobileAds|Sentry|Alamofire|AppsFlyerFramework)\b"
)

# Kept from the original check: plain-text markers anywhere in the repository.
FORBIDDEN_DEPENDENCY_MARKERS = (
    "firebase_analytics",
    "google_mobile_ads",
    "appsflyer",
    "mixpanel",
    "amplitude_flutter",
)

MARKER_SCAN_EXCLUDED = {"PRODUCT_INVARIANTS.md", "check_invariants.py"}

# --- Secrets ----------------------------------------------------------------

SECRET_PATTERNS = (
    ("private key", re.compile(r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----")),
    ("AWS access key", re.compile(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b")),
    ("GitHub token", re.compile(r"\bgh[pousr]_[A-Za-z0-9]{36,}\b|\bgithub_pat_[A-Za-z0-9_]{22,}")),
    ("Google API key", re.compile(r"\bAIza[0-9A-Za-z_\-]{35}")),
    ("Slack token", re.compile(r"\bxox[abposr]-[A-Za-z0-9-]{10,}")),
    ("Stripe live key", re.compile(r"\b[rs]k_live_[0-9A-Za-z]{20,}")),
    ("Anthropic API key", re.compile(r"\bsk-ant-[A-Za-z0-9_\-]{20,}")),
    ("OpenAI API key", re.compile(r"\bsk-(?:proj-)?[A-Za-z0-9_\-]{20,}T3BlbkFJ[A-Za-z0-9_\-]{20,}")),
)

# Signing material, credentials and cloud-service configuration, matched on
# the file name, case-insensitive.
FORBIDDEN_FILE_PATTERNS = (
    "*.jks", "*.keystore", "*.p12", "*.pfx", "*.mobileprovision", "*.p8",
    "key.properties", "keystore.properties", "local.properties", ".env", ".env.*", "*.env",
    "google-services.json", "GoogleService-Info.plist", "id_rsa", "id_ed25519", ".netrc",
)

ALLOWED_FILE_NAMES = {".env.example"}

# --- File discovery ---------------------------------------------------------

IGNORED_DIRECTORIES = {
    ".dart_tool", ".git", ".gradle", "DerivedData", "Pods", "build", "target",
}


def repository_files(root: pathlib.Path) -> list[str]:
    """Return repository files as POSIX paths relative to root.

    Inside a git checkout whose top level is root, this is the tracked files,
    which is what CI sees. Otherwise every file outside build directories.
    """
    try:
        top = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        if pathlib.Path(top).resolve() == root.resolve():
            listed = subprocess.run(
                ["git", "-C", str(root), "ls-files", "-z", "--cached"],
                capture_output=True, check=True,
            ).stdout.decode("utf-8")
            return sorted(
                name for name in listed.split("\0")
                if name and (root / name).is_file()
            )
    except (OSError, subprocess.CalledProcessError):
        pass
    return sorted(
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file()
        and not IGNORED_DIRECTORIES.intersection(path.relative_to(root).parts)
    )


def read_text(root: pathlib.Path, relative: str) -> str | None:
    try:
        return (root / relative).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None


@dataclasses.dataclass
class Result:
    errors: list[str] = dataclasses.field(default_factory=list)
    coverage: list[str] = dataclasses.field(default_factory=list)

    def extend(self, other: Result) -> None:
        self.errors.extend(other.errors)
        self.coverage.extend(other.coverage)


def name_matches(name: str, patterns: tuple[str, ...]) -> bool:
    return any(
        name.startswith(pattern[:-1]) if pattern.endswith("*") else name == pattern
        for pattern in patterns
    )


# --- Checks -----------------------------------------------------------------


def check_required_policies(root: pathlib.Path) -> Result:
    result = Result()
    for policy in REQUIRED_POLICIES:
        text = read_text(root, policy)
        if text is None:
            result.errors.append(f"missing required policy: {policy}")
        elif not text.strip():
            result.errors.append(f"required policy is empty: {policy}")

    invariants = read_text(root, "PRODUCT_INVARIANTS.md") or ""
    if invariants.strip():
        numbered = re.findall(r"^\d+\.\s", invariants, flags=re.MULTILINE)
        if len(numbered) < MINIMUM_INVARIANTS:
            result.errors.append(
                f"PRODUCT_INVARIANTS.md lists {len(numbered)} invariants; "
                f"expected at least {MINIMUM_INVARIANTS} numbered invariants"
            )
        flattened = " ".join(invariants.split())
        for anchor in INVARIANT_ANCHORS:
            if anchor not in flattened:
                result.errors.append(f"PRODUCT_INVARIANTS.md no longer states {anchor!r}")

    license_text = " ".join((read_text(root, "LICENSE") or "").split())
    if "Apache License" not in license_text or "Version 2.0" not in license_text:
        result.errors.append("LICENSE is not the Apache License 2.0")

    for file_name, commands in REQUIRED_WORKFLOW_COMMANDS.items():
        relative = f".github/workflows/{file_name}"
        text = read_text(root, relative)
        if text is None:
            result.errors.append(f"missing required workflow: {relative}")
            continue
        for command in commands:
            if command not in text:
                result.errors.append(f"{relative} no longer runs {command!r}")

    workflows = root / ".github" / "workflows"
    workflow_files = sorted(workflows.glob("*.yml")) + sorted(workflows.glob("*.yaml"))
    for workflow in workflow_files:
        content = workflow.read_text(encoding="utf-8")
        for reference in ACTION_REFERENCE.findall(content):
            if not re.fullmatch(r"[0-9a-f]{40}", reference):
                result.errors.append(
                    f"GitHub Action is not pinned to a full commit SHA in "
                    f"{workflow.relative_to(root).as_posix()}: @{reference}"
                )
    result.coverage.append(
        f"policies: {len(REQUIRED_POLICIES)} required documents, "
        f"{len(REQUIRED_WORKFLOW_COMMANDS)} gate workflows, "
        f"{len(workflow_files)} workflows for pinned actions"
    )
    return result


def manifest_errors(text: str, label: str) -> list[str]:
    try:
        manifest = ElementTree.fromstring(text)
    except ElementTree.ParseError as error:
        return [f"cannot parse Android manifest {label}: {error}"]

    declared = {
        element.get(f"{ANDROID_NAMESPACE}name")
        for element in manifest.iter("permission")
    }
    errors = []
    for tag in ("uses-permission", "uses-permission-sdk-23", "uses-permission-sdk-m"):
        for element in manifest.iter(tag):
            name = element.get(f"{ANDROID_NAMESPACE}name") or ""
            if name in FORBIDDEN_ANDROID_PERMISSIONS:
                errors.append(f"{label} requests {name}, which PRODUCT_INVARIANTS.md forbids")
            elif name in declared:
                # A signature permission the app defines for itself, such as
                # AndroidX's DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION.
                continue
            elif name not in ALLOWED_ANDROID_PERMISSIONS:
                errors.append(
                    f"{label} requests a permission not in the reviewed allowlist: {name}"
                )
    for element in manifest.iter("permission"):
        level = element.get(f"{ANDROID_NAMESPACE}protectionLevel", "normal")
        if "signature" not in level:
            name = element.get(f"{ANDROID_NAMESPACE}name")
            errors.append(f"{label} declares non-signature permission {name}")
    return errors


def check_android_permissions(root: pathlib.Path, files: list[str]) -> Result:
    result = Result()
    if PRODUCTION_MANIFEST not in files:
        result.errors.append(f"production Android manifest is missing: {PRODUCTION_MANIFEST}")
    manifests = [
        path for path in files
        if path.startswith("apps/")
        and path.endswith("/AndroidManifest.xml")
        and not any(marker in f"/{path}" for marker in DEVELOPMENT_SOURCE_SETS)
    ]
    for manifest in manifests:
        text = read_text(root, manifest)
        if text is None:
            result.errors.append(f"cannot read Android manifest {manifest}")
            continue
        result.errors.extend(manifest_errors(text, manifest))
    result.coverage.append(
        f"Android manifests: {len(manifests)} release-source-set manifests in the "
        "repository (manifests of plugins from the pub cache are covered only by "
        "--android-merged-manifest)"
    )
    return result


def check_merged_manifest(path: pathlib.Path) -> Result:
    result = Result()
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        result.errors.append(f"merged manifest not found or unreadable: {path}")
        return result
    result.errors.extend(manifest_errors(text, f"merged manifest {path.name}"))
    result.coverage.append(f"merged Android manifest: {path}")
    return result


def check_network_capability(root: pathlib.Path, files: list[str]) -> Result:
    result = Result()
    entitlements = [path for path in files if path.endswith(".entitlements")]
    for path in entitlements:
        if IOS_NETWORK_ENTITLEMENT.search(read_text(root, path) or ""):
            result.errors.append(f"network entitlement in {path}")

    scanned = 0
    for path in files:
        if any(marker in f"/{path}" for marker in NETWORK_SCAN_EXCLUDED):
            continue
        for prefix, suffix, pattern in NETWORK_APIS:
            if not (path.startswith(prefix) and path.endswith(suffix)):
                continue
            scanned += 1
            text = read_text(root, path)
            if text is None:
                result.errors.append(f"cannot read source file {path}")
                continue
            for number, line in enumerate(text.splitlines(), start=1):
                match = pattern.search(line)
                if match:
                    result.errors.append(f"network API in {path}:{number}: {match.group(0)!r}")
    result.coverage.append(
        f"network capability: {len(entitlements)} iOS entitlement files, "
        f"{scanned} shipped Dart/Rust/Kotlin/Java/Swift/Objective-C files "
        "(a pattern scan of this repository's code, not of dependencies)"
    )
    return result


def dart_lock_names(text: str) -> list[str]:
    if not re.search(r"^packages:\s*$", text, flags=re.MULTILINE):
        raise ValueError("no packages section")
    in_packages = False
    names = []
    for line in text.splitlines():
        if line.startswith("packages:"):
            in_packages = True
        elif line and not line.startswith(" "):
            in_packages = False
        elif in_packages:
            match = re.match(r"^  ([A-Za-z0-9_]+):\s*$", line)
            if match:
                names.append(match.group(1))
    return names


def check_dependencies(root: pathlib.Path, files: list[str]) -> Result:
    result = Result()
    lockfiles = [
        path for path in files
        if path.rsplit("/", 1)[-1] in {"pubspec.lock", "Cargo.lock"}
    ]
    classified = set(SHIPPED_LOCKFILES) | set(NOT_SHIPPED_LOCKFILES)
    for required in SHIPPED_LOCKFILES:
        if required not in files:
            result.errors.append(f"missing lockfile: {required}")
    for path in lockfiles:
        if path not in classified:
            result.errors.append(
                f"lockfile is not classified: {path}; add it to SHIPPED_LOCKFILES "
                "(and the license inventory) or NOT_SHIPPED_LOCKFILES with a reason"
            )
    if LICENSE_INVENTORY not in files:
        result.errors.append(f"missing license inventory: {LICENSE_INVENTORY}")
    else:
        try:
            json.loads(read_text(root, LICENSE_INVENTORY) or "")
        except json.JSONDecodeError as error:
            result.errors.append(f"cannot parse {LICENSE_INVENTORY}: {error}")

    packages = 0
    for path in lockfiles:
        text = read_text(root, path) or ""
        try:
            if path.endswith("pubspec.lock"):
                names = dart_lock_names(text)
                kind, forbidden = "Dart package", FORBIDDEN_DART_PACKAGES
            else:
                names = [package["name"] for package in tomllib.loads(text).get("package", [])]
                kind, forbidden = "Rust crate", FORBIDDEN_RUST_CRATES
        except (ValueError, KeyError, TypeError, tomllib.TOMLDecodeError) as error:
            result.errors.append(f"cannot parse {path}: {error}")
            continue
        packages += len(names)
        for name in names:
            if name_matches(name, forbidden):
                result.errors.append(f"forbidden {kind} {name!r} in {path}")

    native_files = [path for path in files if path.rsplit("/", 1)[-1] in GRADLE_AND_POD_FILES]
    for path in native_files:
        for number, line in enumerate((read_text(root, path) or "").splitlines(), start=1):
            match = FORBIDDEN_NATIVE_DEPENDENCIES.search(line)
            if match:
                result.errors.append(
                    f"forbidden Android dependency or iOS pod in {path}:{number}: {match.group(0)!r}"
                )

    for path in files:
        if path.rsplit("/", 1)[-1] in MARKER_SCAN_EXCLUDED:
            continue
        content = (read_text(root, path) or "").lower()
        for marker in FORBIDDEN_DEPENDENCY_MARKERS:
            if re.search(rf"(^|[^a-z0-9_]){re.escape(marker)}([^a-z0-9_]|$)", content):
                result.errors.append(f"forbidden telemetry/ad dependency marker {marker!r} in {path}")

    result.coverage.append(
        f"dependencies: {packages} locked packages in {len(lockfiles)} lockfiles, "
        f"{len(native_files)} Gradle/CocoaPods/SwiftPM files"
    )
    return result


def check_secrets(root: pathlib.Path, files: list[str]) -> Result:
    result = Result()
    for path in files:
        name = path.rsplit("/", 1)[-1]
        if name not in ALLOWED_FILE_NAMES and any(
            fnmatch.fnmatch(name.lower(), pattern.lower()) for pattern in FORBIDDEN_FILE_PATTERNS
        ):
            result.errors.append(
                f"forbidden file {path}: signing material, credentials or cloud config"
            )
            continue
        text = read_text(root, path)
        if text is None:
            continue
        for label, pattern in SECRET_PATTERNS:
            if pattern.search(text):
                result.errors.append(f"possible {label} in {path}")
    result.coverage.append(
        f"secret scan: {len(files)} files, {len(SECRET_PATTERNS)} token formats "
        "(known formats only; it cannot recognise arbitrary passwords or keys)"
    )
    return result


def run_checks(root: pathlib.Path, files: list[str]) -> Result:
    result = Result()
    for check in (
        check_required_policies(root),
        check_android_permissions(root, files),
        check_network_capability(root, files),
        check_dependencies(root, files),
        check_secrets(root, files),
    ):
        result.extend(check)
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    parser.add_argument(
        "--android-merged-manifest",
        type=pathlib.Path,
        help="check only this Gradle-merged release manifest",
    )
    arguments = parser.parse_args(argv)

    if arguments.android_merged_manifest:
        result = check_merged_manifest(arguments.android_merged_manifest)
    else:
        result = run_checks(ROOT, repository_files(ROOT))

    for line in result.coverage:
        print(f"Checked {line}")
    if result.errors:
        print("Product invariant check failed:", file=sys.stderr)
        for error in result.errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Product invariant check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
