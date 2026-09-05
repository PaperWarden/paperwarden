# ADR-0002: Use a Rust processing core

**Status:** Accepted  
**Date:** 2026-09-06  
**Deciders:** PaperWarden maintainers

## Context

All features need the same hostile-input validation, isolated workspace, bounded
processing, output verification, export, and cleanup lifecycle.

## Decision

Use Rust for the reusable processing core. Expose a narrow asynchronous contract
to Flutter through a generated FFI bridge after the feasibility spike.

## Consequences

- File lifecycle and safety policy have one owner.
- Native libraries need reviewed Rust/native wrappers and mobile packaging.
- FFI compatibility becomes a required CI contract.

