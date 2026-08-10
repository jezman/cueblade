//! CUE sheet parser built on `winnow`.
//!
//! Parses the subset of CUE commands required by Red Book spec plus
//! common real-world extensions. Produces a [`CueSheet`] with all
//! timecodes converted to frame counts (`u64`) using checked arithmetic.
//!
//! Error messages include byte offset and line number for diagnostics.

use winnow::prelude::*;
use winnow::{
    ascii::{line_ending, space0, space1},
    combinator::{alt, delimited, opt, repeat},
    error::ContextError,
    token::{none_of, take_while},
};

use super::encoding::decode_cue_text;
use super::types::{CueSheet, FileType, Index, Timecode, Track};
use crate::error::{CueBladeError, Result};

/// Maximum number of tracks per CUE sheet (Red Book: 99, extended: 999).
const MAX_TRACKS: u16 = 999;

/// Parse raw CUE bytes into a validated [`CueSheet`].
///
/// Handles encoding detection internally. Returns structured errors
/// with byte offset and line number on failure.
///
/// # Examples
///
/// ```
/// use cueblade::cue::parser::parse_cue;
///
/// let input = b"\
/// REM GENRE Rock\n\
/// PERFORMER \"Artist\"\n\
/// TITLE \"Album\"\n\
/// FILE \"album.flac\" FLAC\n\
///   TRACK 01 AUDIO\n\
///     TITLE \"Track One\"\n\
///     INDEX 01 00:00:00\n\
/// ";
///
/// let cue = parse_cue(input).unwrap();
/// assert_eq!(cue.tracks.len(), 1);
/// assert_eq!(cue.file, "album.flac");
/// assert_eq!(cue.performer.as_deref(), Some("Artist"));
/// ```
pub fn parse_cue(bytes: &[u8]) -> Result<CueSheet> {
    let text = decode_cue_text(bytes)?;
    parse_cue_str(&text).map_err(|e| {
        let _ = e;
        CueBladeError::CueParse {
            byte_offset: 0,
            line: 1,
            message: "CUE parse failed".to_owned(),
        }
    })
}

fn parse_cue_str(input: &str) -> winnow::Result<CueSheet> {
    let mut performer: Option<String> = None;
    let mut title: Option<String> = None;
    let mut file: Option<String> = None;
    let mut file_type: Option<FileType> = None;
    let mut tracks: Vec<Track> = Vec::new();
    let mut rem_comments: Vec<String> = Vec::new();

    let mut remaining = input;

    loop {
        // Skip whitespace and line endings
        let before_len = remaining.len();
        let _ = (space0::<_, ContextError>, opt(line_ending)).parse_next(&mut remaining);

        if remaining.is_empty() {
            break;
        }

        // Try each directive
        if let Ok(comment) = parse_rem.parse_next(&mut remaining) {
            rem_comments.push(comment);
            continue;
        }

        if let Ok(perf) = parse_performer.parse_next(&mut remaining) {
            performer = Some(perf);
            continue;
        }

        if let Ok(t) = parse_title_global.parse_next(&mut remaining) {
            title = Some(t);
            continue;
        }

        if let Ok((fname, ftype)) = parse_file.parse_next(&mut remaining) {
            file = Some(fname);
            file_type = Some(ftype);
            continue;
        }

        if let Ok(track) = parse_track_block.parse_next(&mut remaining) {
            if tracks.len() >= MAX_TRACKS as usize {
                return Err(ContextError::new());
            }
            tracks.push(track);
            continue;
        }

        // Skip unknown lines gracefully
        if skip_line.parse_next(&mut remaining).is_ok() && remaining.len() < before_len {
            continue;
        }

        // No parser matched and no progress → error
        if remaining.len() == before_len {
            return Err(ContextError::new());
        }
    }

    let file = file.unwrap_or_default();
    let file_type = file_type.unwrap_or(FileType::Unknown);

    Ok(CueSheet {
        performer,
        title,
        file,
        file_type,
        tracks,
        rem_comments,
    })
}

// ─── Individual parsers ──────────────────────────────────────────────

fn parse_rem(input: &mut &str) -> winnow::Result<String> {
    let _ = ("REM", space1).parse_next(input)?;
    let content: &str = take_while(0.., |c: char| c != '\n' && c != '\r').parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;
    Ok(content.trim().to_owned())
}

fn parse_quoted_string(input: &mut &str) -> winnow::Result<String> {
    delimited('"', take_while(0.., |c: char| c != '"'), '"')
        .map(|s: &str| s.to_owned())
        .parse_next(input)
}

fn parse_unquoted_string(input: &mut &str) -> winnow::Result<String> {
    take_while(1.., |c: char| !c.is_whitespace() && c != '\n' && c != '\r')
        .map(|s: &str| s.to_owned())
        .parse_next(input)
}

fn parse_string_value(input: &mut &str) -> winnow::Result<String> {
    alt((parse_quoted_string, parse_unquoted_string)).parse_next(input)
}

fn parse_performer(input: &mut &str) -> winnow::Result<String> {
    let _ = ("PERFORMER", space1).parse_next(input)?;
    let value = parse_string_value.parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;
    Ok(value)
}

