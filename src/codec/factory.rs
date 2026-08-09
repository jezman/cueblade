//! Factory functions for opening audio decoders by file extension.
//!
//! Currently supports FLAC only. APE/WAV/WV will be added in Phase 5
//! per ROADMAP.md.

use std::path::Path;

use super::flac::FlacDecoder;
use super::traits::Decoder;
use crate::error::{CueBladeError, Result};

/// Open a decoder for the given audio file.
///
/// Detects format by file extension. Returns a boxed [`Decoder`]
/// trait object for format-agnostic processing.
///
/// # Errors
///
/// - [`CueBladeError::Io`] if file cannot be opened.
/// - [`CueBladeError::Sanitization`] if header is invalid or format unsupported.
pub fn open_decoder(path: &Path) -> Result<Box<dyn Decoder>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "flac" => Ok(Box::new(FlacDecoder::open(path)?)),
        other => Err(CueBladeError::Sanitization {
            reason: format!("Unsupported audio format: .{other} (only .flac supported in Phase 1)"),
        }),
    }
}
