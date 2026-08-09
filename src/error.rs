//! Error taxonomy for cueblade.
//!
//! All errors are classified to enable structured logging,
//! machine-readable JSON output, and correct exit codes.

use std::path::PathBuf;
use thiserror::Error;

/// Top-level error type for cueblade operations.
#[derive(Debug, Error)]
pub enum CueBladeError {
    /// CUE sheet parsing failed.
    #[error("CUE parse error at byte {byte_offset}, line {line}: {message}")]
    CueParse {
        byte_offset: usize,
        line: usize,
        message: String,
    },

    /// Input exceeds safety limits.
    #[error("Input validation failed: {reason}")]
    InputValidation { reason: String },

    /// Encoding detection or conversion failed.
    #[error("Encoding error: {message}")]
    Encoding { message: String },

    /// Arithmetic overflow in sample/timecode calculations.
    #[error("Arithmetic overflow in {operation}")]
    Arithmetic { operation: String },

    /// File I/O error with context.
    #[error("I/O error on `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Catch-all for other errors.
    #[error("{0}")]
    Other(String),
}

/// Result alias using [`CueBladeError`].
pub type Result<T> = std::result::Result<T, CueBladeError>;
