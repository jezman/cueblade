//! Audio codec abstraction layer.
//!
//! Provides format-agnostic [`Decoder`] and [`Encoder`] traits
//! for lossless audio processing. All sample arithmetic uses
//! checked operations to prevent overflow (SECURITY.md).

pub mod flac;

use std::io::{Read, Write};
use std::path::Path;

use crate::error::Result;

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
            return Err(crate::error::CueBladeError::Sanitization {
                reason: format!("Invalid sample range: start ({start}) >= end ({end})"),
            });
        }
        if let Some(total) = self.total_samples {
            if end > total {
                return Err(crate::error::CueBladeError::Sanitization {
                    reason: format!(
                        "Sample range exceeds audio length: end ({end}) > total ({total})"
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Trait for streaming audio decoders.
///
/// Implementations must be safe (no unsafe), support seeking by
/// sample offset, and provide header metadata via [`audio_info()`].
pub trait Decoder: Send {
    /// Return audio stream metadata.
    fn audio_info(&self) -> &AudioInfo;

    /// Seek to an absolute sample offset (per-channel).
    ///
    /// Next call to [`read_samples()`] will start from this position.
    fn seek_to_sample(&mut self, sample_offset: u64) -> Result<()>;

    /// Read up to `max_samples` per-channel samples into `buffer`.
    ///
    /// Returns the number of samples actually read (per channel).
    /// Samples are interleaved for multi-channel audio.
    /// Returns 0 at EOF.
    fn read_samples(&mut self, buffer: &mut [i32], max_samples: usize) -> Result<usize>;
}

/// Trait for streaming audio encoders.
///
/// Implementations must be safe (no unsafe) and write to any
/// [`Write`] target. Metadata injection happens before encoding.
pub trait Encoder: Send {
    /// Set Vorbis comment metadata to embed in output.
    fn set_metadata(&mut self, tags: Vec<(String, String)>);

    /// Encode interleaved PCM samples from `buffer`.
    ///
    /// `num_samples` is the count per channel. Buffer length must be
    /// `num_samples * channels`.
    fn write_samples(&mut self, buffer: &[i32], num_samples: usize) -> Result<()>;

    /// Finalize encoding and flush all buffered data.
    ///
    /// Must be called exactly once after all samples are written.
    fn finish(self) -> Result<()>;
}

/// Open a decoder for the given audio file.
///
/// Currently supports FLAC only. APE/WAV/WV will be added in Phase 5.
///
/// # Errors
///
/// Returns [`CueBladeError::Io`] if file cannot be opened.
/// Returns [`CueBladeError::Sanitization`] if header is invalid.
pub fn open_decoder(path: &Path) -> Result<Box<dyn Decoder>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "flac" => Ok(Box::new(flac::FlacDecoder::open(path)?)),
        other => Err(crate::error::CueBladeError::Sanitization {
            reason: format!("Unsupported audio format: .{other} (only .flac supported in Phase 1)"),
        }),
    }
}
