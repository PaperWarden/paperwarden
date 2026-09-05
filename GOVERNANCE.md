# Governance

PaperWarden begins with two maintainers. Issue ownership may rotate; review does
not. The implementing maintainer must not be the sole approver of their change.

Sensitive areas include CI/release configuration, archive handling, parsers,
privacy, redaction, cryptography, dependencies, and licensing. Changes in these
areas require explicit review by the other maintainer.

Architecture decisions are recorded in `docs/adr`. Product-invariant changes need
agreement from both maintainers and must be visible in the pull request.

