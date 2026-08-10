# Roadmap

## Phase 1: Core MVP (v0.1.0) ✅

- [x] CUE parser (winnow) with UTF-8/CP1251 support
- [x] CUE sanitizer: encoding auto-detect, timestamp repair, FILE fallback chain
- [x] FLAC stream decode/encode pipeline
- [x] Atomic writer with fsync
- [x] Explicit mode (`--flac`, `--cue`, `--out`)
- [x] Basic flags: `--template`, `--overwrite skip`, `--dry-run`, `--silent`
- [x] Gap handling (`prepend`, `append`, `discard`)
- [x] Error handling with taxonomy and exit codes
- [x] Unit tests for parser, sample math, sanitizer
- [x] Integration tests with golden files
- [x] Metadata preservation: source Vorbis comments + CUE overrides
- [x] Pipeline refactoring: modular `prepare_source` / `process_track` / `build_tags`
- [x] CI: test, clippy, fmt, MSRV check

## Phase 2: Discovery, Parallelism & Safety (v0.2.0)

- [ ] Auto-discover mode (current directory)
- [ ] Recursive mode (subdirectories)
- [ ] Multi-file CUE support
- [ ] CUE-audio pairing logic with extension fallback
- [ ] Exclude filter (`--exclude <GLOB>`, repeatable, directory support via `**/dir/**`)
- [ ] Rayon worker pool
- [ ] Task grouping by source file
- [ ] Progress bar (indicatif) + silent mode
- [ ] Undo journal + `--undo` flag
- [ ] Lock file
- [ ] Quota pre-check
- [ ] Graceful Ctrl+C handling
- [ ] Benchmarks suite
- [ ] Flags: `--threads`, `--buffer-size`, `--no-fsync`, `--sequential`

## Phase 3: Verification & Enrichment (v0.3.0)

- [ ] Post-cut verification (`--verify`, `--checksum`)
- [ ] MusicBrainz/Discogs lookup by TOC (async, cached)
- [ ] Cover art fetching + embedding
- [ ] Duplicate detection (AcoustID / content hash)
- [ ] ReplayGain / R128 loudness (track + album modes, two-pass)
- [ ] Embedded CUE support (`--embed-cue`, `--extract-cue`)
- [ ] Strict CUE mode (`--strict-cue`)
- [ ] JSON output mode (`--json`)
- [ ] Log file + log level (`--log-file`, `--log-level`)
- [ ] Config file support (`--config`, TOML)
- [ ] Performance metrics summary
- [ ] Comprehensive error messages with spans

## Phase 4: Polish & Distribution (v0.4.0)

- [ ] Pre-built binaries (Linux/macOS/Windows via cross)
- [ ] Shell completions (clap_complete)
- [ ] Man page generation (clap_mangen)
- [ ] Preset system (`--preset archival/streaming/dj`)
- [ ] IO priority + CPU affinity flags
- [ ] Max memory limit (`--max-memory`)
- [ ] Fuzz testing for CUE parser
- [ ] Docker image published to GHCR

## Phase 5: Multi-Format Support (v0.5.0)

- [ ] APE input support (via `symphonia` or `ape-rs`)
- [ ] WAV input support (PCM, RF64)
- [ ] WavPack input support
- [ ] Auto-format detection by magic bytes + extension fallback
- [ ] Format-specific decoder/encoder abstraction trait
- [ ] Output format selection (`--output-format flac|wav`)
- [ ] Updated documentation and examples for all formats
- [ ] Integration tests for each format
