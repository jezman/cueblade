//! Overwrite policy for existing output files.
//!
//! Determines whether to skip, overwrite, or conditionally write
//! based on file existence and modification times.

use std::path::Path;

use crate::cli::OverwriteMode;
use crate::error::Result;

/// Decision made by the overwrite policy check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteDecision {
    /// Proceed with writing (file doesn't exist or overwrite allowed).
    Write,
    /// Skip this track (file exists and policy says skip).
    Skip,
}

/// Overwrite policy checker.
///
/// Evaluates whether an output file should be written based on
///
/// # Examples
///
/// ```
/// use cueblade::pipeline::overwrite::{OverwritePolicy, OverwriteDecision};
/// use cueblade::cli::OverwriteMode;
/// use std::path::Path;
///
/// let policy = OverwritePolicy::new(OverwriteMode::Skip);
/// // Non-existent file → always Write regardless of mode
/// assert_eq!(
///     policy.check(Path::new("/nonexistent/file.flac"), Path::new("/source.flac")).unwrap(),
///     OverwriteDecision::Write
/// );
/// ```
pub struct OverwritePolicy {
    mode: OverwriteMode,
}

impl OverwritePolicy {
    /// Create a new overwrite policy with the given mode.
    pub fn new(mode: OverwriteMode) -> Self {
        Self { mode }
    }

    /// Check whether the output file should be written.
    ///
    /// `output_path`: target file path.
    /// `source_path`: source audio file (used for `Newer` comparison).
    ///
    /// # Errors
    ///
    /// the configured [`OverwriteMode`] and filesystem state.
    /// Returns [`crate::error::CueBladeError::Io`] if metadata cannot be read
    /// for `Newer` mode comparison.
    pub fn check(&self, output_path: &Path, source_path: &Path) -> Result<OverwriteDecision> {
        if !output_path.exists() {
            return Ok(OverwriteDecision::Write);
        }

        match self.mode {
            OverwriteMode::Skip => Ok(OverwriteDecision::Skip),
            OverwriteMode::Overwrite => Ok(OverwriteDecision::Write),
            OverwriteMode::Newer => {
                let source_meta = std::fs::metadata(source_path).map_err(|e| {
                    crate::error::CueBladeError::Io {
                        path: source_path.to_path_buf(),
                        source: e,
                    }
                })?;
                let output_meta = std::fs::metadata(output_path).map_err(|e| {
                    crate::error::CueBladeError::Io {
                        path: output_path.to_path_buf(),
                        source: e,
                    }
                })?;

                let source_mtime =
                    source_meta
                        .modified()
                        .map_err(|e| crate::error::CueBladeError::Io {
                            path: source_path.to_path_buf(),
                            source: e,
                        })?;
                let output_mtime =
                    output_meta
                        .modified()
                        .map_err(|e| crate::error::CueBladeError::Io {
                            path: output_path.to_path_buf(),
                            source: e,
                        })?;

                if source_mtime > output_mtime {
                    Ok(OverwriteDecision::Write)
                } else {
                    Ok(OverwriteDecision::Skip)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_skip_nonexistent_file() {
        let policy = OverwritePolicy::new(OverwriteMode::Skip);
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("missing.flac");
        let source = dir.path().join("source.flac");
        fs::write(&source, b"data").unwrap();

        assert_eq!(
            policy.check(&output, &source).unwrap(),
            OverwriteDecision::Write
        );
    }

    #[test]
    fn test_skip_existing_file() {
        let policy = OverwritePolicy::new(OverwriteMode::Skip);
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("existing.flac");
        let source = dir.path().join("source.flac");
        fs::write(&output, b"old").unwrap();
        fs::write(&source, b"data").unwrap();

        assert_eq!(
            policy.check(&output, &source).unwrap(),
            OverwriteDecision::Skip
        );
    }

    #[test]
    fn test_overwrite_existing_file() {
        let policy = OverwritePolicy::new(OverwriteMode::Overwrite);
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("existing.flac");
        let source = dir.path().join("source.flac");
        fs::write(&output, b"old").unwrap();
        fs::write(&source, b"data").unwrap();

        assert_eq!(
            policy.check(&output, &source).unwrap(),
            OverwriteDecision::Write
        );
    }

    #[test]
    fn test_newer_source_is_newer() {
        let policy = OverwritePolicy::new(OverwriteMode::Newer);
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("existing.flac");
        let source = dir.path().join("source.flac");

        // Write output first (older), then source (newer)
        fs::write(&output, b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&source, b"new").unwrap();

        assert_eq!(
            policy.check(&output, &source).unwrap(),
            OverwriteDecision::Write
        );
    }

    #[test]
    fn test_newer_source_is_older() {
        let policy = OverwritePolicy::new(OverwriteMode::Newer);
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.flac");
        let output = dir.path().join("existing.flac");

        // Write source first (older), then output (newer)
        fs::write(&source, b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&output, b"new").unwrap();

        assert_eq!(
            policy.check(&output, &source).unwrap(),
            OverwriteDecision::Skip
        );
    }
}
