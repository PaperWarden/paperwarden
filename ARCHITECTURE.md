# Architecture

## Context

PaperWarden processes attacker-controlled documents on mobile devices with no
production backend. The architecture prioritizes isolation, bounded resource use,
verifiable outputs, portability, and reproducible releases.

## Layers

| Layer | Technology | Responsibility |
| --- | --- | --- |
| UI | Flutter + Dart | Navigation, viewer controls, tools, settings, responsive UI |
| State | Riverpod | Local feature and job state |
| Core | Rust | Validation, job orchestration, metadata, security, conversion, search |
| Bridge | flutter_rust_bridge / FFI | Narrow, generated Flutter-to-Rust contract |
| PDF rendering | PDFium candidate | Pages, thumbnails, search/text where supported |
| PDF structure | qpdf candidate | Structural transforms, encryption, merge/split |
| Office conversion | LibreOffice components candidate | Local preview/conversion where packaging is viable |
| OCR | Tesseract candidate | Offline recognition |
| Image processing | OpenCV/native/Rust candidates | Crop, perspective, deskew, cleanup |
| Local data | SQLite + FTS candidate | Settings, job history, workflows, optional local index |
| Android | Kotlin | SAF, share sheet, camera, platform integration |
| iOS | Swift | Files, share sheet, camera, platform integration |

Candidates remain provisional until the technical-feasibility milestone proves
mobile packaging, licensing, performance, and binary-size impact.

## Processing boundary

```text
selected file
  -> validate
  -> isolated temporary workspace
  -> process locally with bounded resources
  -> verify output
  -> explicit save/export
  -> cleanup
```

The Rust core owns this lifecycle. Feature modules must not invent independent
temporary-file or export behavior.

## Repository boundaries

- UI code does not parse hostile document structures.
- Native adapters expose narrow platform capabilities to the core/application.
- Processing engines are wrapped behind traits and capability checks.
- Generated bridge artifacts are reproducible and checked in CI once introduced.
- Optional local indexing is opt-in, clearable, and never a hidden copy of originals.

## Network posture

The 1.0 product has no network-dependent path. Build and release tooling may use
the network in CI; production application code may not silently gain that ability.

