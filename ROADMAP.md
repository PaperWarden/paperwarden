# Roadmap

The authoritative product strategy is viewer-and-converter-first. Full Office
editing is deferred until after 1.0.

## M0 - Technical feasibility

- [ ] Generate and validate the Flutter Android/iOS workspace.
- [ ] Validate the Flutter-Rust bridge on Android and iOS.
- [ ] Prototype PDFium rendering on both platforms.
- [ ] Prototype qpdf operations and packaging.
- [ ] Investigate LibreOffice conversion feasibility separately for Android/iOS.
- [ ] Prototype Tesseract OCR and language-pack handling.
- [ ] Prototype scan correction with OpenCV/native/Rust options.
- [ ] Validate SQLite/FTS and the isolated temporary-workspace model.
- [ ] Measure binary size, memory, startup, and large-file behavior.
- [ ] Complete exact-build license/distribution review.

## Product phases

0. Constitution and build guardrails.
1. Core app, files, and safe processing.
2. Universal viewer.
3. Universal converter.
4. PDF viewer and toolbox.
5. Mobile scanner.
6. Offline OCR.
7. Privacy center and sanitization.
8. Security inspector.
9. Sensitive-data detection and true redaction.
10. Signatures and forms.
11. Batch engine and workflows.
12. Local search and file intelligence.
13. Accessibility, localization, and mobile UX.
14. Security hardening and release engineering.
15. PaperWarden 1.0 release gate.

See the source product roadmap for detailed outcomes and acceptance gates.

