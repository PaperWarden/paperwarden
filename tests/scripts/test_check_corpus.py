"""Tests for scripts/check_corpus.py and scripts/corpus_facts.py.

The real corpus must pass, every declared fact must be load-bearing (a false
value for it is rejected), and each schema rule must fail when broken.
"""

from __future__ import annotations

import copy
import json
import pathlib
import shutil
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import check_corpus  # noqa: E402
import corpus_facts  # noqa: E402

CORPUS = ROOT / "tests" / "corpus"


def falsify(value):
    """A value of the same kind that must not satisfy the fact."""
    if isinstance(value, bool):
        return not value
    if isinstance(value, int):
        return value + 1
    if isinstance(value, str):
        return value + "-not"
    if isinstance(value, list):
        return value + (["not-present"] if all(isinstance(item, str) for item in value) else [0])
    if isinstance(value, dict):
        return {**value, "not/present.xml": "absent"}
    raise TypeError(value)


class RealCorpusTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.manifest = json.loads((CORPUS / "manifest.json").read_text(encoding="utf-8"))

    def test_real_corpus_passes(self) -> None:
        result = check_corpus.check_corpus(CORPUS)
        self.assertEqual(result.errors, [])
        self.assertGreater(result.facts, 100)

    def test_every_fixture_declares_facts_of_known_kinds(self) -> None:
        for entry in self.manifest["fixtures"]:
            self.assertTrue(entry["facts"], entry["path"])
            self.assertLessEqual(set(entry["facts"]), corpus_facts.KINDS, entry["path"])

    def test_every_fact_rejects_a_false_value(self) -> None:
        checked = 0
        for entry in self.manifest["fixtures"]:
            data = (CORPUS / entry["path"]).read_bytes()
            for kind, value in entry["facts"].items():
                wrong = "00" + value if kind == "signature" else falsify(value)
                if kind == "zip_ratio_at_least":
                    wrong = value * 1000
                if kind == "absent":  # claim that something present is absent
                    wrong = value + [data[:4].decode("ascii")]
                with self.subTest(fixture=entry["path"], fact=kind):
                    self.assertIsNone(corpus_facts.verify(kind, value, data))
                    self.assertIsNotNone(corpus_facts.verify(kind, wrong, data))
                checked += 1
        self.assertGreater(checked, 100)

    def test_unknown_fact_kind_is_rejected(self) -> None:
        self.assertIn("unknown fact kind", corpus_facts.verify("is_safe", True, b""))

    def test_engine_outcomes_are_reported_pending(self) -> None:
        report = check_corpus.check_corpus(CORPUS).report
        inputs = [row for row in report if row["outcome"] != "reference"]
        self.assertTrue(inputs)
        self.assertTrue(all(str(row["engine"]).startswith("pending") for row in inputs))


