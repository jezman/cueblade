//! Encoding detection and safe conversion for CUE sheets.
//!
//! Implements the fallback chain specified in SECURITY.md:
//! UTF-8 (strict) → chardetng heuristic → CP1251 fallback.
//! All inputs are bounded to 10 MB to prevent DoS.

use crate::error::{CueBladeError, Result};
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;

/// Maximum allowed CUE sheet size in bytes (10 MB).
pub const MAX_CUE_SIZE: usize = 10 * 1024 * 1024;

/// Detect encoding and convert raw bytes to a UTF-8 [`String`].
///
/// # Algorithm
///
/// 1. Reject if `bytes.len() > MAX_CUE_SIZE`.
/// 2. Try strict UTF-8 validation. If valid, return as-is.
/// 3. Use `chardetng` to guess encoding from byte patterns.
/// 4. Decode with detected encoding (lossy replacement for invalid sequences).
/// 5. Fallback to CP1251 if decoding produced errors or empty result.
///
/// # Errors
///
/// Returns [`CueBladeError::InputValidation`] if size limit exceeded.
///
/// # Examples
///
/// ```
/// use cueblade::cue::encoding::decode_cue_text;
///
/// let utf8 = b"TITLE \"Hello\"";
/// let text = decode_cue_text(utf8).unwrap();
/// assert_eq!(text, "TITLE \"Hello\"");
///
/// // CP1251 encoded bytes for "Привет"
/// let cp1251: &[u8] = &[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
/// let text = decode_cue_text(cp1251).unwrap();
/// assert!(text.contains("Привет"));
/// ```
pub fn decode_cue_text(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_CUE_SIZE {
        return Err(CueBladeError::InputValidation {
            reason: format!(
                "CUE sheet exceeds maximum size: {} bytes (limit: {} bytes)",
                bytes.len(),
                MAX_CUE_SIZE
            ),
        });
    }

    // Fast path: valid UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(s.to_owned());
    }

    // Heuristic detection via chardetng v1
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, Utf8Detection::Deny);

    let (decoded, _enc_used, had_errors) = encoding.decode(bytes);

    // If decoding produced replacements or empty result,
    // try CP1251 as explicit fallback (common for Russian/Cyrillic CUE sheets)
    if had_errors || decoded.trim().is_empty() {
        if let Some(cp1251) = Encoding::for_label(b"windows-1251") {
            let (fallback_decoded, _, fallback_had_errors) = cp1251.decode(bytes);
            if !fallback_had_errors && !fallback_decoded.trim().is_empty() {
                return Ok(fallback_decoded.into_owned());
            }
        }
    }

    // Last resort: return chardetng result even with replacement chars
    Ok(decoded.into_owned())
}
