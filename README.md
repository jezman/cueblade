# cueblade

[![Crates.io](https://img.shields.io/crates/v/cueblade?logo=rust)](https://crates.io/crates/cueblade)
[![CI](https://github.com/jezman/cueblade/actions/workflows/ci.yml/badge.svg)](https://github.com/jezman/cueblade/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/cueblade.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](#minimum-supported-rust-version)

A fast, reliable, and safe lossless audio splitter for FLAC images driven by CUE sheets. Written in pure Rust with no unsafe code.

> **Status:** v0.1.2 (Phase 1 MVP). Currently supports explicit FLAC splitting with full metadata preservation. Auto-discovery, parallelism, and multi-format support are planned for future releases.

## Features (v0.1.0)

- **FLAC Pipeline:** Sample-accurate decode (`claxon`) and encode (`flacenc`) with configurable compression.
- **Metadata Preservation:** Reads source FLAC Vorbis comments via `metaflac`; CUE tags (TITLE, ARTIST, ALBUM, TRACKNUMBER) correctly override source values; REM DATE/GENRE used as fallback; proper Vorbis Comment block injection via `flacenc`.
- **CUE Parser:** Winnow-based parser with UTF-8/CP1251 auto-detection, timestamp repair, and FILE fallback chain.
- **Gap Handling:** `prepend`, `append`, `discard` modes with sample-accurate range calculation.
- **Atomic Writes:** Temporary file + fsync + rename pattern ensures crash safety — output is either complete or absent.
- **Explicit Mode:** Process a specific FLAC + CUE pair via CLI flags.
- **Template Engine:** Configurable output naming with `{artist}`, `{album}`, `{title}`, `{n}` placeholders.
- **Safety:** Pure Rust, no `unsafe`, checked arithmetic, input validation on all FLAC headers.
- **Testing:** 11 integration tests with golden files + comprehensive unit tests.

## Installation

### From crates.io

```bash
cargo install cueblade
```

### From Source

```bash
git clone https://github.com/jezman/cueblade.git
cd cueblade
cargo install --path .
```

## Usage

### Explicit Mode

Split a specific FLAC + CUE pair:

```bash
cueblade --flac album.flac --cue album.cue --out ./output/
```

### Common Flags

```bash
# Custom naming template
cueblade --flac album.flac --cue album.cue --out ./output/ \
  --template "{artist}/{year} - {album}/{n:02d} - {title}.flac"

# Skip existing files (default behavior)
cueblade --flac album.flac --cue album.cue --out ./output/ \
  --overwrite skip

# Overwrite existing files
cueblade --flac album.flac --cue album.cue --out ./output/ \
  --overwrite overwrite

# Preview without writing
cueblade --flac album.flac --cue album.cue --out ./output/ \
  --dry-run

# Discard pregaps instead of prepending
cueblade --flac album.flac --cue album.cue --out ./output/ \
  --gap-handling discard

# Silent mode for scripts
cueblade --flac album.flac --cue album.cue --out ./output/ \
  --silent
```

## CLI Reference

| Flag                    | Type   | Description                    | Default                  |
| ----------------------- | ------ | ------------------------------ | ------------------------ |
| `--flac <PATH>`         | Path   | Source FLAC file (required)    | —                        |
| `--cue <PATH>`          | Path   | CUE sheet file (required)      | —                        |
| `--out <DIR>`           | Path   | Output directory               | `./split/`               |
| `--template <TPL>`      | String | Naming template                | `{n:02d} - {title}.flac` |
| `--overwrite <MODE>`    | Enum   | `skip`, `overwrite`, `newer`   | `skip`                   |
| `--gap-handling <MODE>` | Enum   | `prepend`, `append`, `discard` | `prepend`                |
| `--dry-run`             | Bool   | Show plan without writing      | `false`                  |
| `--silent`              | Bool   | Suppress all non-error output  | `false`                  |

## Exit Codes

| Code | Meaning                                 |
| ---- | --------------------------------------- |
| 0    | All tracks split successfully           |
| 1    | Some tracks failed (details in stderr)  |
| 2    | Fatal error (invalid args, I/O failure) |

## Safety Guarantees

- Source files are opened read-only and never modified.
- Output files are atomic: crash at any point yields either a complete file or nothing.
- No `unsafe` code. No FFI. Pure Rust audio codecs (`claxon`, `flacenc`, `metaflac`).
- Arithmetic overflow in sample calculations is caught, not UB.
- Malformed CUE produces clear errors, not panics.
- FLAC header validation rejects unsupported channel counts, bit depths, and zero sample rates.

See [SECURITY.md](SECURITY.md) for the full security policy including input validation, container security, supply chain, and network safety details.

## Building from Source

```bash
# Requirements: Rust 1.85+
cargo build --release

# Tests
cargo test

# Clippy + fmt check
cargo fmt --check && cargo clippy -- -D warnings
```

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full development plan.

| Phase                            | Version | Status  | Key Features                                                                            |
| -------------------------------- | ------- | ------- | --------------------------------------------------------------------------------------- |
| **1. Core MVP**                  | v0.1.0  | ✅ Done | CUE parser, FLAC pipeline, explicit mode, metadata, atomic writes                       |
| **2. Discovery & Parallelism**   | v0.2.0  | Planned | Auto-discover, recursive walk, rayon, progress bar, undo journal, exclude filters       |
| **3. Verification & Enrichment** | v0.3.0  | Planned | Post-cut verify, MusicBrainz lookup, ReplayGain, embedded CUE, JSON output, config file |
| **4. Polish & Distribution**     | v0.4.0  | Planned | Pre-built binaries, shell completions, presets, Docker image, CI/CD, fuzz testing       |
| **5. Multi-Format**              | v0.5.0  | Planned | APE, WAV, WavPack input; auto-format detection; output format selection                 |

## Minimum Supported Rust Version

1.85.0 (edition 2024). MSRV bumps come with a minor version bump and are noted in the changelog.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
