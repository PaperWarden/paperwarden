//! PaperWarden's local processing boundary.
//!
//! The core owns hostile-input validation, job orchestration, isolated
//! workspaces, resource bounds and cleanup (`ARCHITECTURE.md`). Format engines
//! plug into the job lifecycle only after feasibility work proves them; none
//! are selected here.
//!
//! # Module boundaries
//!
//! - [`archive`]: archive entry-path rules; extraction arrives with its engine.
//! - [`cancel`]: cooperative cancellation shared between the UI and workers.
//! - [`job`]: job identifiers and the lifecycle state machine. Pure logic, no
//!   I/O.
//! - [`workspace`]: the only module that creates, cleans or recovers on-disk
//!   job workspaces.
//! - [`limits`]: per-job size and time bounds.
//! - [`validation`]: content-based input checks, never file names.
//! - [`engine`]: the seam processors and output verifiers plug into.
//! - [`runner`]: drives one job from validation to export and cleanup. Export
//!   itself lives in a private module that only ever creates new files.
//!
//! `core/README.md` lists the planned boundaries and the rules between them.
//! The crate performs no network I/O and never spawns processes; the lint
//! policy in `core/clippy.toml` enforces both.

pub mod archive;
pub mod cancel;
pub mod engine;
mod export;
pub mod job;
pub mod limits;
pub mod runner;
pub mod validation;
pub mod workspace;

pub use cancel::{CancellationToken, Cancelled};
pub use job::{Job, JobId, JobState};
pub use limits::JobLimits;
pub use runner::{JobError, JobReport, JobRunner, Operation, Outcome, ReadyJob};
pub use workspace::{RecoveryReport, Workspace};
