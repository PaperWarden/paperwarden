//! PaperWarden's local processing boundary.
//!
//! The core owns hostile-input validation, job orchestration, isolated
//! workspaces, resource bounds and cleanup (`ARCHITECTURE.md`). Format engines
//! plug into the job lifecycle only after feasibility work proves them; none
//! are selected here.
//!
//! # Module boundaries
//!
//! - [`cancel`]: cooperative cancellation shared between the UI and workers.
//! - [`job`]: job identifiers and the lifecycle state machine. Pure logic, no
//!   I/O.
//! - [`workspace`]: the only module that creates, cleans or recovers on-disk
//!   job workspaces.
//!
//! `core/README.md` lists the planned boundaries and the rules between them.
//! The crate performs no network I/O and never spawns processes; the lint
//! policy in `core/clippy.toml` enforces both.

pub mod cancel;
pub mod job;
pub mod workspace;

pub use cancel::{CancellationToken, Cancelled};
pub use job::{Job, JobId, JobState};
pub use workspace::Workspace;
