"""Tests for scripts/check_render_service.py (#59).

The check must fail any merged manifest where PDFium's service could leave
its isolated process, be reached from outside the app, or where the release
app requests a permission; and any APK missing PDFium or the bridge.
"""

from __future__ import annotations

import pathlib
import sys
import tempfile
import unittest
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import check_render_service  # noqa: E402

SERVICE = (
    '<service android:name=".RenderService" android:process=":pdfium" '
    'android:isolatedProcess="true" android:exported="false" />'
)


def manifest(service: str = SERVICE, extra: str = "", top: str = "") -> str:
    return (
        '<manifest xmlns:android="http://schemas.android.com/apk/res/android" '
        f'package="dev.paperwarden.paperwarden">{top}<application>'
        '<activity android:name=".MainActivity" android:exported="true" />'
        f"{service}{extra}</application></manifest>"
    )


class ManifestTest(unittest.TestCase):
    def test_the_shipped_declaration_passes(self) -> None:
        self.assertEqual(check_render_service.check_manifest(manifest()), [])

    def test_a_fully_qualified_name_is_the_same_service(self) -> None:
        text = manifest(SERVICE.replace(".RenderService", "dev.paperwarden.paperwarden.RenderService"))
        self.assertEqual(check_render_service.check_manifest(text), [])

    def test_each_weakened_attribute_fails(self) -> None:
        cases = {
            ('android:isolatedProcess="true"', 'android:isolatedProcess="false"'):
                "not an isolatedProcess",
            ('android:exported="false"', 'android:exported="true"'): "must not be exported",
            ('android:process=":pdfium"', 'android:process=":main"'): "its own process",
        }
        for (original, change), message in cases.items():
            self.assertIn(original, SERVICE)
            service = SERVICE.replace(original, change)
            with self.subTest(change):
                errors = check_render_service.check_manifest(manifest(service))
                self.assertTrue(any(message in error for error in errors), errors)

    def test_missing_attributes_fail(self) -> None:
        bare = '<service android:name=".RenderService" />'
        errors = check_render_service.check_manifest(manifest(bare))
        self.assertEqual(len(errors), 3, errors)

    def test_an_intent_filter_fails(self) -> None:
        service = SERVICE.replace(
            " />", "><intent-filter><action android:name=\"x\" /></intent-filter></service>"
        )
        errors = check_render_service.check_manifest(manifest(service))
        self.assertTrue(any("intent filter" in error for error in errors), errors)

    def test_another_component_in_the_pdfium_process_fails(self) -> None:
        extra = '<receiver android:name=".Other" android:process=":pdfium" />'
        errors = check_render_service.check_manifest(manifest(extra=extra))
        self.assertTrue(any("shares the PDFium process" in error for error in errors), errors)

    def test_a_missing_or_duplicated_service_fails(self) -> None:
        self.assertTrue(check_render_service.check_manifest(manifest("")))
        self.assertTrue(check_render_service.check_manifest(manifest(SERVICE + SERVICE)))

    def test_any_permission_in_the_release_app_fails(self) -> None:
        for tag in ("uses-permission", "uses-permission-sdk-23"):
            top = f'<{tag} android:name="android.permission.INTERNET" />'
            with self.subTest(tag):
                errors = check_render_service.check_manifest(manifest(top=top))
                self.assertEqual(errors, ["release app requests android.permission.INTERNET"])

    def test_only_the_apps_own_signature_permission_is_allowed(self) -> None:
        own = "dev.paperwarden.paperwarden.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"
        declared = f'<permission android:name="{own}" android:protectionLevel="signature" />'
        uses = f'<uses-permission android:name="{own}" />'
        self.assertEqual(check_render_service.check_manifest(manifest(top=declared + uses)), [])
        # Not declared here, or not signature-level, or someone else's.
        for top in (
            uses,
            declared.replace("signature", "normal") + uses,
            declared.replace("dev.paperwarden.paperwarden.", "com.other.")
            + uses.replace("dev.paperwarden.paperwarden.", "com.other."),
        ):
            with self.subTest(top):
                self.assertTrue(check_render_service.check_manifest(manifest(top=top)))

    def test_text_that_is_not_xml_fails(self) -> None:
        self.assertTrue(check_render_service.check_manifest("<manifest"))


class ApkTest(unittest.TestCase):
    def apk(self, names: list[str]) -> pathlib.Path:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = pathlib.Path(directory.name) / "app.apk"
        with zipfile.ZipFile(path, "w") as archive:
            for name in names:
                archive.writestr(name, b"")
        return path

    def test_every_abi_needs_pdfium_and_the_bridge(self) -> None:
        apk = self.apk([
            "lib/x86_64/libpaperwarden_pdfium.so",
            "lib/x86_64/libpaperwarden_bridge.so",
            "lib/arm64-v8a/libpaperwarden_bridge.so",
        ])
        self.assertEqual(check_render_service.check_apk(apk, ["x86_64"]), [])
        self.assertEqual(
            check_render_service.check_apk(apk, ["x86_64", "arm64-v8a"]),
            ["libpaperwarden_pdfium.so missing for arm64-v8a"],
        )

    def test_pdfium_under_the_system_name_fails(self) -> None:
        apk = self.apk([
            "lib/x86_64/libpaperwarden_pdfium.so",
            "lib/x86_64/libpaperwarden_bridge.so",
            "lib/x86_64/libpdfium.so",
        ])
        self.assertEqual(
            check_render_service.check_apk(apk, ["x86_64"]),
            ["lib/x86_64/libpdfium.so must ship as libpaperwarden_pdfium.so"],
        )


if __name__ == "__main__":
    unittest.main()
