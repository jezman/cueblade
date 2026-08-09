//! CLI argument definitions using clap derive.
//!
//! Currently implements explicit mode only (`--flac`, `--cue`, `--out`).
//! Auto-discover, recursive, and extended flags are added in Phase 2+.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Fast, reliable, and safe splitter for lossless audio images by CUE sheets.
#[derive(Debug, Parser)]
#[command(name = "cueblade", version, about)]
pub struct Cli {
    /// Source audio file (explicit mode).
    #[arg(long)]
    pub flac: Option<PathBuf>,

    /// CUE sheet file (explicit mode).
    #[arg(long)]
    pub cue: Option<PathBuf>,

    /// Output directory.
    #[arg(long, default_value = "./split")]
    pub out: PathBuf,

    /// Naming template with variables: {artist}, {album}, {title}, {n}, {n:02d}.
    #[arg(long, default_value = "{n:02d} - {title}.flac")]
    pub template: String,

    /// Overwrite policy for existing output files.
    #[arg(long, value_enum, default_value_t = OverwriteMode::Skip)]
    pub overwrite: OverwriteMode,

    /// How to handle pre-gap regions between tracks.
    #[arg(long, value_enum, default_value_t = GapMode::Prepend)]
    pub gap_handling: GapMode,

    /// Show processing plan without writing any files.
    #[arg(long)]
    pub dry_run: bool,

    /// Suppress all non-error output.
    #[arg(long)]
    pub silent: bool,
}

/// Overwrite policy for existing output files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OverwriteMode {
    /// Skip tracks whose output file already exists.
    Skip,
    /// Always overwrite existing output files.
    Overwrite,
    /// Overwrite only if source audio is newer than output.
    Newer,
}

/// Gap handling mode for pre-gap regions between tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GapMode {
    /// Include pre-gap in the current track (extend start to INDEX 00).
    Prepend,
    /// Include pre-gap of next track at end of current track.
    Append,
    /// Discard all pre-gap regions (use only INDEX 01 boundaries).
    Discard,
}

/// Resolved CLI mode after argument validation.
#[derive(Debug)]
pub enum Mode {
    /// Explicit mode: user specified --flac and --cue directly.
    Explicit {
        flac: PathBuf,
        cue: PathBuf,
        out: PathBuf,
    },
}

/// Fully resolved configuration combining mode and flags.
#[derive(Debug)]
pub struct ResolvedConfig {
    /// Active processing mode.
    pub mode: Mode,
    /// Naming template string.
    pub template: String,
    /// Overwrite policy.
    pub overwrite: OverwriteMode,
    /// Gap handling mode.
    pub gap_handling: GapMode,
    /// Dry-run mode enabled.
    pub dry_run: bool,
    /// Silent mode enabled.
    pub silent: bool,
}

impl Cli {
    /// Validate arguments and resolve into a [`ResolvedConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error string if required arguments are missing
    /// or mutually exclusive flags are combined incorrectly.
    pub fn resolve(self) -> Result<ResolvedConfig, String> {
        let mode = match (self.flac, self.cue) {
            (Some(flac), Some(cue)) => Mode::Explicit {
                flac,
                cue,
                out: self.out,
            },
            (Some(_), None) => return Err("--flac requires --cue".to_owned()),
            (None, Some(_)) => return Err("--cue requires --flac".to_owned()),
            (None, None) => {
                return Err("No mode specified. Use --flac and --cue for explicit mode.".to_owned());
            }
        };

        Ok(ResolvedConfig {
            mode,
            template: self.template,
            overwrite: self.overwrite,
            gap_handling: self.gap_handling,
            dry_run: self.dry_run,
            silent: self.silent,
        })
    }
}
