//! cueblade CLI entry point.

use std::process;

use clap::Parser;

use cueblade::cli::{Cli, Mode};
use cueblade::pipeline;

fn main() {
    let cli = Cli::parse();

    let config = match cli.resolve() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(2);
        }
    };

    let result = match config.mode {
        Mode::Explicit {
            ref flac,
            ref cue,
            ref out,
        } => pipeline::run_explicit(flac, cue, out, &config),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