class SchemaRuleTest(unittest.TestCase):
    """Each test breaks one rule in a temporary copy of the corpus."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.corpus = pathlib.Path(self._tmp.name) / "corpus"
        shutil.copytree(CORPUS, self.corpus)
        self.manifest = json.loads((self.corpus / "manifest.json").read_text(encoding="utf-8"))

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def entry(self, path: str) -> dict:
        return next(entry for entry in self.manifest["fixtures"] if entry["path"] == path)

    def assertFailsWith(self, fragment: str) -> None:
        (self.corpus / "manifest.json").write_text(json.dumps(self.manifest), encoding="utf-8")
        errors = check_corpus.check_corpus(self.corpus).errors
        self.assertTrue(any(fragment in error for error in errors), f"{fragment!r} not in {errors!r}")

    def test_schema_version_is_required(self) -> None:
        self.manifest["schema"] = 1
        self.assertFailsWith("manifest schema must be 2")

    def test_missing_and_unknown_fields_fail(self) -> None:
        entry = self.entry("valid/minimal.txt")
        del entry["provenance"]
        entry["verified_by"] = "nobody"
        self.assertFailsWith("valid/minimal.txt is missing provenance")
        self.assertFailsWith("unknown field verified_by")

    def test_wrong_category_directory_fails(self) -> None:
        self.entry("valid/minimal.txt")["category"] = "security"
        self.assertFailsWith("not under its category directory")

    def test_format_directory_must_match(self) -> None:
        self.entry("valid/pdf/one-page.pdf")["format"] = "image"
        self.assertFailsWith("is not in its format directory")

    def test_unknown_outcome_and_capability_fail(self) -> None:
        entry = self.entry("valid/minimal.txt")
        entry["outcome"] = "probably-fine"
        entry["requires"] = ["magic"]
        self.assertFailsWith("unknown outcome")
        self.assertFailsWith("requires unknown capabilities")

    def test_reference_fixtures_require_nothing(self) -> None:
        self.entry("ocr/printed-line.txt")["requires"] = ["ocr"]
        self.assertFailsWith("reference fixtures require nothing")

    def test_input_must_require_a_capability(self) -> None:
        self.entry("valid/minimal.txt")["requires"] = []
        self.assertFailsWith("inputs require a capability")

    def test_non_redistributable_license_fails(self) -> None:
        self.entry("valid/minimal.txt")["license"] = "GPL-3.0-only"
        self.assertFailsWith("not cleared for redistribution")

    def test_hash_and_size_mismatch_fail(self) -> None:
        (self.corpus / "valid" / "minimal.txt").write_bytes(b"changed")
        self.assertFailsWith("hash mismatch: valid/minimal.txt")
        self.assertFailsWith("size mismatch: valid/minimal.txt")

    def test_false_fact_fails(self) -> None:
        self.entry("security/archive/zip-bomb.zip")["facts"]["zip_uncompressed_bytes"] = 1
        self.assertFailsWith("fact zip_uncompressed_bytes does not hold")

    def test_unlisted_file_fails(self) -> None:
        (self.corpus / "valid" / "stray.bin").write_bytes(b"x")
        self.assertFailsWith("fixture is not in manifest: valid/stray.bin")

    def test_missing_file_fails(self) -> None:
        (self.corpus / "valid" / "minimal.txt").unlink()
        self.assertFailsWith("missing fixture: valid/minimal.txt")

    def test_reference_must_exist_and_be_a_reference(self) -> None:
        entry = self.entry("ocr/printed-line.png")
        entry["references"] = ["ocr/absent.txt", "valid/minimal.txt"]
        self.assertFailsWith("references a fixture not in the manifest: ocr/absent.txt")
        self.assertFailsWith("which is not a reference fixture")

    def test_unsafe_paths_fail(self) -> None:
        rtl = chr(0x202E)
        for path in ("../escape.txt", "/abs.txt", "valid\\x.txt", f"valid/a{rtl}txt.exe"):
            with self.subTest(path=path):
                manifest = copy.deepcopy(self.manifest)
                manifest["fixtures"][0]["path"] = path
                (self.corpus / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
                errors = check_corpus.check_corpus(self.corpus).errors
                self.assertTrue(any("unsafe fixture path" in error for error in errors), errors)

    def test_duplicate_paths_fail(self) -> None:
        self.manifest["fixtures"].append(copy.deepcopy(self.manifest["fixtures"][0]))
        self.assertFailsWith("duplicate fixture path")

    def test_size_budgets_are_enforced(self) -> None:
        original = (check_corpus.MAX_FIXTURE_BYTES, check_corpus.MAX_CORPUS_BYTES)
        try:
            check_corpus.MAX_FIXTURE_BYTES, check_corpus.MAX_CORPUS_BYTES = 1000, 2000
            self.assertFailsWith("-byte fixture limit")
            self.assertFailsWith("the budget is 2000")
        finally:
            check_corpus.MAX_FIXTURE_BYTES, check_corpus.MAX_CORPUS_BYTES = original


if __name__ == "__main__":
    unittest.main()
