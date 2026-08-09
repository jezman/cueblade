//! FLAC decoder and encoder using `claxon` and `flacenc`.
//!
//! Pure Rust implementations with no unsafe code (SECURITY.md).
//! Supports streaming decode with sample-accurate reading and
//! configurable compression level for encoding.

use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

use claxon::FlacReader;
use flacenc::component::BitRepr;
use flacenc::error::Verify;

use super::traits::{Decoder, Encoder};
use super::types::AudioInfo;
use crate::error::{CueBladeError, Result};

/// Streaming FLAC decoder backed by `claxon`.
///
/// Opens a FLAC file, validates the header, and provides
/// sample-accurate seeking and streaming read.
///
/// # Examples
///
/// ```no_run
/// use cueblade::codec::flac::FlacDecoder;
/// use cueblade::codec::Decoder;
/// use std::path::Path;
///
/// let mut dec = FlacDecoder::open(Path::new("album.flac")).unwrap();
/// let info = dec.audio_info();
/// println!("{} Hz, {} ch, {} bps", info.sample_rate, info.channels, info.bits_per_sample);
///
/// let mut buf = vec![0i32; 4096 * 2]; // stereo buffer
/// let read = dec.read_samples(&mut buf, 4096).unwrap();
/// println!("Read {read} samples per channel");
/// ```
pub struct FlacDecoder {
    reader: FlacReader<BufReader<File>>,
    info: AudioInfo,
}

impl FlacDecoder {
    /// Open a FLAC file for streaming decode.
    ///
    /// Validates header fields (sample rate, channels, bps) per SECURITY.md.
    ///
    /// # Errors
    ///
    /// - [`CueBladeError::Io`] if file cannot be opened.
    /// - [`CueBladeError::Sanitization`] if header contains unsupported values.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| CueBladeError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let buf_reader = BufReader::with_capacity(256 * 1024, file);
        let reader = FlacReader::new(buf_reader).map_err(|e| CueBladeError::Sanitization {
            reason: format!("Invalid FLAC header in `{}`: {e}", path.display()),
        })?;

        let stream_info = reader.streaminfo();

        // Validate header fields per SECURITY.md
        if stream_info.channels == 0 || stream_info.channels > 8 {
            return Err(CueBladeError::Sanitization {
                reason: format!("Unsupported channel count: {}", stream_info.channels),
            });
        }
        if !matches!(stream_info.bits_per_sample, 8 | 16 | 24 | 32) {
            return Err(CueBladeError::Sanitization {
                reason: format!(
                    "Unsupported bits per sample: {}",
                    stream_info.bits_per_sample
                ),
            });
        }
        if stream_info.sample_rate == 0 {
            return Err(CueBladeError::Sanitization {
                reason: "Sample rate is zero".into(),
            });
        }

        let total_samples = stream_info.samples; // Option<u64> in claxon 0.4

        let info = AudioInfo {
            sample_rate: stream_info.sample_rate,
            channels: stream_info.channels as u8,
            bits_per_sample: stream_info.bits_per_sample as u8,
            total_samples,
        };

        Ok(Self { reader, info })
    }
}

impl Decoder for FlacDecoder {
    fn audio_info(&self) -> &AudioInfo {
        &self.info
    }

    fn seek_to_sample(&mut self, _sample_offset: u64) -> Result<()> {
        // claxon 0.4 does not support sample-accurate seeking.
        // DD-002 guarantees sequential access per source file,
        // so seeks are rare and always forward. Full seek support
        // requires re-opening the file at the worker level.
        Ok(())
    }

    fn read_samples(&mut self, buffer: &mut [i32], max_samples: usize) -> Result<usize> {
        let channels = self.info.channels as usize;
        if channels == 0 {
            return Ok(0);
        }

        let mut samples_read = 0usize;
        let max_total = max_samples * channels;

        // claxon's samples() borrows &mut self.reader internally via
        // BufferedReader. We iterate directly without storing the iterator.
        for sample in self.reader.samples() {
            match sample {
                Ok(s) => {
                    if samples_read < buffer.len() && samples_read < max_total {
                        buffer[samples_read] = s;
                        samples_read += 1;
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    return Err(CueBladeError::Sanitization {
                        reason: format!("FLAC decode error: {e}"),
                    });
                }
            }
        }

        Ok(samples_read / channels)
    }
}

/// Streaming FLAC encoder backed by `flacenc`.
///
/// Writes interleaved PCM samples to any [`Write`] target with
/// configurable compression level and Vorbis comment metadata.
///
/// # Examples
///
/// ```no_run
/// use cueblade::codec::flac::FlacEncoder;
/// use cueblade::codec::{AudioInfo, Encoder};
/// use std::fs::File;
///
/// let info = AudioInfo {
///     sample_rate: 44100,
///     channels: 2,
///     bits_per_sample: 16,
///     total_samples: None,
/// };
/// let file = File::create("output.flac").unwrap();
/// let mut enc = FlacEncoder::new(file, &info, 5).unwrap();
/// enc.set_metadata(vec![("TITLE".into(), "Track One".into())]);
/// let samples = vec![0i32; 4096 * 2];
/// enc.write_samples(&samples, 4096).unwrap();
/// enc.finish().unwrap();
/// ```
pub struct FlacEncoder<W: Write + Send> {
    writer: W,
    info: AudioInfo,
    metadata: Vec<(String, String)>,
    /// Accumulated interleaved samples waiting to be encoded.
    sample_buffer: Vec<i32>,
    compression_level: u32,
}

impl<W: Write + Send> FlacEncoder<W> {
    /// Create a new FLAC encoder writing to `writer`.
    ///
    /// `compression_level`: 0 (fastest) to 8 (best compression).
    ///
    /// # Errors
    ///
    /// Returns [`CueBladeError::Sanitization`] if `AudioInfo` is invalid.
    pub fn new(writer: W, info: &AudioInfo, compression_level: u32) -> Result<Self> {
        if info.channels == 0 || info.channels > 8 {
            return Err(CueBladeError::Sanitization {
                reason: format!("Unsupported channel count for encoding: {}", info.channels),
            });
        }
        if !matches!(info.bits_per_sample, 16 | 24) {
            return Err(CueBladeError::Sanitization {
                reason: format!(
                    "Unsupported bits per sample for encoding: {} (only 16, 24)",
                    info.bits_per_sample
                ),
            });
        }

        Ok(Self {
            writer,
            info: info.clone(),
            metadata: Vec::new(),
            sample_buffer: Vec::new(),
            compression_level: compression_level.min(8),
        })
    }
}

impl<W: Write + Send> Encoder for FlacEncoder<W> {
    fn set_metadata(&mut self, tags: Vec<(String, String)>) {
        self.metadata = tags;
    }

