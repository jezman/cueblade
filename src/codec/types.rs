//! Audio stream metadata and sample arithmetic utilities.
//!
//! All calculations use checked operations to prevent overflow
//! per SECURITY.md.

use crate::error::{CueBladeError, Result};

/// Audio stream metadata extracted from header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInfo {
    /// Sample rate in Hz (e.g., 44100, 48000, 96000).
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u8,
    /// Bits per sample (16, 24, 32).
    pub bits_per_sample: u8,
    /// Total number of samples per channel (if known).
    pub total_samples: Option<u64>,
}

impl AudioInfo {
    /// Calculate bytes per frame (one sample across all channels).
    ///
    /// Returns `None` on arithmetic overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use cueblade::codec::AudioInfo;
    ///
    /// let info = AudioInfo {
    ///     sample_rate: 44100,
    ///     channels: 2,
    ///     bits_per_sample: 16,
    ///     total_samples: None,
    /// };
    /// assert_eq!(info.bytes_per_frame(), Some(4)); // 2 channels × 2 bytes
    /// ```
    pub fn bytes_per_frame(&self) -> Option<u32> {
        let bytes_per_sample = (self.bits_per_sample as u32).checked_add(7)? / 8;
        bytes_per_sample.checked_mul(self.channels as u32)
    }

    /// Convert timecode frames (75/sec CD-DA) to PCM sample offset.
    ///
    /// Uses checked arithmetic. Returns `None` on overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use cueblade::codec::AudioInfo;
    ///
    /// let info = AudioInfo {
    ///     sample_rate: 44100,
    ///     channels: 2,
    ///     bits_per_sample: 16,
    ///     total_samples: None,
    /// };
    /// // 1 second = 75 CD frames = 44100 samples
    /// assert_eq!(info.frames_to_samples(75), Some(44100));
    /// assert_eq!(info.frames_to_samples(0), Some(0));
    /// ```
    pub fn frames_to_samples(&self, cd_frames: u64) -> Option<u64> {
        // CD-DA: 75 frames/sec → samples = cd_frames * sample_rate / 75
        let numerator = cd_frames.checked_mul(self.sample_rate as u64)?;
        Some(numerator / 75)
    }

    /// Validate that a sample range is within bounds.
    ///
    /// Returns `Ok(())` if `start < end` and both are within
    /// `total_samples` (when known). Returns descriptive error otherwise.
    pub fn validate_range(&self, start: u64, end: u64) -> Result<()> {
        if start >= end {
            return Err(CueBladeError::Sanitization {
                reason: format!("Invalid sample range: start ({start}) >= end ({end})"),
            });
        }
        if let Some(total) = self.total_samples {
            if end > total {
                return Err(CueBladeError::Sanitization {
                    reason: format!(
                        "Sample range exceeds audio length: end ({end}) > total ({total})"
                    ),
                });
            }
        }
        Ok(())
    }
}
