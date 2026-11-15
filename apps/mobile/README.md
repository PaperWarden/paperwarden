# PaperWarden mobile shell

Flutter owns PaperWarden's cross-platform UI. Android and iOS host projects are
committed so native configuration and permission changes remain reviewable.

The production Android manifest intentionally has no `INTERNET` permission.
Platform packaging decisions for the Rust bridge, PDFium, qpdf, Tesseract, and
OpenCV remain gated by the M0 feasibility issues.
