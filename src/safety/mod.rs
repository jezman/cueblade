//! Safety primitives for file system operations.
//!
//! Provides atomic writes with fsync guarantees, path validation,
//! and future quota/lock utilities per SECURITY.md and DD-003.

pub mod atomic;

pub use atomic::AtomicWriter;
