//! Semantic sanitization of parsed CUE sheets.
//!
//! Performs timestamp repair, FILE fallback chain resolution,
//! and encoding post-validation. Produces a [`SanitizedCue`]
//! with guaranteed invariants for downstream processing.

use std::path::{Path, PathBuf};

use super::types::{CueSheet, FileType};
use crate::error::{CueBladeError, Result};

/// Fallback extensions for FILE directive resolution (DD-004, SECURITY.md).
const FILE_FALLBACK_EXTENSIONS: &[&str] = &[".flac", ".ape", ".wav", ".wv"];

/// A sanitized CUE sheet with guaranteed semantic invariants.
///
/// Obtained via [`sanitize()`]. All timestamps are monotonically
/// increasing within each track, FILE references are resolved
/// to existing paths, and string fields are valid UTF-8.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use cueblade::cue::sanitizer::SanitizedCue;
/// use cueblade::cue::types::{CueSheet, FileType};
///
/// let cue = CueSheet {
///     performer: None,
///     title: None,
///     file: "album.flac".into(),
///     file_type: FileType::Flac,
///     tracks: vec![],
///     rem_comments: vec![],
/// };
/// // SanitizedCue wraps a validated CueSheet
/// let sanitized = SanitizedCue::from_validated(cue, PathBuf::from("/tmp/album.flac"));
/// assert_eq!(sanitized.cue().file, "album.flac");
/// ```
#[derive(Debug, Clone)]
pub struct SanitizedCue {
    inner: CueSheet,
    resolved_path: PathBuf,
}

impl SanitizedCue {
    /// Create from an already-validated CUE sheet and resolved audio path.
    ///
    /// This constructor assumes all invariants hold. Use [`sanitize()`]
    /// for untrusted input.
    pub fn from_validated(cue: CueSheet, resolved_path: PathBuf) -> Self {
        Self {
            inner: cue,
            resolved_path,
        }
    }

    /// Access the underlying sanitized [`CueSheet`].
    pub fn cue(&self) -> &CueSheet {
        &self.inner
    }

    /// Resolved absolute path to the source audio file.
    pub fn resolved_audio_path(&self) -> &Path {
        &self.resolved_path
    }

    /// Consume and return the inner [`CueSheet`].
    pub fn into_inner(self) -> CueSheet {
        self.inner
    }
}

/// Sanitize a parsed [`CueSheet`] relative to a base directory.
///
/// # Operations
///
/// 1. **Timestamp repair**: clamp negative values, enforce monotonicity
///    within each track, remove duplicate INDEX 01 entries.
/// 2. **FILE fallback chain**: resolve the FILE directive against
///    `base_dir`, trying fallback extensions if the original not found.
/// 3. **Encoding post-check**: verify all string fields are valid UTF-8
///    without replacement characters.
///
/// # Errors
///
/// - [`CueBladeError::FileNotFound`] if audio file cannot be resolved.
/// - [`CueBladeError::Sanitization`] for unrecoverable semantic issues.
///
/// # Examples
///
/// ```no_run
/// use cueblade::cue::parser::parse_cue;
/// use cueblade::cue::sanitizer::sanitize;
/// use std::path::Path;
///
/// let bytes = std::fs::read("album.cue").unwrap();
/// let cue = parse_cue(&bytes).unwrap();
/// let sanitized = sanitize(cue, Path::new(".")).unwrap();
/// println!("Resolved: {}", sanitized.resolved_audio_path().display());
/// ```
pub fn sanitize(cue: CueSheet, base_dir: &Path) -> Result<SanitizedCue> {
    // 1. Encoding post-check
    check_encoding(&cue)?;

    // Reject empty track lists
    if cue.tracks.is_empty() {
        return Err(CueBladeError::Sanitization {
            reason: "CUE sheet contains no tracks".into(),
        });
    }

    // 2. Timestamp repair
    let mut repaired = cue.clone();
    repair_timestamps(&mut repaired);

    // 3. FILE fallback chain
    let resolved_path = resolve_file(&repaired.file, repaired.file_type, base_dir)?;

    Ok(SanitizedCue::from_validated(repaired, resolved_path))
}

// ─── Internal helpers ────────────────────────────────────────────────

