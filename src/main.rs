//! cueblade CLI entry point.
//!
//! Parses arguments, resolves mode, and dispatches to the
//! appropriate processing pipeline. Currently supports
//! explicit mode only.

use std::process;

use clap::Parser;

use cueblade::cli::{Cli, Mode};
use cueblade::codec::{self, Encoder};
use cueblade::cue;
use cueblade::discovery;
use cueblade::safety;

fn main() {
    let cli = Cli::parse();

    let mode = match cli.resolve() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(2);
        }
    };

    let result = match mode {
        Mode::Explicit { flac, cue, out } => run_explicit(&flac, &cue, &out),
    };

    match result {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

/// Execute explicit mode: parse CUE → sanitize → decode → encode → atomic write.
///
/// Single-threaded pipeline for one source file. Rayon parallelism
/// and multi-source batching are added in Phase 2.
fn run_explicit(
    flac_path: &std::path::Path,
    cue_path: &std::path::Path,
    out_dir: &std::path::Path,
) -> cueblade::error::Result<()> {
    // 1. Discovery: validate paths
    let source = discovery::discover_explicit(flac_path, cue_path)?;
    eprintln!(
        "Source: {} + {}",
        source.audio_path.display(),
        source.cue_path.display()
    );

    // 2. Parse CUE
    let cue_bytes =
        std::fs::read(&source.cue_path).map_err(|e| cueblade::error::CueBladeError::Io {
            path: source.cue_path.clone(),
            source: e,
        })?;
    let cue_sheet = cue::parse_cue(&cue_bytes)?;
    eprintln!("Parsed CUE: {} tracks", cue_sheet.tracks.len());

    // 3. Sanitize
    let base_dir = source
        .cue_path
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let sanitized = cue::sanitize(cue_sheet, base_dir)?;
    eprintln!(
        "Sanitized: resolved audio = {}",
        sanitized.resolved_audio_path().display()
    );

    // 4. Open decoder
    let mut decoder = codec::open_decoder(sanitized.resolved_audio_path())?;
    let info = decoder.audio_info().clone();
    eprintln!(
        "Audio: {} Hz, {} ch, {} bps",
        info.sample_rate, info.channels, info.bits_per_sample
    );

    // 5. Ensure output directory exists
    std::fs::create_dir_all(out_dir).map_err(|e| cueblade::error::CueBladeError::Io {
        path: out_dir.to_path_buf(),
        source: e,
    })?;

    // 6. Process each track: decode range → encode → atomic write
    let cue_data = sanitized.cue();
    for (i, track) in cue_data.tracks.iter().enumerate() {
        // Determine sample range from indices
        let start_idx = track
            .indices
            .iter()
            .find(|idx| idx.number == 1)
            .ok_or_else(|| cueblade::error::CueBladeError::Sanitization {
                reason: format!("Track {} has no INDEX 01", track.number),
            })?;

        let end_frames = if i + 1 < cue_data.tracks.len() {
            // End at next track's INDEX 01
            let next_track = &cue_data.tracks[i + 1];
            next_track
                .indices
                .iter()
                .find(|idx| idx.number == 1)
                .map(|idx| idx.timestamp.frames())
                .unwrap_or_else(|| {
                    info.total_samples
                        .map(|s| s * 75 / info.sample_rate as u64)
                        .unwrap_or(u64::MAX)
                })
        } else {
            // Last track: use total samples or MAX
            info.total_samples
                .map(|s| s * 75 / info.sample_rate as u64)
                .unwrap_or(u64::MAX)
        };

        let start_samples = info
            .frames_to_samples(start_idx.timestamp.frames())
            .ok_or_else(|| cueblade::error::CueBladeError::Arithmetic {
                operation: format!(
                    "frames_to_samples({}) for track {}",
                    start_idx.timestamp.frames(),
                    track.number
                ),
            })?;
        let end_samples = info.frames_to_samples(end_frames).ok_or_else(|| {
            cueblade::error::CueBladeError::Arithmetic {
                operation: format!("frames_to_samples({end_frames}) for track {}", track.number),
            }
        })?;

        // Clamp end to available samples
        let end_samples = if let Some(total) = info.total_samples {
            end_samples.min(total)
        } else {
            end_samples
        };

        if start_samples >= end_samples {
            eprintln!(
                "Skipping track {}: empty range ({start_samples}..{end_samples})",
                track.number
            );
            continue;
        }

        // Build output filename: NN - Title.flac
        let title = track.title.as_deref().unwrap_or("Untitled");
        let filename = format!("{:02} - {title}.flac", track.number);
        let relative_path = std::path::Path::new(&filename);

        eprintln!(
            "Track {:02}: {} [{start_samples}..{end_samples}] → {}",
            track.number, title, filename
        );

        // Seek to start
        decoder.seek_to_sample(start_samples)?;

        // Calculate samples to read
        let num_samples = (end_samples - start_samples) as usize;
        let channels = info.channels as usize;
        let buffer_size = 4096usize;
        let mut read_buffer = vec![0i32; buffer_size * channels];

        // Encode via atomic writer
        let writer = safety::AtomicWriter::new(out_dir, relative_path)?;
        writer.write_with(|file| {
            let mut encoder = codec::flac::FlacEncoder::new(file, &info, 5)?;

            // Set basic metadata from CUE
            let mut tags = Vec::new();
            if let Some(ref t) = track.title {
                tags.push(("TITLE".to_string(), t.clone()));
            }
            if let Some(ref p) = track.performer {
                tags.push(("ARTIST".to_string(), p.clone()));
            } else if let Some(ref p) = cue_data.performer {
                tags.push(("ARTIST".to_string(), p.clone()));
            }
            if let Some(ref t) = cue_data.title {
                tags.push(("ALBUM".to_string(), t.clone()));
            }
            tags.push(("TRACKNUMBER".to_string(), format!("{}", track.number)));
            encoder.set_metadata(tags);

            // Stream decode → encode
            let mut remaining = num_samples;
            while remaining > 0 {
                let to_read = remaining.min(buffer_size);
                let read = decoder.read_samples(&mut read_buffer, to_read)?;
                if read == 0 {
                    break; // EOF
                }
                encoder.write_samples(&read_buffer[..read * channels], read)?;
                remaining -= read;
            }

            encoder.finish()?;
            Ok(())
        })?;
    }

    eprintln!("Done.");
    Ok(())
}
