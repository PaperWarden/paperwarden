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

## qpdf (feasibility review, PW-012; not yet shipped)

- **Build:** compiled from upstream source by `core/crates/qpdf/build.rs`,
  with qpdf's native crypto only (no OpenSSL or GnuTLS). The release archives
  and their SHA-256 digests are pinned in `scripts/fetch_qpdf.py`.
- **Processes attacker-controlled content:** yes, every PDF a structure tool
  is given.

  | Component | Version | License | Source |
  | --- | --- | --- | --- |
  | qpdf | 12.4.2 | Apache-2.0 (Artistic-2.0 also offered for old versions; not elected) | github.com/qpdf/qpdf releases |
  | qpdf's Rijndael (`rijndael.cc`) | bundled | public domain | qpdf |
  | qpdf's SHA-2 (sphlib 3.0) | bundled | MIT | qpdf |
  | zlib | 1.3.2 | Zlib | github.com/madler/zlib releases |
  | IJG libjpeg | 9f | IJG | ijg.org |

- **Obligations:** ship qpdf's `LICENSE.txt` and `NOTICE.md` (Apache-2.0
  section 4), the sphlib MIT notice from `NOTICE.md`, the zlib license, and
  the IJG statement ("This software is based in part on the work of the
  Independent JPEG Group") in the app's open-source notices.
- **Rust binding:** none from crates.io. PaperWarden declares the handful of
  C functions it calls (`core/crates/qpdf/src/ffi.rs`); the crate's only
  dependency beyond the workspace is `cc` (build-only, already inventoried).
