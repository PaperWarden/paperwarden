# Threat Model

**Status:** Initial Phase 0 model. Review after each feasibility spike.

## Assets

- Original documents and their confidentiality and integrity.
- Extracted text, images, metadata, credentials, and signatures.
- Device storage, memory, CPU, battery, and application availability.
- Exported output correctness and authenticity.
- Release artifacts and signing material.

## Adversaries and untrusted input

Any document, archive, filename, metadata field, embedded object, link, font,
image, OCR language pack, or imported workflow can be attacker-controlled.

## Trust boundaries

1. Operating-system file picker/share sheet to PaperWarden.
2. Flutter UI to the Rust FFI boundary.
3. Rust orchestration to native and third-party processing engines.
4. Isolated temporary workspace to explicit user export.
5. Source repository and CI to signed release artifacts.

## Required controls

- Validate type and structure before dispatch.
- Bound file size, decompressed size, nesting, dimensions, time, and memory.
- Canonicalize archive paths and reject traversal, absolute paths, and unsafe links.
- Never execute active document content or automatically resolve remote resources.
- Use new output files, verify outputs, and clean all temporary data.
- Treat engine crashes and malformed outputs as failed jobs.
- Keep release credentials out of source and restrict them to protected release jobs.

## High-impact failure cases

- Document disclosure or unintended network transmission.
- Arbitrary code execution through a parser or active content.
- Write outside the workspace or overwrite of an original.
- Persistent recovery of sensitive temporary data.
- Decompression/resource exhaustion causing device denial of service.
- Cosmetic-only redaction that leaves extractable content.
- Compromised dependency or release pipeline.

## Known unknowns

Sandboxing strength, native engine packaging, engine update cadence, cryptographic
signing, and user-supplied language packs remain unresolved until feasibility work.

