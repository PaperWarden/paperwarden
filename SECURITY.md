# Security Policy

## Project status and reporting

PaperWarden is pre-alpha and is not yet ready to accept untrusted production use.
Do not open public issues containing exploit details, private documents, personal
data, live credentials, or malicious samples. Use GitHub private vulnerability
reporting when it is enabled for the repository.

Supported release versions and a dedicated security contact will be published
before the first public test release.

## System and scope

This policy covers the PaperWarden mobile application, Rust processing core,
Flutter-Rust bridge, Kotlin and Swift adapters, document-processing integrations,
test corpus, CI workflows, dependency configuration, and release artifacts.

PaperWarden has no production backend, accounts, hosted storage, telemetry, or
required network service. Build infrastructure and GitHub Releases are outside the
runtime product boundary but remain inside the software supply-chain review scope.

## Threat model and trust boundaries

Documents, archives, filenames, metadata, embedded objects, links, fonts, images,
OCR packs, and imported workflows are attacker-controlled. Important boundaries
are the operating-system picker/share sheet, Flutter-to-Rust FFI, Rust-to-native or
third-party engines, isolated job workspaces, explicit export, and source-to-release
supply chain.

Important assets are document confidentiality and integrity, derived text and
metadata, device resources, verified outputs, signing material, and release
authenticity.

## Security invariants

- Production code does not upload documents or require network access.
- Document-provided macros, PDF JavaScript, and other active content are not
  executed; remote resources are not automatically fetched.
- Input type and structure are validated before engine dispatch.
- File size, decompressed size, archive depth, image dimensions, processing time,
  and memory are bounded where practical and fail closed.
- Archive paths are canonicalized; traversal, absolute paths, and unsafe links are
  rejected before extraction.
- Jobs use isolated temporary workspaces. Cleanup covers success, failure,
  cancellation, and stale-workspace recovery.
- Original files are not silently overwritten. Outputs are verified before
  explicit export.
- Redaction is not complete while removed content remains searchable or
  extractable.
- New dependencies and release changes receive supply-chain and license review.

## Reportable findings and severity context

Report vulnerabilities with realistic reachability that could cause code
execution, sandbox or path escape, unintended network transfer, sensitive-data
disclosure, original-file modification, persistent temporary-data exposure,
resource exhaustion from plausibly sized input, ineffective redaction or
sanitization, signature/verification bypass, or release-pipeline compromise.

Severity depends on whether a malicious file can trigger the issue without an
explicit high-risk user decision, the confidentiality or integrity impact, the
platform boundary crossed, persistence, exploit reliability, and whether the
failure violates a published product invariant.

## Out of scope and current limitations

- Unsupported or merely malformed files failing safely are not vulnerabilities.
- A requested feature being absent is not a vulnerability.
- Office editing, cloud collaboration, and malware classification are outside the
  1.0 product scope.
- PaperWarden is an inspector and safe-processing tool, not an antivirus product.
- Third-party engine limitations remain reportable when PaperWarden exposes them in
  a realistically exploitable way; dependency ownership alone is not a reason to
  suppress a finding.
- Denial-of-service claims need a reproducible case and a realistic input/device
  context; ordinary resource cost inherent to a user-requested conversion is not
  automatically a vulnerability.

No additional exclusions or accepted security risks are approved at this stage.

## Disclosure expectations

Provide a concise impact statement, affected platform/version, safe reproduction
steps, and the smallest redistributable fixture possible. Maintainers will confirm
receipt, assess severity, coordinate a fix and regression test, and credit the
reporter if requested and safe to do so.
