//! Format-agnostic decoder and encoder traits.
//!
//! Implementations must be safe (no unsafe), support the operations
//! defined here, and be `Send` for use in rayon worker pool.

use super::types::AudioInfo;
use crate::error::Result;

/// Trait for streaming audio decoders.
///
/// Implementations must be safe (no unsafe), support seeking by
/// sample offset, and provide header metadata via [`Decoder::audio_info`].
pub trait Decoder: Send {
    /// Return audio stream metadata.
    fn audio_info(&self) -> &AudioInfo;

    /// Seek to an absolute sample offset (per-channel).
    ///
    /// Next call to [`Decoder::read_samples`] will start from this position.
    fn seek_to_sample(&mut self, sample_offset: u64) -> Result<()>;

    /// Read up to `max_samples` per-channel samples into `buffer`.
    ///
    /// Returns the number of samples actually read (per channel).
    /// Samples are interleaved for multi-channel audio.
    /// Returns 0 at EOF.
    fn read_samples(&mut self, buffer: &mut [i32], max_samples: usize) -> Result<usize>;

    /// Get Vorbis comment tags from the source file.
    fn tags(&self) -> &[(String, String)];
}

/// Trait for streaming audio encoders.
///
/// Implementations must be safe (no unsafe) and write to any
/// [`std::io::Write`] target. Metadata injection happens before encoding.
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
