"""Tests for scripts/check_invariants.py.

Each test starts from a minimal repository that passes every check, breaks one
thing, and asserts that the matching check fails. Forbidden names and secret
samples are assembled at runtime so this file never contains them literally.
"""

from __future__ import annotations

import pathlib
import sys
import tempfile
import textwrap
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[2] / "scripts"))

import check_invariants as ci  # noqa: E402

INVARIANTS = "# Product Invariants\n\n" + "\n".join(
    [
        "1. No required PaperWarden backend.",
        "2. No paid API.",
        "3. No ads, subscriptions, analytics, or telemetry.",
        "4. Never uploads documents.",
        "5. Android must not request `android.permission.INTERNET` for 1.0.",
        "6. Active content is never executed.",
        "7. Remote resources are never fetched automatically.",
        "8. Originals are not silently overwritten.",
        "9. Isolated temporary workspace.",
        "10. Magic bytes.",
        "11. Archive limits.",
        "12. Corpus before advertising.",
        "13. Redaction.",
        "14. Security checks are fixed, not removed or weakened to make CI pass.",
    ]
)

MANIFEST = """<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <application android:label="paperwarden"/>
</manifest>
"""

DEBUG_MANIFEST = """<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-permission android:name="android.permission.INTERNET"/>
</manifest>
"""

PUBSPEC_LOCK = textwrap.dedent(
    """\
    packages:
      async:
        dependency: transitive
        description:
          name: async
        source: hosted
        version: "2.13.1"
      flutter:
        dependency: "direct main"
        description: flutter
        source: sdk
        version: "0.0.0"
    sdks:
      dart: ">=3.0.0"
    """
)

CARGO_LOCK = textwrap.dedent(
    """\
    version = 4

    [[package]]
    name = "paperwarden-core"
    version = "0.0.1"

    [[package]]
    name = "memchr"
    version = "2.7.4"
    source = "registry+https://github.com/rust-lang/crates.io-index"
    """
)


def workflow(name: str, *commands: str) -> str:
    steps = "\n".join(f"      - run: {command}" for command in commands)
    return textwrap.dedent(
        f"""\
        name: {name}
        on: [pull_request]
        permissions:
          contents: read
        jobs:
          check:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@{"a" * 40} # v6
        """
    ) + steps + "\n"


class FakeRepo:
    def __init__(self, root: pathlib.Path) -> None:
        self.root = root
        for policy in ci.REQUIRED_POLICIES:
            self.write(policy, f"# {policy}\n\nContent.\n")
        self.write("PRODUCT_INVARIANTS.md", INVARIANTS)
        self.write("LICENSE", "Apache License\nVersion 2.0, January 2004\n")
        self.write(ci.PRODUCTION_MANIFEST, MANIFEST)
        self.write("apps/mobile/android/app/src/debug/AndroidManifest.xml", DEBUG_MANIFEST)
        self.write("apps/mobile/android/app/src/profile/AndroidManifest.xml", DEBUG_MANIFEST)
        self.write("apps/mobile/pubspec.lock", PUBSPEC_LOCK)
        self.write("core/Cargo.lock", CARGO_LOCK)
        self.write("fuzz/Cargo.lock", CARGO_LOCK)
        self.write("third_party/dependencies.json", "[]\n")
        self.write("apps/mobile/lib/main.dart", "import 'package:flutter/material.dart';\n")
        self.write("core/src/lib.rs", "pub fn version() -> &'static str { \"0\" }\n")
        for file_name, commands in ci.REQUIRED_WORKFLOW_COMMANDS.items():
            self.write(f".github/workflows/{file_name}", workflow(file_name, *commands))

    def write(self, relative: str, content: str) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def delete(self, relative: str) -> None:
        (self.root / relative).unlink()

    def errors(self) -> list[str]:
        return ci.run_checks(self.root, ci.repository_files(self.root)).errors


class CheckInvariantsTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.repo = FakeRepo(pathlib.Path(self._tmp.name))

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def assertFailsWith(self, fragment: str) -> None:
        errors = self.repo.errors()
        self.assertTrue(
            any(fragment in error for error in errors),
            f"expected an error containing {fragment!r}, got {errors!r}",
        )

    # Baseline -----------------------------------------------------------

    def test_minimal_repository_passes(self) -> None:
        self.assertEqual(self.repo.errors(), [])

    # Required policies --------------------------------------------------

    def test_missing_policy_fails(self) -> None:
        self.repo.delete("THREAT_MODEL.md")
        self.assertFailsWith("missing required policy: THREAT_MODEL.md")

    def test_emptied_policy_fails(self) -> None:
        self.repo.write("PRIVACY.md", "  \n")
        self.assertFailsWith("required policy is empty: PRIVACY.md")

    def test_dropped_invariant_fails(self) -> None:
        self.repo.write("PRODUCT_INVARIANTS.md", INVARIANTS.replace("14. ", "- "))
        self.assertFailsWith("expected at least 14 numbered invariants")

    def test_reworded_invariant_anchor_fails(self) -> None:
        self.repo.write(
            "PRODUCT_INVARIANTS.md",
            INVARIANTS.replace("analytics, or telemetry", "analytics (optional)"),
        )
        self.assertFailsWith("PRODUCT_INVARIANTS.md no longer states")

    def test_non_apache_license_fails(self) -> None:
        self.repo.write("LICENSE", "MIT License\n")
        self.assertFailsWith("LICENSE is not the Apache License 2.0")

    def test_deleted_gate_workflow_fails(self) -> None:
        self.repo.delete(".github/workflows/licenses.yml")
        self.assertFailsWith("missing required workflow: .github/workflows/licenses.yml")

    def test_gate_command_removed_from_workflow_fails(self) -> None:
        self.repo.write(".github/workflows/security.yml", workflow("security", "echo skipped"))
        self.assertFailsWith(".github/workflows/security.yml no longer runs")

    def test_unpinned_action_fails(self) -> None:
        self.repo.write(
            ".github/workflows/extra.yml",
            "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@v6\n",
        )
        self.assertFailsWith("not pinned to a full commit SHA")

    # Android permissions ------------------------------------------------

    def test_missing_production_manifest_fails(self) -> None:
        self.repo.delete(ci.PRODUCTION_MANIFEST)
        self.assertFailsWith("production Android manifest is missing")

    def test_internet_permission_in_production_fails(self) -> None:
        self.repo.write(ci.PRODUCTION_MANIFEST, DEBUG_MANIFEST)
        self.assertFailsWith("android.permission.INTERNET")

    def test_internet_permission_via_sdk23_tag_fails(self) -> None:
        self.repo.write(
            ci.PRODUCTION_MANIFEST,
            DEBUG_MANIFEST.replace("uses-permission", "uses-permission-sdk-23"),
        )
        self.assertFailsWith("android.permission.INTERNET")

    def test_internet_permission_in_plugin_manifest_fails(self) -> None:
        self.repo.write("apps/mobile/rust_builder/android/src/main/AndroidManifest.xml", DEBUG_MANIFEST)
        self.assertFailsWith("rust_builder/android/src/main/AndroidManifest.xml")

    def test_unreviewed_permission_fails(self) -> None:
        self.repo.write(
            ci.PRODUCTION_MANIFEST,
            DEBUG_MANIFEST.replace("INTERNET", "READ_CONTACTS"),
        )
        self.assertFailsWith("permission not in the reviewed allowlist: android.permission.READ_CONTACTS")

    def test_malformed_manifest_fails(self) -> None:
        self.repo.write(ci.PRODUCTION_MANIFEST, "<manifest")
        self.assertFailsWith("cannot parse Android manifest")

    def test_debug_and_profile_internet_permission_allowed(self) -> None:
        self.assertEqual(self.repo.errors(), [])

    def test_merged_manifest_mode_checks_only_that_file(self) -> None:
        merged = self.repo.root / "merged.xml"
        merged.write_text(DEBUG_MANIFEST, encoding="utf-8")
        result = ci.check_merged_manifest(merged)
        self.assertTrue(any("android.permission.INTERNET" in e for e in result.errors))
        merged.write_text(MANIFEST, encoding="utf-8")
        self.assertEqual(ci.check_merged_manifest(merged).errors, [])

    def test_merged_manifest_missing_fails(self) -> None:
        result = ci.check_merged_manifest(self.repo.root / "absent.xml")
        self.assertTrue(any("merged manifest not found" in e for e in result.errors))

    # Network capability -------------------------------------------------

    def test_ios_network_entitlement_fails(self) -> None:
        self.repo.write(
            "apps/mobile/ios/Runner/Runner.entitlements",
            "<key>com.apple.security.network.client</key><true/>",
        )
        self.assertFailsWith("network entitlement")

    def test_dart_http_client_fails(self) -> None:
        self.repo.write("apps/mobile/lib/src/fetch.dart", "final c = " + "Http" + "Client();\n")
        self.assertFailsWith("network API in apps/mobile/lib/src/fetch.dart")

    def test_dart_network_image_fails(self) -> None:
        self.repo.write("apps/mobile/lib/src/logo.dart", "Image" + ".network(url);\n")
        self.assertFailsWith("network API in apps/mobile/lib/src/logo.dart")

    def test_dart_http_package_import_fails(self) -> None:
        self.repo.write("apps/mobile/lib/src/api.dart", "import 'package:" + "http/http.dart';\n")
        self.assertFailsWith("network API in apps/mobile/lib/src/api.dart")

    def test_rust_std_net_fails(self) -> None:
        self.repo.write("core/crates/bridge/src/net.rs", "use std::" + "net::TcpStream;\n")
        self.assertFailsWith("network API in core/crates/bridge/src/net.rs")

    def test_kotlin_url_connection_fails(self) -> None:
        self.repo.write(
            "apps/mobile/android/app/src/main/kotlin/MainActivity.kt",
            "val c = url.openConnection() as " + "HttpURL" + "Connection\n",
        )
        self.assertFailsWith("network API in apps/mobile/android/app/src/main/kotlin/MainActivity.kt")

    def test_swift_url_session_fails(self) -> None:
        self.repo.write("apps/mobile/ios/Runner/AppDelegate.swift", "let s = " + "URL" + "Session.shared\n")
        self.assertFailsWith("network API in apps/mobile/ios/Runner/AppDelegate.swift")

    def test_network_words_in_tests_are_not_scanned(self) -> None:
        self.repo.write("apps/mobile/test/fake.dart", "// " + "Http" + "Client is not used\n")
        self.assertEqual(self.repo.errors(), [])

    # Dependencies -------------------------------------------------------

    def test_telemetry_dart_package_fails(self) -> None:
        name = "firebase" + "_crashlytics"
        self.repo.write(
            "apps/mobile/pubspec.lock",
            PUBSPEC_LOCK.replace("  async:", f"  {name}:"),
        )
        self.assertFailsWith(f"forbidden Dart package {name!r}")

    def test_http_dart_package_fails(self) -> None:
        self.repo.write("apps/mobile/pubspec.lock", PUBSPEC_LOCK.replace("  async:", "  dio:"))
        self.assertFailsWith("forbidden Dart package 'dio'")

    def test_subscription_dart_package_fails(self) -> None:
        name = "purchases" + "_flutter"
        self.repo.write("apps/mobile/pubspec.lock", PUBSPEC_LOCK.replace("  async:", f"  {name}:"))
        self.assertFailsWith(f"forbidden Dart package {name!r}")

    def test_http_crate_fails(self) -> None:
        self.repo.write("core/Cargo.lock", CARGO_LOCK.replace('"memchr"', '"req' + 'west"'))
        self.assertFailsWith("forbidden Rust crate 'req" + "west' in core/Cargo.lock")

    def test_telemetry_crate_in_fuzz_lock_fails(self) -> None:
        self.repo.write("fuzz/Cargo.lock", CARGO_LOCK.replace('"memchr"', '"sen' + 'try"'))
        self.assertFailsWith("in fuzz/Cargo.lock")

    def test_gradle_ads_dependency_fails(self) -> None:
        self.repo.write(
            "apps/mobile/android/app/build.gradle.kts",
            'dependencies { implementation("com.google.android.gms:play-services-' + 'ads:23.0.0") }\n',
        )
        self.assertFailsWith("forbidden Android dependency")

    def test_firebase_config_file_fails(self) -> None:
        self.repo.write("apps/mobile/android/app/google-" + "services.json", "{}")
        self.assertFailsWith("forbidden file")

    def test_legacy_marker_still_detected(self) -> None:
        self.repo.write("docs/plan.md", "We will add " + "mix" + "panel.\n")
        self.assertFailsWith("forbidden telemetry/ad dependency marker")

    # Lockfile inventory -------------------------------------------------

    def test_missing_shipped_lockfile_fails(self) -> None:
        self.repo.delete("core/Cargo.lock")
        self.assertFailsWith("missing lockfile: core/Cargo.lock")

    def test_unclassified_lockfile_fails(self) -> None:
        self.repo.write("tools/gen/pubspec.lock", PUBSPEC_LOCK)
        self.assertFailsWith("lockfile is not classified: tools/gen/pubspec.lock")

    def test_unparseable_lockfile_fails(self) -> None:
        self.repo.write("core/Cargo.lock", "[[package]\n")
        self.assertFailsWith("cannot parse core/Cargo.lock")

    def test_missing_inventory_fails(self) -> None:
        self.repo.delete("third_party/dependencies.json")
        self.assertFailsWith("missing license inventory")

    # Secrets ------------------------------------------------------------

    def test_private_key_fails(self) -> None:
        self.repo.write(
            "apps/mobile/android/key.pem",
            "-----BEGIN " + "PRIVATE KEY-----\nabc\n",
        )
        self.assertFailsWith("possible private key in apps/mobile/android/key.pem")

    def test_github_token_fails(self) -> None:
        self.repo.write("scripts/x.sh", "TOKEN=" + "ghp_" + "A" * 36 + "\n")
        self.assertFailsWith("possible GitHub token in scripts/x.sh")

    def test_aws_key_fails(self) -> None:
        self.repo.write("notes.txt", "AKIA" + "ABCDEFGHIJKLMNOP\n")
        self.assertFailsWith("possible AWS access key in notes.txt")

    def test_google_api_key_fails(self) -> None:
        self.repo.write("cfg.json", '{"k": "AIza' + "B" * 35 + '"}')
        self.assertFailsWith("possible Google API key in cfg.json")

    def test_anthropic_key_fails(self) -> None:
        self.repo.write("cfg.env.example", "KEY=sk-" + "ant-" + "x" * 40 + "\n")
        self.assertFailsWith("possible Anthropic API key")

    def test_signing_keystore_file_fails(self) -> None:
        (self.repo.root / "apps/mobile/android/upload.jks").write_bytes(b"\x00\x01")
        self.assertFailsWith("forbidden file apps/mobile/android/upload.jks")

    def test_key_properties_fails(self) -> None:
        self.repo.write("apps/mobile/android/key.properties", "storePassword=x\n")
        self.assertFailsWith("forbidden file apps/mobile/android/key.properties")

    def test_env_file_fails(self) -> None:
        self.repo.write(".env", "A=1\n")
        self.assertFailsWith("forbidden file .env")

    # Reporting ----------------------------------------------------------

    def test_coverage_is_reported(self) -> None:
        result = ci.run_checks(self.repo.root, ci.repository_files(self.repo.root))
        summary = "\n".join(result.coverage)
        self.assertIn("Android manifests", summary)
        self.assertIn("locked packages", summary)
        self.assertIn("secret scan", summary)


if __name__ == "__main__":
    unittest.main()
