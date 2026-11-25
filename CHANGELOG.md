# Changelog

All notable changes will be documented here after the first release baseline.

## Unreleased

- Established the Phase 0 repository constitution and feasibility scaffold.
- Configured the Rust core as a Cargo workspace with a shared lint policy that
  forbids unsafe code, panics, network sockets and subprocesses in production
  code, plus documented module boundaries and test conventions.
