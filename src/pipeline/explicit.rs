//! Explicit mode processing pipeline.
//!
//! Orchestrates discovery → parse → sanitize → gap calculation →
//! decode → encode → atomic write for a single source file.
//! Single-threaded; rayon parallelism is added in Phase 2.

use std::path::{Path, PathBuf};

use crate::cli::ResolvedConfig;
use crate::codec::{self, Decoder, Encoder};
use crate::cue;
use crate::discovery;
use crate::error::{CueBladeError, Result};
use crate::pipeline::gap::calculate_track_ranges;
use crate::pipeline::overwrite::{OverwriteDecision, OverwritePolicy};
use crate::safety;
use crate::template::{self, TemplateContext};

/// Context holding a prepared source ready for track extraction.
struct PreparedSource {
    decoder: Box<dyn Decoder>,
    cue_data: cue::CueSheet,
    track_ranges: Vec<crate::pipeline::gap::TrackRange>,
    /// Path to the resolved source audio file (needed for overwrite checks).
    source_audio_path: PathBuf,
}

/// Execute explicit mode pipeline for a single audio + CUE pair.
pub fn run_explicit(
    flac_path: &Path,
    cue_path: &Path,
    out_dir: &Path,
    config: &ResolvedConfig,
) -> Result<()> {
    // 1. Prepare source (discovery, parse, sanitize, open decoder, gaps)
    let mut prepared = prepare_source(flac_path, cue_path, config)?;

    // 2. Ensure output directory exists (unless dry-run)
    if !config.dry_run {
        std::fs::create_dir_all(out_dir).map_err(|e| CueBladeError::Io {
            path: out_dir.to_path_buf(),
            source: e,
        })?;
    }

    // 3. Setup overwrite policy
    let overwrite_policy = OverwritePolicy::new(config.overwrite);

    // 4. Process each track
    let mut skipped = 0usize;
    let mut written = 0usize;

    for range in &prepared.track_ranges {
        match process_track(
            &mut prepared.decoder,
            &prepared.cue_data,
            &prepared.source_audio_path,
            range,
            out_dir,
            config,
            &overwrite_policy,
        )? {
            TrackResult::Written => written += 1,
            TrackResult::Skipped => skipped += 1,
            TrackResult::DryRun => written += 1,
        }
    }

    log(
        config,
        &format!("Done. Written: {written}, Skipped: {skipped}"),
    );
    Ok(())
}

