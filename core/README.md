# PaperWarden core

The Rust processing core. It owns hostile-input validation, job orchestration,
isolated workspaces, resource bounds and cleanup (`ARCHITECTURE.md`,
ADR-0002). Flutter owns the UI; Kotlin and Swift stay thin platform adapters.

## Workspace layout

`core/` is both the Cargo workspace root and its first member,
`paperwarden-core`. Future crates, such as the FFI bridge, go under
`core/crates/<name>`, are added to `members` in `Cargo.toml`, and opt into the
shared lint policy with `[lints] workspace = true`. The fuzz harnesses in
`/fuzz` stay outside the workspace, as `cargo-fuzz` expects.

## Module boundaries

| Module | Owns | Must not |
| --- | --- | --- |
| `cancel` | Cooperative cancellation tokens shared by the UI and workers | Clean up; the job owner does |
| `job` | Job identifiers and the lifecycle state machine | Perform I/O |
| `workspace` | Creating, cleaning and recovering on-disk job workspaces | Know about formats or engines |

Planned boundaries are created by their own issue, not ahead of time:

| Planned module | Purpose | Invariants |
| --- | --- | --- |
| `validation` | Type and structure checks from magic bytes, never filenames | 10 |
| `limits` | Size, decompression, nesting, dimension, time and memory bounds | 11 |
| `archive` | Safe extraction: traversal, absolute paths, links, bombs | 11 |
| `engine` | Trait seam for processors; engines stay provisional until proven | 6, 12 |
| `export` | Explicit save to a new destination after verification | 8 |
| `security`, `privacy`, `pdf` | Reserved; `CODEOWNERS` requires core-team review | — |

Rules between modules:

- Only `workspace` creates or deletes job directories. Features never manage
  temporary files themselves (ADR-0004).
- Engines are reached only through the `engine` seam, never called directly by
  feature code, so a provisional engine can be replaced without touching callers.
- Nothing in the core performs network I/O or launches processes.

## Lint policy

Defined once in `[workspace.lints]` in `Cargo.toml`; CI runs clippy with
`-D warnings`, so every warning blocks a merge.

| Rule | Why |
| --- | --- |
| `unsafe_code = "forbid"` | Memory safety for code that parses hostile input. A future FFI crate does not inherit this and must justify its own policy. |
| `unwrap_used`, `expect_used`, `panic`, `indexing_slicing` denied | A malformed document must produce an error, never a panic. Use `get`, `?` and typed errors. |
| `todo`, `unimplemented`, `dbg_macro` denied | No unfinished or debugging code reaches `main`. |
| `print_stdout`, `print_stderr`, `exit` denied | The core reports through return values. |
| `mem_forget` denied | A forgotten guard skips workspace cleanup (invariant 9). |
| `disallowed_types`, `disallowed_methods` denied | `clippy.toml` bans `std::net` sockets and `std::process::Command::new` (invariants 1, 4, 6, 7). |
| `missing_docs` | Every public item states its contract, including `# Errors`. |

Tests may `unwrap`, `expect`, index and panic; `clippy.toml` allows this only in
test code. Any other exception needs an item-level
`#[allow(lint, reason = "...")]` that a reviewer can see, never a crate-wide one.

## Test conventions

- Unit tests sit beside the code in a `#[cfg(test)] mod tests` and may use
  private items.
- Integration tests in `core/tests/` use only the public API, as Flutter will.
- Shared integration helpers live in `core/tests/support/mod.rs` and are
  declared with `#[cfg(test)] mod support;` so clippy treats them as test code.
- Every test owns a unique temporary root (`support::unique_root`) and removes
  it, so tests run in parallel and leave nothing behind.
- Name tests for the behaviour they prove, for example
  `failed_job_workspace_is_removed_when_dropped`.
- Cover failure, cancellation, malformed input and cleanup, not only success
  (`ENGINEERING_STANDARDS.md`).

## Commands

Run from `core/`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --locked
```
