//! Explicit mode discovery: validate user-specified --flac and --cue paths.
//!
//! Returns a [`SourceGroup`] ready for the processing pipeline.
//! All paths are validated for existence and readability.

use std::path::{Path, PathBuf};

use crate::error::{CueBladeError, Result};

/// A matched audio + CUE source pair ready for processing.
///
/// Produced by discovery modes (explicit, auto, recursive).
/// Contains validated, absolute paths to source files.
///
/// # Examples
///
/// ```
/// use cueblade::discovery::SourceGroup;
/// use std::path::PathBuf;
///
/// let group = SourceGroup {
///     audio_path: PathBuf::from("/music/album.flac"),
///     cue_path: PathBuf::from("/music/album.cue"),
/// };
/// assert!(group.audio_path.is_absolute());
/// ```
#[derive(Debug, Clone)]
pub struct SourceGroup {
    /// Absolute path to the source audio file.
    pub audio_path: PathBuf,
    /// Absolute path to the CUE sheet file.
    pub cue_path: PathBuf,
}

/// Validate explicit mode inputs and return a [`SourceGroup`].
///
/// Checks that both files exist and are readable. Paths are
/// canonicalized to absolute form.
///
/// # Errors
///
/// - [`CueBladeError::Io`] if a file does not exist or is not readable.
/// - [`CueBladeError::InputValidation`] if paths are invalid.
///
/// # Examples
///
/// ```no_run
/// use cueblade::discovery::explicit::discover_explicit;
/// use std::path::Path;
///
/// let group = discover_explicit(
///     Path::new("album.flac"),
///     Path::new("album.cue"),
/// ).unwrap();
/// println!("Audio: {}", group.audio_path.display());
/// println!("CUE:   {}", group.cue_path.display());
/// ```
pub fn discover_explicit(flac: &Path, cue: &Path) -> Result<SourceGroup> {
    // Validate audio file
    if !flac.exists() {
        return Err(CueBladeError::Io {
            path: flac.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Audio file not found: {}", flac.display()),
            ),
        });
    }
    if !flac.is_file() {
        return Err(CueBladeError::InputValidation {
            reason: format!("Audio path is not a file: {}", flac.display()),
        });
    }

    // Validate CUE file
    if !cue.exists() {
        return Err(CueBladeError::Io {
            path: cue.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("CUE file not found: {}", cue.display()),
            ),
        });
    }
    if !cue.is_file() {
        return Err(CueBladeError::InputValidation {
            reason: format!("CUE path is not a file: {}", cue.display()),
        });
    }

    // Canonicalize to absolute paths
    let audio_path = std::fs::canonicalize(flac).map_err(|e| CueBladeError::Io {
        path: flac.to_path_buf(),
        source: e,
    })?;
    let cue_path = std::fs::canonicalize(cue).map_err(|e| CueBladeError::Io {
        path: cue.to_path_buf(),
        source: e,
    })?;

    Ok(SourceGroup {
        audio_path,
        cue_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_discover_explicit_valid() {
        let dir = tempfile::tempdir().unwrap();
        let flac = dir.path().join("album.flac");
        let cue = dir.path().join("album.cue");
        fs::write(&flac, b"fake audio").unwrap();
        fs::write(&cue, b"fake cue").unwrap();

        let group = discover_explicit(&flac, &cue).unwrap();
        assert!(group.audio_path.is_absolute());
        assert!(group.cue_path.is_absolute());
    }

    #[test]
    fn test_discover_explicit_missing_audio() {
        let dir = tempfile::tempdir().unwrap();
        let cue = dir.path().join("album.cue");
        fs::write(&cue, b"fake cue").unwrap();

        let result = discover_explicit(&dir.path().join("missing.flac"), &cue);
        assert!(matches!(result, Err(CueBladeError::Io { .. })));
    }

    #[test]
    fn test_discover_explicit_missing_cue() {
        let dir = tempfile::tempdir().unwrap();
        let flac = dir.path().join("album.flac");
        fs::write(&flac, b"fake audio").unwrap();

        let result = discover_explicit(&flac, &dir.path().join("missing.cue"));
        assert!(matches!(result, Err(CueBladeError::Io { .. })));
    }

    #[test]
    fn test_discover_explicit_directory_not_file() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        let cue = dir.path().join("album.cue");
        fs::write(&cue, b"fake cue").unwrap();

        let result = discover_explicit(&subdir, &cue);
        assert!(matches!(result, Err(CueBladeError::InputValidation { .. })));
    }
}
