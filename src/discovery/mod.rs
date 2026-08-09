//! Discovery engine for locating audio + CUE source pairs.
//!
//! Supports explicit mode (Phase 1) and will add auto-discover,
//! recursive walk, and exclude filtering in Phase 2.

pub mod explicit;

pub use explicit::{SourceGroup, discover_explicit};
