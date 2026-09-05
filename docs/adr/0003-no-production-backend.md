# ADR-0003: No production backend

**Status:** Accepted  
**Date:** 2026-09-06  
**Deciders:** PaperWarden maintainers

## Context

PaperWarden exists to perform common document tasks locally without transferring
sensitive content to online services.

## Decision

Version 1.0 has no PaperWarden backend, account system, hosted database, cloud
storage, conversion/OCR API, analytics, or required Internet path.

## Consequences

- Core functions work in airplane mode and recurring production cost is zero.
- Device constraints must be handled explicitly.
- Any future online feature requires a new ADR and product-invariant change.