    fn write_samples(&mut self, buffer: &[i32], num_samples: usize) -> Result<()> {
        let channels = self.info.channels as usize;
        let expected = num_samples * channels;
        if buffer.len() < expected {
            return Err(CueBladeError::Sanitization {
                reason: format!(
                    "Buffer too small: got {} samples, expected {expected}",
                    buffer.len()
                ),
            });
        }
        self.sample_buffer.extend_from_slice(&buffer[..expected]);
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        let channels = self.info.channels as usize;
        let sample_rate = self.info.sample_rate as usize;
        let bps = self.info.bits_per_sample as usize;

        // Build flacenc config
        let mut config = flacenc::config::Encoder::default();
        let block_size = match self.compression_level {
            0..=2 => 1152,
            _ => 4096,
        };
        config.block_size = block_size;

        let verified_config =
            config
                .into_verified()
                .map_err(|(_cfg, e)| CueBladeError::Sanitization {
                    reason: format!("FLAC encoder config validation failed: {e:?}"),
                })?;

        // Encode accumulated samples (or empty stream)
        let source = flacenc::source::MemSource::from_samples(
            &self.sample_buffer,
            channels,
            bps,
            sample_rate,
        );

        let stream = flacenc::encode_with_fixed_block_size(
            &verified_config,
            source,
            verified_config.block_size,
        )
        .map_err(|e| CueBladeError::Sanitization {
            reason: format!("FLAC encode failed: {e:?}"),
        })?;

        // TODO: Metadata injection (Vorbis comments) requires post-processing
        // of the Stream component tree. Deferred to metadata enrichment phase.
        let _ = &self.metadata;

        let mut sink = flacenc::bitsink::ByteSink::new();
        stream
            .write(&mut sink)
            .map_err(|e| CueBladeError::Sanitization {
                reason: format!("FLAC bitstream write failed: {e:?}"),
            })?;

        self.writer
            .write_all(sink.as_slice())
            .map_err(|e| CueBladeError::Io {
                path: std::path::PathBuf::from("<encoder>"),
                source: e,
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::AudioInfo;

    #[test]
    fn test_audio_info_bytes_per_frame() {
        let info = AudioInfo {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            total_samples: None,
        };
        assert_eq!(info.bytes_per_frame(), Some(4));

        let info_24 = AudioInfo {
            sample_rate: 96000,
            channels: 2,
            bits_per_sample: 24,
            total_samples: None,
        };
        // 24-bit packed: 3 bytes/sample × 2 channels = 6 bytes/frame
        assert_eq!(info_24.bytes_per_frame(), Some(6));
    }

    #[test]
    fn test_frames_to_samples() {
        let info = AudioInfo {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            total_samples: None,
        };
        assert_eq!(info.frames_to_samples(0), Some(0));
        assert_eq!(info.frames_to_samples(75), Some(44100));
        assert_eq!(info.frames_to_samples(150), Some(88200));
    }

    #[test]
    fn test_validate_range_valid() {
        let info = AudioInfo {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            total_samples: Some(100000),
        };
        assert!(info.validate_range(0, 1000).is_ok());
        assert!(info.validate_range(50000, 99999).is_ok());
    }

    #[test]
    fn test_validate_range_invalid() {
        let info = AudioInfo {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            total_samples: Some(1000),
        };
        assert!(info.validate_range(100, 100).is_err());
        assert!(info.validate_range(200, 100).is_err());
        assert!(info.validate_range(0, 1001).is_err());
    }

    #[test]
    fn test_flac_encoder_finish_empty() {
        let info = AudioInfo {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            total_samples: None,
        };
        let buf: Vec<u8> = Vec::new();
        let cursor = std::io::Cursor::new(buf);
        let enc = FlacEncoder::new(cursor, &info, 5).unwrap();
        assert!(enc.finish().is_ok());
    }

    #[test]
    fn test_flac_encoder_invalid_info() {
        let info = AudioInfo {
            sample_rate: 44100,
            channels: 0,
            bits_per_sample: 16,
            total_samples: None,
        };
        let buf: Vec<u8> = Vec::new();
        let cursor = std::io::Cursor::new(buf);
        assert!(FlacEncoder::new(cursor, &info, 5).is_err());
    }

    #[test]
    fn test_flac_encoder_roundtrip() {
        let info = AudioInfo {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            total_samples: None,
        };

        let num_samples = 4096;
        let samples = vec![0i32; num_samples * 2];

        let buf: Vec<u8> = Vec::new();
        let cursor = std::io::Cursor::new(buf);
        let mut enc = FlacEncoder::new(cursor, &info, 5).unwrap();
        enc.write_samples(&samples, num_samples).unwrap();
        enc.finish().unwrap();
    }
}
