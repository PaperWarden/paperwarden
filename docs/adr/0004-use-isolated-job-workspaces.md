# ADR-0004: Use isolated job workspaces

**Status:** Accepted  
**Date:** 2026-09-06  
**Deciders:** PaperWarden maintainers

## Context

Converters and parsers create intermediate data, can fail or be cancelled, and
must never silently damage originals or leak residual content.

## Decision

Every operation follows a core-owned job state machine and runs in a unique local
temporary workspace. Export occurs only after verification; cleanup is mandatory
after success, failure, cancellation, and stale-workspace recovery.

## Consequences

- Features cannot manage temporary files ad hoc.
- Recovery and lifecycle tests are release-blocking.
- The core must make cleanup idempotent and observable without telemetry.

