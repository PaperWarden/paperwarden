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
