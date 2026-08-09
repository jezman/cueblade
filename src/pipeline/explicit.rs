//! Explicit mode processing pipeline.
//!
//! Orchestrates discovery → parse → sanitize → gap calculation →
//! decode → encode → atomic write for a single source file.
//! Single-threaded; rayon parallelism is added in Phase 2.

use std::path::Path;

use crate::cli::ResolvedConfig;
use crate::codec::{self, Encoder};
use crate::cue;
use crate::discovery;
use crate::error::{CueBladeError, Result};
use crate::pipeline::gap::calculate_track_ranges;
use crate::pipeline::overwrite::{OverwriteDecision, OverwritePolicy};
use crate::safety;
use crate::template::{self, TemplateContext};

/// Execute explicit mode pipeline for a single audio + CUE pair.
pub fn run_explicit(
    flac_path: &Path,
    cue_path: &Path,
    out_dir: &Path,
    config: &ResolvedConfig,
) -> Result<()> {
    // 1. Discovery
    let source = discovery::discover_explicit(flac_path, cue_path)?;
    log(
        config,
        &format!(
            "Source: {} + {}",
            source.audio_path.display(),
            source.cue_path.display()
        ),
    );

    // 2. Parse CUE
    let cue_bytes = std::fs::read(&source.cue_path).map_err(|e| CueBladeError::Io {
        path: source.cue_path.clone(),
        source: e,
    })?;
    let cue_sheet = cue::parse_cue(&cue_bytes)?;
    log(
        config,
        &format!("Parsed CUE: {} tracks", cue_sheet.tracks.len()),
    );

    // 3. Sanitize
    let base_dir = source.cue_path.parent().unwrap_or(Path::new("."));
    let sanitized = cue::sanitize(cue_sheet, base_dir)?;
    log(
        config,
        &format!(
            "Sanitized: resolved audio = {}",
            sanitized.resolved_audio_path().display()
        ),
    );

    // 4. Open decoder
    let mut decoder = codec::open_decoder(sanitized.resolved_audio_path())?;
    let info = decoder.audio_info().clone();
    log(
        config,
        &format!(
            "Audio: {} Hz, {} ch, {} bps",
            info.sample_rate, info.channels, info.bits_per_sample
        ),
    );

    // 5. Ensure output directory exists (unless dry-run)
    if !config.dry_run {
        std::fs::create_dir_all(out_dir).map_err(|e| CueBladeError::Io {
            path: out_dir.to_path_buf(),
            source: e,
        })?;
    }

    // 6. Calculate track ranges with gap handling
    let cue_data = sanitized.cue();
    let track_ranges = calculate_track_ranges(
        cue_data,
        info.sample_rate,
        info.total_samples,
        config.gap_handling,
    )?;
    log(
        config,
        &format!(
            "Calculated {} track ranges ({:?})",
            track_ranges.len(),
            config.gap_handling
        ),
    );

    // 7. Setup overwrite policy
    let overwrite_policy = OverwritePolicy::new(config.overwrite);

    // 8. Process each track
    let mut skipped = 0usize;
    let mut written = 0usize;

    for range in &track_ranges {
        let track = cue_data
            .tracks
            .iter()
            .find(|t| t.number == range.track_number)
            .ok_or_else(|| CueBladeError::Sanitization {
                reason: format!("Track {} not found in CUE data", range.track_number),
            })?;

        let start_samples = range.start_sample;
        let end_samples = range.end_sample;

        // Render output filename
        let ctx = TemplateContext::from_track(track, cue_data);
        let relative_path_str = template::render_template(&config.template, &ctx)?;
        let relative_path = Path::new(&relative_path_str);
        let output_path = out_dir.join(relative_path);

        // Check overwrite policy
        match overwrite_policy.check(&output_path, &source.audio_path)? {
            OverwriteDecision::Skip => {
                log(
                    config,
                    &format!(
                        "Skipping track {:02}: {} → {} (exists)",
                        track.number, ctx.title, relative_path_str
                    ),
                );
                skipped += 1;
                continue;
            }
            OverwriteDecision::Write => {}
        }

        if config.dry_run {
            log(
                config,
                &format!(
                    "[DRY-RUN] Track {:02}: {} [{start_samples}..{end_samples}] → {}",
                    track.number, ctx.title, relative_path_str
                ),
            );
            written += 1;
            continue;
        }

        log(
            config,
            &format!(
                "Track {:02}: {} [{start_samples}..{end_samples}] → {}",
                track.number, ctx.title, relative_path_str
            ),
        );

        // Seek to start
        decoder.seek_to_sample(start_samples)?;

        // Stream decode → encode via atomic writer
        let num_samples = (end_samples - start_samples) as usize;
        let channels = info.channels as usize;
        let buffer_size = 4096usize;
        let mut read_buffer = vec![0i32; buffer_size * channels];

        let writer = safety::AtomicWriter::new(out_dir, relative_path)?;
        writer.write_with(|file| {
            let mut encoder = codec::flac::FlacEncoder::new(file, &info, 5)?;

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

            let mut remaining = num_samples;
            while remaining > 0 {
                let to_read = remaining.min(buffer_size);
                let read = decoder.read_samples(&mut read_buffer, to_read)?;
                if read == 0 {
                    break;
                }
                encoder.write_samples(&read_buffer[..read * channels], read)?;
                remaining -= read;
            }

            encoder.finish()?;
            Ok(())
        })?;

        written += 1;
    }

    log(
        config,
        &format!("Done. Written: {written}, Skipped: {skipped}"),
    );
    Ok(())
}

/// Print a message unless silent mode is active.
fn log(config: &ResolvedConfig, msg: &str) {
    if !config.silent {
        eprintln!("{msg}");
    }
}