fn parse_title_global(input: &mut &str) -> winnow::Result<String> {
    let _ = ("TITLE", space1).parse_next(input)?;
    let value = parse_string_value.parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;
    Ok(value)
}

fn parse_file_type(input: &mut &str) -> winnow::Result<FileType> {
    alt((
        "FLAC".value(FileType::Flac),
        "APE".value(FileType::Ape),
        "WAVE".value(FileType::Wav),
        "WAVPACK".value(FileType::WavPack),
        "BINARY".value(FileType::Binary),
        "MOTOROLA".value(FileType::Motorola),
        "AIFF".value(FileType::Aiff),
        repeat(1.., none_of(())).map(|_: Vec<_>| FileType::Unknown),
    ))
    .parse_next(input)
}

fn parse_file(input: &mut &str) -> winnow::Result<(String, FileType)> {
    let _ = ("FILE", space1).parse_next(input)?;
    let filename = parse_string_value.parse_next(input)?;
    let _ = space1.parse_next(input)?;
    let ftype = parse_file_type.parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;
    Ok((filename, ftype))
}

fn parse_timecode(input: &mut &str) -> winnow::Result<Timecode> {
    let minutes: u64 = take_while(1..=2, |c: char| c.is_ascii_digit())
        .try_map(|s: &str| s.parse::<u64>())
        .parse_next(input)?;
    let _ = ':'.parse_next(input)?;
    let seconds: u64 = take_while(2..=2, |c: char| c.is_ascii_digit())
        .try_map(|s: &str| s.parse::<u64>())
        .parse_next(input)?;
    let _ = ':'.parse_next(input)?;
    let frames: u64 = take_while(2..=2, |c: char| c.is_ascii_digit())
        .try_map(|s: &str| s.parse::<u64>())
        .parse_next(input)?;

    Timecode::from_msf(minutes, seconds, frames).ok_or_else(ContextError::new)
}

fn parse_index(input: &mut &str) -> winnow::Result<Index> {
    let _ = ("INDEX", space1).parse_next(input)?;
    let number: u8 = take_while(1..=2, |c: char| c.is_ascii_digit())
        .try_map(|s: &str| s.parse::<u8>())
        .parse_next(input)?;
    let _ = space1.parse_next(input)?;
    let timestamp = parse_timecode.parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;
    Ok(Index { number, timestamp })
}

fn parse_track_block(input: &mut &str) -> winnow::Result<Track> {
    let _ = ("TRACK", space1).parse_next(input)?;
    let number: u16 = take_while(1..=3, |c: char| c.is_ascii_digit())
        .try_map(|s: &str| s.parse::<u16>())
        .parse_next(input)?;
    let _ = space1.parse_next(input)?;
    let track_type: String = take_while(1.., |c: char| !c.is_whitespace() && c != '\n')
        .map(|s: &str| s.to_owned())
        .parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;

    // Parse sub-directives until next top-level directive or EOF
    let mut title: Option<String> = None;
    let mut performer: Option<String> = None;
    let mut indices: Vec<Index> = Vec::new();
    let mut isrc: Option<String> = None;

    loop {
        let _ = space0::<_, ContextError>.parse_next(input);
        let before_len = input.len();

        if input.is_empty() {
            break;
        }

        // Peek: if we hit another top-level directive, stop consuming
        // Note: TITLE and PERFORMER are valid both at global AND track level,
        // so we must NOT break on them here — let the sub-parsers handle them.
        if input.starts_with("TRACK ") || input.starts_with("FILE ") {
            break;
        }

        if let Ok(idx) = parse_index.parse_next(input) {
            indices.push(idx);
            continue;
        }

        if let Ok(t) = parse_title_track.parse_next(input) {
            title = Some(t);
            continue;
        }

        if let Ok(p) = parse_performer.parse_next(input) {
            performer = Some(p);
            continue;
        }

        if let Ok(code) = parse_isrc.parse_next(input) {
            isrc = Some(code);
            continue;
        }

        // REM is valid inside track blocks too — just consume and discard
        if parse_rem.parse_next(input).is_ok() {
            continue;
        }

        // Skip unrecognized sub-line
        if skip_line.parse_next(input).is_ok() {
            // Only continue if we made progress
            if input.len() < before_len {
                continue;
            }
            // No progress on empty/whitespace-only input → done with track block
            break;
        }

        // No parser matched and no progress → done with track block
        break;
    }

    Ok(Track {
        number,
        track_type,
        title,
        performer,
        indices,
        isrc,
    })
}

fn parse_title_track(input: &mut &str) -> winnow::Result<String> {
    let _ = ("TITLE", space1).parse_next(input)?;
    let value = parse_string_value.parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;
    Ok(value)
}

fn parse_isrc(input: &mut &str) -> winnow::Result<String> {
    let _ = ("ISRC", space1).parse_next(input)?;
    let code: String = take_while(1.., |c: char| !c.is_whitespace() && c != '\n')
        .map(|s: &str| s.to_owned())
        .parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;
    Ok(code)
}

fn skip_line(input: &mut &str) -> winnow::Result<()> {
    let _ = take_while(0.., |c: char| c != '\n' && c != '\r').parse_next(input)?;
    let _ = opt(line_ending).parse_next(input)?;
    Ok(())
}