/// Verify all string fields contain valid UTF-8 without U+FFFD.
fn check_encoding(cue: &CueSheet) -> Result<()> {
    let has_replacement = |s: &str| s.contains('\u{FFFD}');

    if let Some(ref p) = cue.performer {
        if has_replacement(p) {
            return Err(CueBladeError::Sanitization {
                reason: format!(
                    "Global PERFORMER contains invalid UTF-8 replacement characters: {p:?}"
                ),
            });
        }
    }
    if let Some(ref t) = cue.title {
        if has_replacement(t) {
            return Err(CueBladeError::Sanitization {
                reason: format!(
                    "Global TITLE contains invalid UTF-8 replacement characters: {t:?}"
                ),
            });
        }
    }
    for track in &cue.tracks {
        if let Some(ref t) = track.title {
            if has_replacement(t) {
                return Err(CueBladeError::Sanitization {
                    reason: format!(
                        "Track {} TITLE contains invalid UTF-8 replacement characters: {t:?}",
                        track.number
                    ),
                });
            }
        }
        if let Some(ref p) = track.performer {
            if has_replacement(p) {
                return Err(CueBladeError::Sanitization {
                    reason: format!(
                        "Track {} PERFORMER contains invalid UTF-8 replacement characters: {p:?}",
                        track.number
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Repair timestamps in-place: enforce monotonicity, clamp negatives,
/// deduplicate INDEX 01.
fn repair_timestamps(cue: &mut CueSheet) {
    for track in &mut cue.tracks {
        if track.indices.is_empty() {
            continue;
        }

        // Sort indices by number to ensure deterministic processing
        track.indices.sort_by_key(|idx| idx.number);

        // Deduplicate INDEX 01: keep only the first occurrence
        let mut seen_01 = false;
        track.indices.retain(|idx| {
            if idx.number == 1 {
                if seen_01 {
                    false // remove duplicate
                } else {
                    seen_01 = true;
                    true
                }
            } else {
                true
            }
        });

        // Enforce monotonicity: each index must be >= previous
        let mut prev_frames: u64 = 0;
        for idx in &mut track.indices {
            let frames = idx.timestamp.frames();
            if frames < prev_frames {
                // Clamp to previous value (timestamp repair per SECURITY.md)
                idx.timestamp = super::types::Timecode::from_msf(
                    prev_frames / (75 * 60),
                    (prev_frames / 75) % 60,
                    prev_frames % 75,
                )
                .unwrap_or(idx.timestamp);
            } else {
                prev_frames = frames;
            }
        }
    }
}

/// Resolve FILE directive against base_dir with extension fallback chain.
fn resolve_file(filename: &str, file_type: FileType, base_dir: &Path) -> Result<PathBuf> {
    let mut tried: Vec<String> = Vec::new();

    // Try exact filename first
    let exact = base_dir.join(filename);
    tried.push(exact.display().to_string());
    if exact.is_file() {
        return Ok(exact);
    }

    // Extract stem (without extension) for fallback
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    // Determine priority order based on declared file type
    let mut extensions: Vec<&str> = FILE_FALLBACK_EXTENSIONS.to_vec();
    let primary_ext = match file_type {
        FileType::Flac => ".flac",
        FileType::Ape => ".ape",
        FileType::Wav => ".wav",
        FileType::WavPack => ".wv",
        _ => "",
    };
    if !primary_ext.is_empty() {
        // Move primary extension to front
        extensions.retain(|&e| e != primary_ext);
        extensions.insert(0, primary_ext);
    }

    // Try each fallback extension
    for ext in &extensions {
        let candidate = base_dir.join(format!("{stem}{ext}"));
        tried.push(candidate.display().to_string());
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(CueBladeError::FileNotFound {
        path: filename.to_owned(),
        tried,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue::types::{CueSheet, FileType, Index, Timecode, Track};

    fn make_cue(file: &str, tracks: Vec<Track>) -> CueSheet {
        CueSheet {
            performer: None,
            title: None,
            file: file.into(),
            file_type: FileType::Flac,
            tracks,
            rem_comments: vec![],
        }
    }

    fn make_track(number: u16, indices: Vec<Index>) -> Track {
        Track {
            number,
            track_type: "AUDIO".into(),
            title: None,
            performer: None,
            indices,
            isrc: None,
        }
    }

    #[test]
    fn test_repair_monotonicity() {
        let mut cue = make_cue(
            "test.flac",
            vec![make_track(
                1,
                vec![
                    Index {
                        number: 0,
                        timestamp: Timecode::from_msf(0, 0, 0).unwrap(),
                    },
                    Index {
                        number: 1,
                        timestamp: Timecode::from_msf(0, 0, 10).unwrap(),
                    },
                    // Non-monotonic: should be clamped to >= 10
                    Index {
                        number: 2,
                        timestamp: Timecode::from_msf(0, 0, 5).unwrap(),
                    },
                ],
            )],
        );

        repair_timestamps(&mut cue);

        let indices = &cue.tracks[0].indices;
        assert!(indices[1].timestamp.frames() <= indices[2].timestamp.frames());
    }

    #[test]
    fn test_repair_dedup_index_01() {
        let mut cue = make_cue(
            "test.flac",
            vec![make_track(
                1,
                vec![
                    Index {
                        number: 1,
                        timestamp: Timecode::from_msf(0, 0, 0).unwrap(),
                    },
                    Index {
                        number: 1,
                        timestamp: Timecode::from_msf(0, 1, 0).unwrap(),
                    },
                ],
            )],
        );

        repair_timestamps(&mut cue);

        let count_01 = cue.tracks[0]
            .indices
            .iter()
            .filter(|i| i.number == 1)
            .count();
        assert_eq!(count_01, 1);
    }

    #[test]
    fn test_check_encoding_valid() {
        let cue = make_cue("test.flac", vec![]);
        assert!(check_encoding(&cue).is_ok());
    }

    #[test]
    fn test_check_encoding_replacement_char() {
        let mut cue = make_cue("test.flac", vec![]);
        cue.title = Some("Album\u{FFFD}Title".into());
        assert!(check_encoding(&cue).is_err());
    }

    #[test]
    fn test_resolve_file_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("album.flac");
        std::fs::write(&file_path, b"fake").unwrap();

        let result = resolve_file("album.flac", FileType::Flac, dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), file_path);
    }

    #[test]
    fn test_resolve_file_fallback_extension() {
        let dir = tempfile::tempdir().unwrap();
        // Only .ape exists, but CUE says FLAC
        let ape_path = dir.path().join("album.ape");
        std::fs::write(&ape_path, b"fake").unwrap();

        let result = resolve_file("album.flac", FileType::Flac, dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ape_path);
    }

    #[test]
    fn test_resolve_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_file("nonexistent.flac", FileType::Flac, dir.path());
        assert!(matches!(result, Err(CueBladeError::FileNotFound { .. })));
    }
}
