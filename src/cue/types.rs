//! Data types representing a parsed CUE sheet.
//!
//! These types model the Red Book CUE specification with extensions
//! for real-world edge cases (mixed encodings, non-standard REM fields).
//! All timecodes are stored as frame counts (`u64`) for safe arithmetic.

use std::fmt;

/// Audio file type referenced in a FILE directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// FLAC lossless audio
    Flac,
    /// Monkey's Audio (APE)
    Ape,
    /// Waveform Audio (WAV/PCM/RF64)
    Wav,
    /// WavPack lossless/hybrid
    WavPack,
    /// Binary data (raw PCM, used in some CUE sheets)
    Binary,
    /// Motorola byte order binary
    Motorola,
    /// AIFF audio
    Aiff,
    /// Unknown or unsupported file type
    Unknown,
}

impl fmt::Display for FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Flac => write!(f, "FLAC"),
            Self::Ape => write!(f, "APE"),
            Self::Wav => write!(f, "WAVE"),
            Self::WavPack => write!(f, "WAVPACK"),
            Self::Binary => write!(f, "BINARY"),
            Self::Motorola => write!(f, "MOTOROLA"),
            Self::Aiff => write!(f, "AIFF"),
            Self::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Timecode in CUE format: MM:SS:FF (frames, 75 per second).
///
/// Stored internally as absolute frame count (`u64`) to enable
/// safe checked arithmetic without repeated conversions.
///
/// # Examples
///
/// ```
/// use cueblade::cue::types::Timecode;
///
/// let tc = Timecode::from_msf(2, 30, 50).unwrap();
/// assert_eq!(tc.frames(), 2 * 4500 + 30 * 75 + 50);
/// assert_eq!(tc.to_msf(), (2, 30, 50));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timecode(u64);

impl Timecode {
    /// Frames per second in CD-DA (Red Book).
    pub const FRAMES_PER_SECOND: u64 = 75;

    /// Create a [`Timecode`] from minutes, seconds, and frames.
    ///
    /// Returns `None` if any component would cause overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use cueblade::cue::types::Timecode;
    ///
    /// assert!(Timecode::from_msf(0, 0, 0).is_some());
    /// assert!(Timecode::from_msf(99, 59, 74).is_some());
    /// ```
    pub fn from_msf(minutes: u64, seconds: u64, frames: u64) -> Option<Self> {
        let mins = minutes.checked_mul(Self::FRAMES_PER_SECOND * 60)?;
        let secs = seconds.checked_mul(Self::FRAMES_PER_SECOND)?;
        let total = mins.checked_add(secs)?.checked_add(frames)?;
        Some(Self(total))
    }

    /// Raw frame count.
    #[inline]
    pub fn frames(self) -> u64 {
        self.0
    }

    /// Convert back to (minutes, seconds, frames).
    ///
    /// # Examples
    ///
    /// ```
    /// use cueblade::cue::types::Timecode;
    ///
    /// let tc = Timecode::from_msf(5, 10, 25).unwrap();
    /// assert_eq!(tc.to_msf(), (5, 10, 25));
    /// ```
    pub fn to_msf(self) -> (u64, u64, u64) {
        let total_secs = self.0 / Self::FRAMES_PER_SECOND;
        let frames = self.0 % Self::FRAMES_PER_SECOND;
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        (minutes, seconds, frames)
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (m, s, fr) = self.to_msf();
        write!(f, "{m:02}:{s:02}:{fr:02}")
    }
}

/// An INDEX point within a track.
///
/// INDEX 00 = pre-gap start, INDEX 01 = track start,
/// INDEX 02+ = sub-index points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    /// Index number (0–99).
    pub number: u8,
    /// Absolute position from disc start.
    pub timestamp: Timecode,
}

/// A single track entry in a CUE sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// Track number (1–99 per Red Book, extended to 999).
    pub number: u16,
    /// Track type (AUDIO, CDG, MODE1/2048, etc.).
    pub track_type: String,
    /// Optional title.
    pub title: Option<String>,
    /// Optional performer.
    pub performer: Option<String>,
    /// Index points (must include at least INDEX 01).
    pub indices: Vec<Index>,
    /// ISRC code if present.
    pub isrc: Option<String>,
}

/// A complete parsed CUE sheet.
///
/// Represents the top-level structure after syntactic parsing.
/// Semantic validation and sanitization are performed separately.
///
/// # Examples
///
/// ```
/// use cueblade::cue::types::{CueSheet, Track, Index, Timecode, FileType};
///
/// let cue = CueSheet {
///     performer: Some("Artist".into()),
///     title: Some("Album".into()),
///     file: "album.flac".into(),
///     file_type: FileType::Flac,
///     tracks: vec![Track {
///         number: 1,
///         track_type: "AUDIO".into(),
///         title: Some("Track One".into()),
///         performer: None,
///         indices: vec![Index {
///             number: 1,
///             timestamp: Timecode::from_msf(0, 0, 0).unwrap(),
///         }],
///         isrc: None,
///     }],
///     rem_comments: vec![],
/// };
/// assert_eq!(cue.tracks.len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueSheet {
    /// Global performer (may be overridden per-track).
    pub performer: Option<String>,
    /// Global title (album name).
    pub title: Option<String>,
    /// Filename from the FILE directive.
    pub file: String,
    /// File type from the FILE directive.
    pub file_type: FileType,
    /// Ordered list of tracks.
    pub tracks: Vec<Track>,
    /// Collected REM comments (raw strings).
    pub rem_comments: Vec<String>,
}
