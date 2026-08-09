//! CLI argument definitions using clap derive.
//!
//! Currently implements explicit mode only (`--flac`, `--cue`, `--out`).
//! Auto-discover, recursive, and extended flags are added in Phase 2+.

use std::path::PathBuf;

use clap::Parser;

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

impl Cli {
    /// Validate arguments and resolve into a concrete [`Mode`].
    ///
    /// # Errors
    ///
    /// Returns an error string if required arguments are missing
    /// or mutually exclusive flags are combined incorrectly.
    pub fn resolve(self) -> Result<Mode, String> {
        match (self.flac, self.cue) {
            (Some(flac), Some(cue)) => Ok(Mode::Explicit {
                flac,
                cue,
                out: self.out,
            }),
            (Some(_), None) => Err("--flac requires --cue".into()),
            (None, Some(_)) => Err("--cue requires --flac".into()),
            (None, None) => {
                Err("No mode specified. Use --flac and --cue for explicit mode.".into())
            }
        }
    }
}
