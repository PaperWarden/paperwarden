"""Tests for scripts/check_privacy_manifest.py. They run anywhere: binaries are
given as saved `nm -u` output, and the real manifest is checked against the
PDFium framework's symbols when they are available locally."""

from __future__ import annotations

import copy
import pathlib
import plistlib
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import check_privacy_manifest as checker  # noqa: E402

MANIFEST = plistlib.loads(checker.MANIFEST.read_bytes())
FILE_TIMESTAMP = "NSPrivacyAccessedAPICategoryFileTimestamp"


class Symbols(unittest.TestCase):
    def test_nm_output_is_normalised(self) -> None:
        output = "/path/Runner:\n                 U _stat$INODE64\n_fstat\n_OBJC_CLASS_$_NSUserDefaults\n\n"
        self.assertEqual(checker.symbol_names(output), {"stat", "fstat", "OBJC_CLASS_$_NSUserDefaults"})

    def test_categories_come_from_symbols_and_selectors(self) -> None:
        used = checker.categories_used({"fstat", "mach_absolute_time", "malloc"}, b"...activeInputModes...")
        self.assertEqual(used[FILE_TIMESTAMP], {"fstat"})
        self.assertEqual(used["NSPrivacyAccessedAPICategorySystemBootTime"], {"mach_absolute_time"})
        self.assertEqual(used["NSPrivacyAccessedAPICategoryActiveKeyboards"], {"activeInputModes"})
        self.assertNotIn("NSPrivacyAccessedAPICategoryDiskSpace", used)


class Manifest(unittest.TestCase):
    def test_the_real_manifest_is_valid_and_declares_file_timestamps(self) -> None:
        self.assertEqual(checker.manifest_problems(MANIFEST), [])
        self.assertIn(FILE_TIMESTAMP, checker.declared(MANIFEST))

    def test_an_undeclared_category_fails(self) -> None:
        problems = checker.check(MANIFEST, {"Runner": ({"statfs"}, b"")})
        self.assertEqual(len(problems), 1)
        self.assertIn("DiskSpace", problems[0])
        self.assertIn("statfs", problems[0])

    def test_a_declared_category_passes(self) -> None:
        self.assertEqual(checker.check(MANIFEST, {"pdfium": ({"stat", "fstat", "malloc"}, b"")}), [])

    def test_tracking_collection_and_bad_reasons_fail(self) -> None:
        for key, value in [
            ("NSPrivacyTracking", True),
            ("NSPrivacyTrackingDomains", ["tracker.example"]),
            ("NSPrivacyCollectedDataTypes", [{"NSPrivacyCollectedDataType": "x"}]),
        ]:
            with self.subTest(key=key):
                manifest = copy.deepcopy(MANIFEST)
                manifest[key] = value
                self.assertTrue(checker.manifest_problems(manifest))
        manifest = copy.deepcopy(MANIFEST)
        manifest["NSPrivacyAccessedAPITypes"][0]["NSPrivacyAccessedAPITypeReasons"] = ["XXXX.1"]
        self.assertTrue(checker.manifest_problems(manifest))

    def test_main_reads_saved_symbols(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            good = pathlib.Path(directory) / "good.txt"
            good.write_text("_stat\n_malloc\n", encoding="utf-8")
            bad = pathlib.Path(directory) / "bad.txt"
            bad.write_text("_mach_absolute_time\n", encoding="utf-8")
            self.assertEqual(checker.main(["--symbols", str(good)]), 0)
            self.assertEqual(checker.main(["--symbols", str(bad)]), 1)

    def test_the_pdfium_framework_is_covered_when_available(self) -> None:
        framework = ROOT / "third_party" / "pdfium" / "ios-device-arm64" / "lib" / "pdfium.framework" / "pdfium"
        imports = ROOT / "docs" / "feasibility" / "evidence" / "PW-011" / "ios-imports.txt"
        if imports.exists():
            symbols = checker.symbol_names(imports.read_text(encoding="utf-8"))
        elif framework.exists():
            self.skipTest("nm is not available here; CI checks the framework itself")
        else:
            self.skipTest("PDFium iOS framework not fetched")
        self.assertEqual(checker.check(MANIFEST, {"pdfium": (symbols, b"")}), [])


if __name__ == "__main__":
    unittest.main()
