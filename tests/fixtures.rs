//! Test fixture generation for integration tests.
//!
//! Uses a pre-encoded valid FLAC file (generated via ffmpeg) to avoid
//! encoder/decoder compatibility issues between flacenc and claxon.

use std::fs;
use std::path::{Path, PathBuf};

/// Base64-encoded minimal valid FLAC: 5 seconds of silence,
/// 44100 Hz, stereo, 16-bit. Generated via ffmpeg/libflac.
const SILENCE_FLAC_B64: &str = include_str!("fixtures/silence.flac.b64");

/// A generated test fixture containing a FLAC file and CUE sheet.
#[allow(dead_code)]
pub struct TestFixture {
    pub dir: PathBuf,
    pub flac_path: PathBuf,
    pub cue_path: PathBuf,
    pub sample_rate: u32,
    pub channels: usize,
    pub bits_per_sample: usize,
    pub total_samples: usize,
}

/// Decode base64 FLAC and write to path.
fn write_flac_fixture(path: &Path) {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(SILENCE_FLAC_B64.trim())
        .expect("valid base64 FLAC fixture");
    fs::write(path, &bytes).expect("write FLAC fixture");
}

/// Create a test fixture with a 3-track CUE sheet and matching FLAC.
///
/// Audio: 5 seconds of silence at 44100 Hz, stereo, 16-bit.
/// Tracks:
///   1. INDEX 01 at 00:00:00
///   2. INDEX 00 at 00:00:01, INDEX 01 at 00:00:02
///   3. INDEX 01 at 00:00:03
pub fn create_three_track_fixture(base_dir: &Path) -> TestFixture {
    let sample_rate = 44100u32;
    let channels = 2usize;
    let bps = 16usize;
    let total_samples = sample_rate as usize * 5; // 5 seconds

    let flac_path = base_dir.join("album.flac");
    let cue_path = base_dir.join("album.cue");

    write_flac_fixture(&flac_path);

    // Track 1: 00:00:00 → 00:00:02 (2 sec)
    // Track 2: INDEX 00 at 00:00:01, INDEX 01 at 00:00:02 → 00:00:03 (1 sec main + 1 sec gap)
    // Track 3: 00:00:03 → 00:00:05 (2 sec)
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "First Track"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Second Track"
    INDEX 00 00:00:01
    INDEX 01 00:00:02
  TRACK 03 AUDIO
    TITLE "Third Track"
    INDEX 01 00:00:03
"#;

    fs::write(&cue_path, cue_content).expect("CUE write failed");

    TestFixture {
        dir: base_dir.to_path_buf(),
        flac_path,
        cue_path,
        sample_rate,
        channels,
        bits_per_sample: bps,
        total_samples,
    }
}

/// Create a minimal single-track fixture.
pub fn create_single_track_fixture(base_dir: &Path) -> TestFixture {
    let sample_rate = 44100u32;
    let channels = 2usize;
    let bps = 16usize;
    let total_samples = sample_rate as usize * 5;

    let flac_path = base_dir.join("single.flac");
    let cue_path = base_dir.join("single.cue");

    write_flac_fixture(&flac_path);

    let cue_content = r#"PERFORMER "Solo Artist"
TITLE "Single Album"
FILE "single.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Only Track"
    INDEX 01 00:00:00
"#;

    fs::write(&cue_path, cue_content).expect("CUE write failed");

    TestFixture {
        dir: base_dir.to_path_buf(),
        flac_path,
        cue_path,
        sample_rate,
        channels,
        bits_per_sample: bps,
        total_samples,
    }
}
