//! Error taxonomy for cueblade.
//!
//! All errors are classified to enable structured logging,
//! correct exit codes, and future machine-readable JSON output.
//! Each variant maps to a deterministic exit code per SECURITY.md.

use std::path::PathBuf;
use thiserror::Error;

/// Top-level error type for cueblade operations.
///
/// Each variant maps to a specific exit code via [`exit_code()`](CueBladeError::exit_code).
#[derive(Debug, Error)]
pub enum CueBladeError {
    /// CUE sheet parsing failed.
    #[error("CUE parse error at byte {byte_offset}, line {line}: {message}")]
    CueParse {
        byte_offset: usize,
        line: usize,
        message: String,
    },

    /// Input exceeds safety limits or fails validation.
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

    /// Referenced audio file not found after fallback chain.
    #[error("Audio file not found: `{path}` (tried: {tried:?})")]
    FileNotFound { path: String, tried: Vec<String> },

    /// CUE sanitization failed.
    #[error("CUE sanitization error: {reason}")]
    Sanitization { reason: String },

    /// Catch-all for other errors.
    #[error("{0}")]
    Other(String),
}

impl CueBladeError {
    /// Return the deterministic exit code for this error variant.
    ///
    /// Exit codes are stable and documented:
    ///
    /// | Code | Meaning                        | Variants                              |
    /// |------|--------------------------------|---------------------------------------|
    /// | 1    | Generic / unclassified         | `Other`                               |
    /// | 3    | CUE parse failure              | `CueParse`                            |
    /// | 4    | Encoding error                 | `Encoding`                            |
    /// | 5    | I/O error                      | `Io`                                  |
    /// | 6    | Input validation / sanitization| `InputValidation`, `Sanitization`     |
    /// | 7    | Arithmetic overflow            | `Arithmetic`                          |
    /// | 8    | File not found                 | `FileNotFound`                        |
    ///
    /// Note: exit code `0` = success (no error), `2` = CLI argument error
    /// (handled separately in main before `CueBladeError` is constructed).
    ///
    /// # Examples
    ///
    /// ```
    /// use cueblade::error::CueBladeError;
    ///
    /// let err = CueBladeError::Other("test".into());
    /// assert_eq!(err.exit_code(), 1);
    ///
    /// let err = CueBladeError::CueParse {
    ///     byte_offset: 0,
    ///     line: 1,
    ///     message: "bad".into(),
    /// };
    /// assert_eq!(err.exit_code(), 3);
    /// ```
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Other(_) => 1,
            Self::CueParse { .. } => 3,
            Self::Encoding { .. } => 4,
            Self::Io { .. } => 5,
            Self::InputValidation { .. } | Self::Sanitization { .. } => 6,
            Self::Arithmetic { .. } => 7,
            Self::FileNotFound { .. } => 8,
        }
    }
}
///
/// Result alias using [`CueBladeError`].
pub type Result<T> = std::result::Result<T, CueBladeError>;

impl From<std::io::Error> for CueBladeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::from("<unknown>"),
            source: e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_other() {
        let err = CueBladeError::Other("test".into());
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn test_exit_code_cue_parse() {
        let err = CueBladeError::CueParse {
            byte_offset: 42,
            line: 5,
            message: "unexpected token".into(),
        };
        assert_eq!(err.exit_code(), 3);
    }

    #[test]
    fn test_exit_code_encoding() {
        let err = CueBladeError::Encoding {
            message: "invalid sequence".into(),
        };
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn test_exit_code_io() {
        let err = CueBladeError::Io {
            path: PathBuf::from("/tmp/test"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "gone"),
        };
        assert_eq!(err.exit_code(), 5);
    }

    #[test]
    fn test_exit_code_input_validation() {
        let err = CueBladeError::InputValidation {
            reason: "too large".into(),
        };
        assert_eq!(err.exit_code(), 6);
    }

    #[test]
    fn test_exit_code_sanitization() {
        let err = CueBladeError::Sanitization {
            reason: "bad timestamp".into(),
        };
        assert_eq!(err.exit_code(), 6);
    }

    #[test]
    fn test_exit_code_arithmetic() {
        let err = CueBladeError::Arithmetic {
            operation: "overflow".into(),
        };
        assert_eq!(err.exit_code(), 7);
    }

    #[test]
    fn test_exit_code_file_not_found() {
        let err = CueBladeError::FileNotFound {
            path: "album.flac".into(),
            tried: vec!["album.flac".into()],
        };
        assert_eq!(err.exit_code(), 8);
    }

    #[test]
    fn test_display_cue_parse() {
        let err = CueBladeError::CueParse {
            byte_offset: 100,
            line: 10,
            message: "expected TRACK".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("line 10"));
        assert!(msg.contains("byte 100"));
        assert!(msg.contains("expected TRACK"));
    }

    #[test]
    fn test_display_io() {
        let err = CueBladeError::Io {
            path: PathBuf::from("/music/album.flac"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied"),
        };
        let msg = format!("{err}");
        assert!(msg.contains("/music/album.flac"));
        assert!(msg.contains("access denied"));
    }
}
