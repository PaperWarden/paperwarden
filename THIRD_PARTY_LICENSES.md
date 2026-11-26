# Third-Party Licenses

No production processing engine has been selected or shipped yet.

Before adopting a dependency, record its exact package/build, version, source,
license/SPDX identifier, copyright notice, distribution obligations, platform,
and whether it processes attacker-controlled content.

Priority feasibility reviews: PDFium, qpdf, LibreOffice components, Tesseract,
OpenCV, flutter_rust_bridge, SQLite, and their transitive/native dependencies.

The machine-readable inventory is `third_party/dependencies.json`. CI requires
every locked Cargo and Dart/Flutter package to have an exact version, source,
scope, and approved SPDX license identifier in that inventory. Native engines
must not be added until their redistribution obligations are reviewed here.

## PDFium (feasibility review, PW-010; not yet shipped)

- **Build:** `bblanchon/pdfium-binaries` `chromium/8125` (PDFium 158.0.8125.0),
  non-V8, non-XFA; per-asset SHA-256 pinned in `scripts/fetch_pdfium.py`.
- **Processes attacker-controlled content:** yes, every PDF the user opens.
- **PDFium:** BSD-3-Clause (some files Apache-2.0).
- **Bundled in the library:**

  | Component | License |
  | --- | --- |
  | FreeType | FreeType License; GPL-2.0 alternative not elected |
  | HarfBuzz | MIT ("Old MIT") |
  | ICU | Unicode-3.0 |
  | libjpeg-turbo | IJG, BSD-3-Clause and Zlib |
  | libpng | libpng-2.0 |
  | zlib | Zlib |
  | OpenJPEG | BSD-2-Clause |
  | Little CMS | MIT |
  | Anti-Grain Geometry 2.3 | permissive AGG license |
  | Abseil, Highway, cpu_features | Apache-2.0 |
  | dragonbox, libunwind, llvm-libc | Apache-2.0 WITH LLVM-exception (dragonbox also Boost-1.0) |
  | fast_float, simdutf | MIT |
  | Catapult | BSD-3-Clause |

  The GPLv3 text in ICU's notice is the Autoconf exception for build scripts
  that are not part of the library.
- **Obligations:** ship every notice from the package's `licenses/` directory in
  the app's open-source notices. Include the FreeType credit ("Portions of this
  software are copyright The FreeType Project (www.freetype.org)") and the IJG
  statement ("This software is based in part on the work of the Independent
  JPEG Group").
- **Rust binding:** `pdfium-render` 0.9.6 (MIT OR Apache-2.0) and its crates,
  recorded in `third_party/dependencies.json`.
