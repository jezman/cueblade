//! CUE sheet parsing, encoding detection, sanitization, and data types.
//!
//! This module provides a safe CUE parser built on `winnow`
//! with automatic UTF-8/CP1251 encoding support. Parsing is purely
//! syntactic; semantic sanitization is performed by [`sanitizer::sanitize()`]
//! before downstream processing.

pub mod encoding;
pub mod parser;
pub mod sanitizer;
pub mod types;

pub use parser::parse_cue;
pub use sanitizer::{SanitizedCue, sanitize};
pub use types::{CueSheet, FileType, Index, Timecode, Track};
