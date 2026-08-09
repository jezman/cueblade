//! cueblade library crate.
//!
//! Provides CUE parsing, audio codec pipelines, discovery engine,
//! safety primitives, and sanitization for the cueblade CLI tool.

pub mod cli;
pub mod codec;
pub mod cue;
pub mod discovery;
pub mod error;
pub mod safety;
