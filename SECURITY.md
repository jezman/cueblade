# Security Policy

## Memory Safety

- **No unsafe code.** All audio I/O uses safe Rust crates (`claxon`, `flacenc`, `metaflac`). No FFI.
- **RAII resource management.** File handles and buffers are freed automatically on error paths.
- **Checked arithmetic.** Sample offset calculations use `checked_mul` and `checked_add`. Overflow produces `CueBladeError::Arithmetic`, never UB.
- **Bounded allocations.** No unbounded `Vec` growth. Task lists are bounded by input size.
- **Bounded channels.** Progress/journal channels have fixed capacity. Backpressure prevents OOM.

## File System Safety

- **Read-only source access.** Source files are opened via `File::open()` without write permissions.
- **Atomic commits.** Output is never partially visible. A crash at any point leaves either the old state or a complete new file.
- **Unique temp names.** UUID-based temp files prevent collisions in concurrent/multi-instance scenarios.
- **Path traversal protection.** Output paths are validated relative to the base directory. Symlinks in output are rejected unless explicitly allowed.
- **Permission preservation.** Output files inherit umask. No world-writable outputs.
- **Lock file.** Prevents concurrent writes to the same output directory. Flock-based with automatic cleanup on crash. _(Phase 2)_
- **Quota pre-check.** Estimates free space before batch start. 10% margin. Refuses to proceed if insufficient. _(Phase 2)_

## Input Validation

- **CUE size limit.** Max 10 MB per CUE sheet. Prevents DoS via malicious input.
- **Track count limit.** Max 999 tracks per CUE (Red Book spec). Exceeding produces an error, not a panic.
- **Timestamp validation.** Monotonicity checked. Negative/out-of-range timestamps are clamped or rejected depending on `--strict-cue`.
- **Encoding detection.** `chardetng` with fallback chain (UTF-8 → CP1251 → Shift-JIS → EUC-KR → Latin-1). Invalid UTF-8 after detection produces a graceful error with byte offset.
- **Audio header validation.** Sample rate, channels, and bits-per-sample are validated before processing. Unsupported configurations produce clear error messages.
- **FILE directive fallback.** Automatic file search by basename with extension chain. Not found → error listing all attempted paths.
- **Format auto-detection.** Magic bytes take priority over file extension. Prevents misidentification. _(Phase 5)_

## Operational Safety

- **No global mutable state.** All configuration is passed explicitly. Thread-safe by construction.
- **Graceful shutdown.** Ctrl+C handler sets an atomic flag. Workers check the flag between tracks. Partial results are preserved. _(Phase 2)_
- **Error isolation.** One corrupted track ≠ entire batch failure. Detailed per-track error reporting with taxonomy.
- **Exit codes.** Machine-readable: `0` (success), `1` (partial failure), `2` (fatal/config error).
- **Undo capability.** `--undo` reverts the last batch using the journal. Journal is written atomically. _(Phase 2)_
- **Dry-run mode.** Full simulation without writing. Validates all stages except I/O.

## Container Security _(Phase 4)_

- **Non-root user.** Image runs as `nobody:nobody`.
- **Read-only root filesystem.** `--read-only` in production.
- **No capabilities.** `--cap-drop ALL` by default.
- **Minimal base image.** Alpine + musl, ~15 MB. Minimal attack surface.
- **Volume-only write.** Output directory is mounted as a volume. Host filesystem is inaccessible.
- **No network by default.** `--network none` unless enrichment is needed.
- **Reproducible builds.** Multi-stage Dockerfile, pinned versions, no cache pollution.

## Supply Chain

- **Minimal dependencies.** Audited via `cargo audit` in CI. _(Phase 4)_
- **No FFI.** Pure Rust audio codec implementations eliminate C library vulnerabilities.
- **Pinned versions.** `Cargo.lock` is committed. Dependabot/Renovate for updates.
- **MSRV policy.** Minimum supported Rust version is documented. No surprise breakage.
- **Dependency review.** Each new crate (`claxon`, `flacenc`, `metaflac`, `winnow`) undergoes manual review for unsafe code, maintenance status, and license compatibility.

## Network Safety (Enrichment) _(Phase 3)_

- **HTTPS only.** MusicBrainz/CoverArt/AcoustID accessed exclusively over TLS.
- **Request timeout.** 10 s connect, 30 s read. Prevents hanging.
- **Rate limiting.** Respects API rate limits (1 req/sec for MusicBrainz). Retry with exponential backoff.
- **No credential storage.** All APIs are public; no tokens required.
- **Graceful fallback.** Network failure → skip enrichment, continue with CUE metadata only. Never blocks processing.

## Reporting Vulnerabilities

If you discover a security vulnerability, please report it privately via GitHub Security Advisories or email. Do not open a public issue. We will acknowledge receipt within 48 hours and aim to release a fix within 7 days for critical issues.
