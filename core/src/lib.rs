//! PaperWarden's local processing boundary.
//!
//! Feature engines will plug into the job lifecycle after feasibility work. This
//! crate intentionally performs no network I/O.

pub mod job;

pub use job::{Job, JobId, JobState};
