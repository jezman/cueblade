//! Gap handling logic for pre-gap regions between tracks.
//!
//! Calculates adjusted sample ranges based on the configured
//! [`GapMode`]: prepend (include pre-gap in current track),
//! append (include next track's pre-gap at end), or discard.

use crate::cli::GapMode;
use crate::cue::types::CueSheet;
use crate::error::{CueBladeError, Result};

/// A calculated sample range for a single track after gap adjustment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRange {
    /// Track number from CUE sheet.
    pub track_number: u16,
    /// Start sample offset (inclusive).
    pub start_sample: u64,
    /// End sample offset (exclusive).
    pub end_sample: u64,
}

/// Calculate sample ranges for all tracks with gap handling applied.
///
/// Takes a sanitized [`CueSheet`] and audio metadata to convert
/// timecodes to sample offsets, then adjusts ranges based on
/// the selected [`GapMode`].
///
/// # Gap Modes
///
/// - **Prepend**: Each track starts at INDEX 00 (if present) instead
///   of INDEX 01. First track without INDEX 00 starts at 0.
/// - **Append**: Each track ends at the next track's INDEX 00 (if present)
///   instead of its INDEX 01. Last track extends to total_samples.
/// - **Discard**: Tracks use only INDEX 01 boundaries (no gap adjustment).
///
/// # Errors
///
/// Returns [`CueBladeError::Arithmetic`] if timecode-to-sample conversion
/// overflows. Returns [`CueBladeError::Sanitization`] if a track lacks
/// INDEX 01.
///
/// # Examples
///
/// ```
/// use cueblade::pipeline::gap::{calculate_track_ranges, TrackRange};
/// use cueblade::cli::GapMode;
/// use cueblade::cue::types::{CueSheet, FileType, Index, Timecode, Track};
///
/// let cue = CueSheet {
///     performer: None,
///     title: None,
///     file: "test.flac".into(),
///     file_type: FileType::Flac,
///     tracks: vec![
///         Track {
///             number: 1,
///             track_type: "AUDIO".into(),
///             title: None,
///             performer: None,
///             indices: vec![
///                 Index { number: 1, timestamp: Timecode::from_msf(0, 0, 0).unwrap() },
///             ],
///             isrc: None,
///         },
///         Track {
///             number: 2,
///             track_type: "AUDIO".into(),
///             title: None,
///             performer: None,
///             indices: vec![
///                 Index { number: 0, timestamp: Timecode::from_msf(0, 0, 20).unwrap() },
///                 Index { number: 1, timestamp: Timecode::from_msf(0, 0, 30).unwrap() },
///             ],
///             isrc: None,
///         },
///     ],
///     rem_comments: vec![],
/// };
///
/// // Discard mode: track 1 = [0..17640], track 2 = [17640..end]
/// // 30 frames at 44100 Hz = 17640 samples
/// let ranges = calculate_track_ranges(&cue, 44100, Some(88200), GapMode::Discard).unwrap();
/// assert_eq!(ranges.len(), 2);
/// assert_eq!(ranges[0].start_sample, 0);
/// assert_eq!(ranges[1].start_sample, 17640); // INDEX 01 of track 2 (30 frames)
/// ```
pub fn calculate_track_ranges(
    cue: &CueSheet,
    sample_rate: u32,
    total_samples: Option<u64>,
    gap_mode: GapMode,
) -> Result<Vec<TrackRange>> {
    if cue.tracks.is_empty() {
        return Ok(Vec::new());
    }

    let frames_to_samples = |frames: u64| -> Result<u64> {
        let numerator =
            frames
                .checked_mul(sample_rate as u64)
                .ok_or_else(|| CueBladeError::Arithmetic {
                    operation: format!("frames_to_samples: {frames} * {sample_rate}"),
                })?;
        Ok(numerator / 75)
    };

    // Collect INDEX 01 frames for each track (required)
    let mut index_01_frames: Vec<u64> = Vec::with_capacity(cue.tracks.len());
    for track in &cue.tracks {
        let idx01 = track
            .indices
            .iter()
            .find(|i| i.number == 1)
            .ok_or_else(|| CueBladeError::Sanitization {
                reason: format!("Track {} has no INDEX 01", track.number),
            })?;
        index_01_frames.push(idx01.timestamp.frames());
    }

    // Collect INDEX 00 frames for each track (optional)
    let index_00_frames: Vec<Option<u64>> = cue
        .tracks
        .iter()
        .map(|track| {
            track
                .indices
                .iter()
                .find(|i| i.number == 0)
                .map(|i| i.timestamp.frames())
        })
        .collect();

    let total_frames_limit = total_samples.map(|s| s * 75 / sample_rate as u64);

    let mut ranges = Vec::with_capacity(cue.tracks.len());

    for (i, track) in cue.tracks.iter().enumerate() {
        let base_start = index_01_frames[i];
        let base_end = if i + 1 < cue.tracks.len() {
            index_01_frames[i + 1]
        } else {
            total_frames_limit.unwrap_or(u64::MAX)
        };

        let (adjusted_start, adjusted_end) = match gap_mode {
            GapMode::Discard => (base_start, base_end),

            GapMode::Prepend => {
                // Extend start to INDEX 00 if present
                let start = index_00_frames[i].unwrap_or(base_start);
                (start, base_end)
            }

            GapMode::Append => {
                // Extend end to next track's INDEX 00 if present
                let end = if i + 1 < cue.tracks.len() {
                    index_00_frames[i + 1].unwrap_or(base_end)
                } else {
                    // Last track: extend to total or keep base_end
                    base_end
                };
                (base_start, end)
            }
        };

        let start_sample = frames_to_samples(adjusted_start)?;
        let mut end_sample = frames_to_samples(adjusted_end)?;

        // Clamp end to total_samples
        if let Some(total) = total_samples {
            end_sample = end_sample.min(total);
        }

        // Skip empty ranges
        if start_sample >= end_sample {
            continue;
        }

        ranges.push(TrackRange {
            track_number: track.number,
            start_sample,
            end_sample,
        });
    }

    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue::types::{CueSheet, FileType, Index, Timecode, Track};

    fn make_cue(tracks: Vec<Track>) -> CueSheet {
        CueSheet {
            performer: None,
            title: None,
            file: "test.flac".into(),
            file_type: FileType::Flac,
            tracks,
            rem_comments: vec![],
        }
    }

    fn make_track(number: u16, indices: Vec<(u8, u64, u64, u64)>) -> Track {
        Track {
            number,
            track_type: "AUDIO".into(),
            title: None,
            performer: None,
            indices: indices
                .into_iter()
                .map(|(num, m, s, f)| Index {
                    number: num,
                    timestamp: Timecode::from_msf(m, s, f).unwrap(),
                })
                .collect(),
            isrc: None,
        }
    }

    #[test]
    fn test_discard_no_gaps() {
        // Track 1: INDEX 01 at 00:00:00 (frame 0)
        // Track 2: INDEX 01 at 00:00:30 (frame 30*75 = 2250)
        // At 44100 Hz: track1 = [0..1323000], track2 = [1323000..2646000]
        let cue = make_cue(vec![
            make_track(1, vec![(1, 0, 0, 0)]),
            make_track(2, vec![(1, 0, 0, 30)]), // 30 frames = 0.4 sec
        ]);

        let ranges = calculate_track_ranges(&cue, 44100, Some(2646000), GapMode::Discard).unwrap();
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_sample, 0);
        assert_eq!(ranges[0].end_sample, 30 * 44100 / 75); // 17640
        assert_eq!(ranges[1].start_sample, 17640);
        assert_eq!(ranges[1].end_sample, 2646000);
    }

    #[test]
    fn test_prepend_with_index_00() {
        // Track 1: INDEX 01 at 00:00:00 (frame 0)
        // Track 2: INDEX 00 at 00:00:20 (20 sec = 1500 frames), INDEX 01 at 00:00:30 (30 sec = 2250 frames)
        let cue = make_cue(vec![
            make_track(1, vec![(1, 0, 0, 0)]),
            make_track(2, vec![(0, 0, 20, 0), (1, 0, 30, 0)]),
        ]);

        let ranges =
            calculate_track_ranges(&cue, 44100, Some(44100 * 60), GapMode::Prepend).unwrap();
        assert_eq!(ranges.len(), 2);
        // Track 1: unchanged (no INDEX 00)
        assert_eq!(ranges[0].start_sample, 0);
        // Track 2: starts at INDEX 00 (1500 frames = 882000 samples)
        let expected_start = 1500 * 44100 / 75;
        assert_eq!(ranges[1].start_sample, expected_start);
    }

    #[test]
    fn test_append_with_next_index_00() {
        // Track 1: INDEX 01 at 00:00:00 (frame 0)
        // Track 2: INDEX 00 at 00:00:20 (20 sec = 1500 frames), INDEX 01 at 00:00:30 (30 sec = 2250 frames)
        let cue = make_cue(vec![
            make_track(1, vec![(1, 0, 0, 0)]),
            make_track(2, vec![(0, 0, 20, 0), (1, 0, 30, 0)]),
        ]);

        let ranges =
            calculate_track_ranges(&cue, 44100, Some(44100 * 60), GapMode::Append).unwrap();
        assert_eq!(ranges.len(), 2);
        // Track 1: ends at track 2's INDEX 00 (1500 frames = 882000 samples)
        let expected_end = 1500 * 44100 / 75;
        assert_eq!(ranges[0].end_sample, expected_end);
        // Track 2: starts at INDEX 01 (2250 frames = 1323000 samples)
        let expected_start = 2250 * 44100 / 75;
        assert_eq!(ranges[1].start_sample, expected_start);
    }

    #[test]
    fn test_first_track_no_index_00_prepend() {
        let cue = make_cue(vec![make_track(1, vec![(1, 0, 0, 0)])]);

        let ranges = calculate_track_ranges(&cue, 44100, Some(44100), GapMode::Prepend).unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_sample, 0); // No INDEX 00 → start at 0
    }

    #[test]
    fn test_last_track_append_clamped() {
        let cue = make_cue(vec![make_track(1, vec![(1, 0, 0, 0)])]);

        let ranges = calculate_track_ranges(&cue, 44100, Some(44100), GapMode::Append).unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].end_sample, 44100); // Clamped to total
    }

    #[test]
    fn test_missing_index_01_error() {
        let cue = make_cue(vec![Track {
            number: 1,
            track_type: "AUDIO".into(),
            title: None,
            performer: None,
            indices: vec![Index {
                number: 0,
                timestamp: Timecode::from_msf(0, 0, 0).unwrap(),
            }],
            isrc: None,
        }]);

        let result = calculate_track_ranges(&cue, 44100, None, GapMode::Discard);
        assert!(matches!(result, Err(CueBladeError::Sanitization { .. })));
    }

    #[test]
    fn test_empty_tracks() {
        let cue = make_cue(vec![]);
        let ranges = calculate_track_ranges(&cue, 44100, None, GapMode::Discard).unwrap();
        assert!(ranges.is_empty());
    }
}
