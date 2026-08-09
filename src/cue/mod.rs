//! CUE sheet parsing, encoding detection, and data types.
//!
//! This module provides a safe CUE parser built on `winnow`
//! with automatic UTF-8/CP1251 encoding support. Parsing is purely
//! syntactic; semantic sanitization will be implemented in a
//! dedicated `sanitizer` module in a subsequent phase.

pub mod encoding;
pub mod parser;
pub mod types;

pub use parser::parse_cue;
pub use types::{CueSheet, FileType, Index, Timecode, Track};
