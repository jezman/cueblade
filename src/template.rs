//! Simple template engine for output file naming.
//!
//! Supports variables: `{artist}`, `{album}`, `{title}`, `{n}`, `{n:02d}`.
//! Hand-rolled parser with no external dependencies.
//! All rendered paths are sanitized to prevent filesystem issues.

use crate::cue::types::{CueSheet, Track};
use crate::error::{CueBladeError, Result};

/// Context values available for template rendering.
#[derive(Debug, Clone)]
pub struct TemplateContext {
    /// Track performer → global performer fallback.
    pub artist: String,
    /// Global title (album name).
    pub album: String,
    /// Track title.
    pub title: String,
    /// Track number.
    pub track_number: u16,
}

impl TemplateContext {
    /// Build context from a track and its parent CUE sheet.
    ///
    /// Artist falls back: track.performer → cue.performer → "Unknown Artist".
    /// Title falls back: track.title → "Untitled".
    /// Album falls back: cue.title → "Unknown Album".
    ///
    /// # Examples
    ///
    /// ```
    /// use cueblade::template::TemplateContext;
    /// use cueblade::cue::types::{CueSheet, Track, FileType};
    ///
    /// let cue = CueSheet {
    ///     performer: Some("Global Artist".into()),
    ///     title: Some("Album".into()),
    ///     file: "test.flac".into(),
    ///     file_type: FileType::Flac,
    ///     tracks: vec![],
    ///     rem_comments: vec![],
    /// };
    /// let track = Track {
    ///     number: 3,
    ///     track_type: "AUDIO".into(),
    ///     title: Some("Song".into()),
    ///     performer: None,
    ///     indices: vec![],
    ///     isrc: None,
    /// };
    /// let ctx = TemplateContext::from_track(&track, &cue);
    /// assert_eq!(ctx.artist, "Global Artist");
    /// assert_eq!(ctx.track_number, 3);
    /// ```
    pub fn from_track(track: &Track, cue: &CueSheet) -> Self {
        Self {
            artist: track
                .performer
                .as_deref()
                .or(cue.performer.as_deref())
                .unwrap_or("Unknown Artist")
                .to_owned(),
            album: cue.title.as_deref().unwrap_or("Unknown Album").to_owned(),
            title: track.title.as_deref().unwrap_or("Untitled").to_owned(),
            track_number: track.number,
        }
    }
}

/// Render a template string with the given context.
///
/// Supported variables:
/// - `{artist}` — performer name
/// - `{album}` — album title
/// - `{title}` — track title
/// - `{n}` — track number (no padding)
/// - `{n:02d}` — track number zero-padded to 2 digits
/// - `{n:03d}` — track number zero-padded to 3 digits
///
/// Unknown variables are left as-is (e.g., `{unknown}` → `{unknown}`).
/// Output is sanitized: path-unsafe characters replaced with `_`.
///
/// # Errors
///
/// Returns [`CueBladeError::Sanitization`] if template is empty.
///
/// # Examples
///
/// ```
/// use cueblade::template::{render_template, TemplateContext};
///
/// let ctx = TemplateContext {
///     artist: "Artist".into(),
///     album: "Album".into(),
///     title: "Song".into(),
///     track_number: 5,
/// };
///
/// assert_eq!(
///     render_template("{n:02d} - {title}.flac", &ctx).unwrap(),
///     "05 - Song.flac"
/// );
/// assert_eq!(
///     render_template("{artist}/{album}/{n:02d} - {title}.flac", &ctx).unwrap(),
///     "Artist/Album/05 - Song.flac"
/// );
/// ```
pub fn render_template(template: &str, ctx: &TemplateContext) -> Result<String> {
    if template.is_empty() {
        return Err(CueBladeError::Sanitization {
            reason: "Template string cannot be empty".into(),
        });
    }

    let mut result = String::with_capacity(template.len() + 32);
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            // Collect variable name until '}'
            let mut var_name = String::new();
            let mut found_close = false;
            for vc in chars.by_ref() {
                if vc == '}' {
                    found_close = true;
                    break;
                }
                var_name.push(vc);
            }

            if !found_close {
                // Malformed: no closing brace, emit literally
                result.push('{');
                result.push_str(&var_name);
                continue;
            }

            // Resolve variable
            let replacement = resolve_variable(&var_name, ctx);
            result.push_str(&replacement);
        } else {
            result.push(c);
        }
    }

    Ok(sanitize_path(&result))
}