/// Prepares the source: parses CUE, opens decoder, calculates ranges.
fn prepare_source(
    flac_path: &Path,
    cue_path: &Path,
    config: &ResolvedConfig,
) -> Result<PreparedSource> {
    // Discovery
    let source = discovery::discover_explicit(flac_path, cue_path)?;
    log(
        config,
        &format!(
            "Source: {} + {}",
            source.audio_path.display(),
            source.cue_path.display()
        ),
    );

    // Parse CUE
    let cue_bytes = std::fs::read(&source.cue_path).map_err(|e| CueBladeError::Io {
        path: source.cue_path.clone(),
        source: e,
    })?;
    let cue_sheet = cue::parse_cue(&cue_bytes)?;
    log(
        config,
        &format!("Parsed CUE: {} tracks", cue_sheet.tracks.len()),
    );

    // Sanitize
    let base_dir = source.cue_path.parent().unwrap_or(Path::new("."));
    let sanitized = cue::sanitize(cue_sheet, base_dir)?;
    let resolved_audio = sanitized.resolved_audio_path().to_path_buf();
    log(
        config,
        &format!("Sanitized: resolved audio = {}", resolved_audio.display()),
    );

    // Open decoder
    let decoder = codec::open_decoder(&resolved_audio)?;
    let info = decoder.audio_info().clone();
    log(
        config,
        &format!(
            "Audio: {} Hz, {} ch, {} bps",
            info.sample_rate, info.channels, info.bits_per_sample
        ),
    );

    // Calculate track ranges with gap handling
    let cue_data = sanitized.cue().clone();
    let track_ranges = calculate_track_ranges(
        &cue_data,
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

    Ok(PreparedSource {
        decoder,
        cue_data,
        track_ranges,
        source_audio_path: resolved_audio,
    })
}

enum TrackResult {
    Written,
    Skipped,
    DryRun,
}

/// Processes a single track: builds tags, seeks, decodes, encodes, writes atomically.
#[allow(clippy::too_many_arguments)]
fn process_track(
    decoder: &mut Box<dyn Decoder>,
    cue_data: &cue::CueSheet,
    source_audio_path: &Path,
    range: &crate::pipeline::gap::TrackRange,
    out_dir: &Path,
    config: &ResolvedConfig,
    overwrite_policy: &OverwritePolicy,
) -> Result<TrackResult> {
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

    // Check overwrite policy against SOURCE audio path
    match overwrite_policy.check(&output_path, source_audio_path)? {
        OverwriteDecision::Skip => {
            log(
                config,
                &format!(
                    "Skipping track {:02}: {} → {} (exists)",
                    track.number, ctx.title, relative_path_str
                ),
            );
            return Ok(TrackResult::Skipped);
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
        return Ok(TrackResult::DryRun);
    }

    log(
        config,
        &format!(
            "Track {:02}: {} [{start_samples}..{end_samples}] → {}",
            track.number, ctx.title, relative_path_str
        ),
    );

    // Build tags
    let tags = build_tags(decoder.tags(), track, cue_data);

    // Seek to start
    decoder.seek_to_sample(start_samples)?;

    // Stream decode → encode via atomic writer
    let num_samples = (end_samples - start_samples) as usize;
    let channels = decoder.audio_info().channels as usize;
    let buffer_size = 4096usize;
    let mut read_buffer = vec![0i32; buffer_size * channels];

    let writer = safety::AtomicWriter::new(out_dir, relative_path)?;
    writer.write_with(|file| {
        let mut encoder = codec::flac::FlacEncoder::new(file, decoder.audio_info(), 5)?;
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

    Ok(TrackResult::Written)
}

/// Insert or update a tag in the list. Removes existing entries with the same key first.
fn upsert_tag(tags: &mut Vec<(String, String)>, key: &str, value: String) {
    tags.retain(|(k, _)| k != key);
    tags.push((key.to_string(), value));
}

/// Builds final tag list: source FLAC tags + CUE overrides.
fn build_tags(
    source_tags: &[(String, String)],
    track: &cue::Track,
    cue_data: &cue::CueSheet,
) -> Vec<(String, String)> {
    let mut tags = source_tags.to_vec();

    // CUE-derived overrides
    if let Some(ref t) = track.title {
        upsert_tag(&mut tags, "TITLE", t.clone());
    }
    if let Some(ref p) = track.performer {
        upsert_tag(&mut tags, "ARTIST", p.clone());
    } else if let Some(ref p) = cue_data.performer {
        upsert_tag(&mut tags, "ARTIST", p.clone());
    }
    if let Some(ref t) = cue_data.title {
        upsert_tag(&mut tags, "ALBUM", t.clone());
    }
    upsert_tag(&mut tags, "TRACKNUMBER", format!("{}", track.number));

    // REM comments fallback
    for comment in &cue_data.rem_comments {
        if let Some(date) = comment.strip_prefix("DATE ") {
            if !tags.iter().any(|(k, _)| k == "DATE") {
                upsert_tag(&mut tags, "DATE", date.trim().trim_matches('"').to_string());
            }
        } else if let Some(genre) = comment.strip_prefix("GENRE ") {
            if !tags.iter().any(|(k, _)| k == "GENRE") {
                upsert_tag(
                    &mut tags,
                    "GENRE",
                    genre.trim().trim_matches('"').to_string(),
                );
            }
        }
    }

    tags
}

/// Print a message unless silent mode is active.
fn log(config: &ResolvedConfig, msg: &str) {
    if !config.silent {
        eprintln!("{msg}");
    }
}
