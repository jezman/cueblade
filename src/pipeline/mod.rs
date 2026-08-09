//! Processing pipeline components.
//!
//! Contains overwrite policy, gap handling, explicit mode pipeline,
//! task planning, worker logic, and result aggregation.

pub mod explicit;
pub mod gap;
pub mod overwrite;

pub use explicit::run_explicit;
pub use gap::{TrackRange, calculate_track_ranges};
pub use overwrite::OverwritePolicy;