/// Resolve a single template variable to its string value.
fn resolve_variable(var: &str, ctx: &TemplateContext) -> String {
    match var {
        "artist" => ctx.artist.clone(),
        "album" => ctx.album.clone(),
        "title" => ctx.title.clone(),
        "n" => ctx.track_number.to_string(),
        "n:02d" => format!("{:02}", ctx.track_number),
        "n:03d" => format!("{:03}", ctx.track_number),
        other => format!("{{{other}}}"), // unknown → leave as-is
    }
}

/// Sanitize a rendered path string for filesystem safety.
///
/// Replaces characters unsafe on Windows/Linux/macOS with `_`.
/// Preserves `/` as directory separator and `.` for extensions.
fn sanitize_path(path: &str) -> String {
    path.chars()
        .map(|c| match c {
            '/' | '.' | '-' | '_' | '(' | ')' | '[' | ']' => c,
            c if c.is_alphanumeric() || c == ' ' => c,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> TemplateContext {
        TemplateContext {
            artist: "Test Artist".into(),
            album: "Test Album".into(),
            title: "Test Song".into(),
            track_number: 7,
        }
    }

    #[test]
    fn test_render_basic_template() {
        let ctx = test_ctx();
        assert_eq!(
            render_template("{n:02d} - {title}.flac", &ctx).unwrap(),
            "07 - Test Song.flac"
        );
    }

    #[test]
    fn test_render_full_template() {
        let ctx = test_ctx();
        assert_eq!(
            render_template("{artist}/{album}/{n:02d} - {title}.flac", &ctx).unwrap(),
            "Test Artist/Test Album/07 - Test Song.flac"
        );
    }

    #[test]
    fn test_render_n_no_padding() {
        let ctx = test_ctx();
        assert_eq!(
            render_template("{n} - {title}.flac", &ctx).unwrap(),
            "7 - Test Song.flac"
        );
    }

    #[test]
    fn test_render_n_03d() {
        let ctx = test_ctx();
        assert_eq!(
            render_template("{n:03d} - {title}.flac", &ctx).unwrap(),
            "007 - Test Song.flac"
        );
    }

    #[test]
    fn test_render_unknown_variable() {
        let ctx = test_ctx();
        // Unknown variables are left as-is but sanitized ({ and } → _)
        assert_eq!(
            render_template("{unknown} - {title}.flac", &ctx).unwrap(),
            "_unknown_ - Test Song.flac"
        );
    }

    #[test]
    fn test_render_empty_template_error() {
        let ctx = test_ctx();
        assert!(render_template("", &ctx).is_err());
    }

    #[test]
    fn test_sanitize_unsafe_chars() {
        assert_eq!(sanitize_path("file<>:\"name.flac"), "file____name.flac");
        assert_eq!(
            sanitize_path("normal/path/file.flac"),
            "normal/path/file.flac"
        );
    }

    #[test]
    fn test_from_track_fallbacks() {
        use crate::cue::types::{CueSheet, FileType, Track};

        let cue = CueSheet {
            performer: None,
            title: None,
            file: "test.flac".into(),
            file_type: FileType::Flac,
            tracks: vec![],
            rem_comments: vec![],
        };
        let track = Track {
            number: 1,
            track_type: "AUDIO".into(),
            title: None,
            performer: None,
            indices: vec![],
            isrc: None,
        };
        let ctx = TemplateContext::from_track(&track, &cue);
        assert_eq!(ctx.artist, "Unknown Artist");
        assert_eq!(ctx.album, "Unknown Album");
        assert_eq!(ctx.title, "Untitled");
    }
}
