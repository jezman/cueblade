//! Processing pipeline components.
//!
//! Contains overwrite policy, task planning, worker logic,
//! and result aggregation. Currently implements overwrite
//! policy only; planner and workers are added in Phase 2.

pub mod overwrite;

pub use overwrite::OverwritePolicy;
