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

/// FLAC decoder backed by `claxon`.
///
/// Reads all samples into memory at open time for reliable
/// sequential and seekable access. claxon's sample iterator
/// borrows the reader mutably, making incremental reads across
/// multiple calls impossible without storing the iterator.
/// For Phase 1 this is acceptable; true streaming decode will
/// be optimized in a future pass.
pub struct FlacDecoder {
    info: AudioInfo,
    /// All decoded samples (interleaved i32).
    samples: Vec<i32>,
    /// Current read position in samples (per channel).
    position: usize,
    /// Vorbis comment tags read from the source FLAC file.
    tags: Vec<(String, String)>,
}

impl FlacDecoder {
    /// Open a FLAC file and decode all samples into memory.
    ///
    /// Validates header fields (sample rate, channels, bps) per SECURITY.md.
    ///
    /// # Errors
    ///
    /// - [`CueBladeError::Io`] if file cannot be opened.
    /// - [`CueBladeError::Sanitization`] if header is invalid or decode fails.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| CueBladeError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let buf_reader = BufReader::with_capacity(256 * 1024, file);
        let mut reader = FlacReader::new(buf_reader).map_err(|e| CueBladeError::Sanitization {
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

        let info = AudioInfo {
            sample_rate: stream_info.sample_rate,
            channels: stream_info.channels as u8,
            bits_per_sample: stream_info.bits_per_sample as u8,
            total_samples: stream_info.samples,
        };

        // Read all samples into memory
        let mut samples = Vec::new();
        for sample in reader.samples() {
            let s = sample.map_err(|e| CueBladeError::Sanitization {
                reason: format!("FLAC decode error: {e}"),
            })?;
            samples.push(s);
        }

        // Read all Vorbis comments from source FLAC via metaflac
        let tags = metaflac::Tag::read_from_path(path)
            .ok()
            .and_then(|tag| tag.vorbis_comments().cloned())
            .map(|vc| {
                vc.comments
                    .into_iter()
                    .flat_map(|(key, values)| {
                        values.into_iter().map(move |v| (key.to_uppercase(), v))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(Self {
            info,
            samples,
            position: 0,
            tags,
        })
    }
}

impl Decoder for FlacDecoder {
    fn audio_info(&self) -> &AudioInfo {
        &self.info
    }

    fn seek_to_sample(&mut self, sample_offset: u64) -> Result<()> {
        let channels = self.info.channels as usize;
        let target = sample_offset as usize * channels;
        self.position = target.min(self.samples.len());
        Ok(())
    }

    fn read_samples(&mut self, buffer: &mut [i32], max_samples: usize) -> Result<usize> {
        let channels = self.info.channels as usize;
        if channels == 0 {
            return Ok(0);
        }
        let available_total = self.samples.len().saturating_sub(self.position);
        let available_samples = available_total / channels;
        let to_read = max_samples.min(available_samples);
        if to_read == 0 {
            return Ok(0);
        }
        let src_start = self.position;
        let src_end = self.position + to_read * channels;
        let dst_end = to_read * channels;
        buffer[..dst_end].copy_from_slice(&self.samples[src_start..src_end]);
        self.position = src_end;
        Ok(to_read)
    }

    fn tags(&self) -> &[(String, String)] {
        &self.tags
    }
}

/// Streaming FLAC encoder backed by `flacenc`.
///
/// Writes interleaved PCM samples to any [`Write`] target with
/// configurable compression level and Vorbis comment metadata.
pub struct FlacEncoder<W: Write + Send> {
    writer: W,
    info: AudioInfo,
    metadata: Vec<(String, String)>,
    /// Accumulated interleaved samples waiting to be encoded.
    sample_buffer: Vec<i32>,
    compression_level: u32,
}

/// Encode a list of (key, value) pairs into Vorbis Comment binary format.
///
/// Format per https://xiph.org/vorbis/doc/Vorbis_I_spec.html#x1-640005:
/// - 4 bytes LE: vendor string length
/// - N bytes: vendor string (UTF-8)
/// - 4 bytes LE: number of comments
/// - For each comment:
///   - 4 bytes LE: comment length
///   - M bytes: "KEY=value" (UTF-8, KEY uppercase ASCII)
fn encode_vorbis_comment(tags: &[(String, String)]) -> Vec<u8> {
    let vendor = "cueblade 0.1.0";
    let vendor_bytes = vendor.as_bytes();
    let mut buf = Vec::new();

    // Vendor string
    buf.extend_from_slice(&(vendor_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(vendor_bytes);

    // Number of comments
    buf.extend_from_slice(&(tags.len() as u32).to_le_bytes());

    // Each comment: "KEY=value"
    for (key, value) in tags {
        let comment = format!("{key}={value}");
        let comment_bytes = comment.as_bytes();
        buf.extend_from_slice(&(comment_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(comment_bytes);
    }
    buf
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

        // Encode accumulated samples
        let source = flacenc::source::MemSource::from_samples(
            &self.sample_buffer,
            channels,
            bps,
            sample_rate,
        );

        let mut stream = flacenc::encode_with_fixed_block_size(
            &verified_config,
            source,
            verified_config.block_size,
        )
        .map_err(|e| CueBladeError::Sanitization {
            reason: format!("FLAC encode failed: {e:?}"),
        })?;

        // Inject Vorbis comment metadata block (type=4)
        if !self.metadata.is_empty() {
            let vc_bytes = encode_vorbis_comment(&self.metadata);
            let meta_block = flacenc::component::MetadataBlockData::new_unknown(4, &vc_bytes)
                .map_err(|e| CueBladeError::Sanitization {
                    reason: format!("Failed to create Vorbis comment block: {e:?}"),
                })?;
            stream.add_metadata_block(meta_block);
        }

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
