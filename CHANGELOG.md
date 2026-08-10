# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-08-10

### Added

- **CUE Parser:** Winnow-based parser with UTF-8/CP1251 auto-detection, timestamp repair, and FILE fallback chain.
- **FLAC Pipeline:** Sample-accurate decode (`claxon`) and encode (`flacenc`) with configurable compression level (0–8).
- **Metadata Handling:**
  - Source FLAC Vorbis comments read via `metaflac` crate and preserved in output files.
  - CUE-derived tags (TITLE, ARTIST, ALBUM, TRACKNUMBER) override source tags.
  - REM DATE/GENRE comments used as fallback when absent in source.
  - Proper Vorbis Comment block injection into output FLAC via `flacenc` metadata API.
- **Explicit Mode:** `--flac`, `--cue`, `--out` flags for single-pair processing.
- **Gap Handling:** `prepend`, `append`, `discard` modes with sample-accurate range calculation.
- **Atomic Writes:** Temporary file + fsync + rename pattern for crash safety.
- **Template Engine:** Configurable output naming with `{artist}`, `{album}`, `{title}`, `{n}`, `{year}` placeholders.
- **Overwrite Policy:** `skip`, `overwrite`, `newer` modes.
- **CLI Flags:** `--template`, `--overwrite`, `--dry-run`, `--silent`, `--gap-handling`.
- **Error Taxonomy:** Structured error types with meaningful exit codes (0/1/2).
- **Safety:** Pure Rust, no unsafe, checked arithmetic, FLAC header validation.
- **Pipeline Architecture:** Modular `prepare_source` / `process_track` / `build_tags` design.
- **Testing:** 11 integration tests with golden files; unit tests for parser, sample math, sanitizer, encoder roundtrip.

### Security

- No `unsafe` code or FFI.
- All arithmetic uses checked operations.
- Input validation on FLAC headers (channels ≤ 8, bps ∈ {8,16,24,32}, sample rate > 0).
- Malformed CUE sheets produce descriptive errors without panics.
- Source files are opened read-only and never modified.

[0.1.1]: https://github.com/jezman/cueblade/releases/tag/v0.1.1
