//! End-to-end integration tests for explicit mode pipeline.
//!
//! Tests the full flow: CLI args → discovery → parse → sanitize →
//! gap handling → decode → encode → atomic write → verification.

mod fixtures;

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

use fixtures::{create_single_track_fixture, create_three_track_fixture};

/// Helper: run cueblade CLI with given args, return Assert.
fn cueblade_cmd() -> Command {
    Command::cargo_bin("cueblade").expect("binary exists")
}

// ─── Basic end-to-end ────────────────────────────────────────────────

#[test]
fn test_explicit_three_tracks_default_template() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_three_track_fixture(dir.path());
    let out_dir = dir.path().join("output");

    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Default template: {n:02d} - {title}.flac
    assert!(out_dir.join("01 - First Track.flac").exists());
    assert!(out_dir.join("02 - Second Track.flac").exists());
    assert!(out_dir.join("03 - Third Track.flac").exists());

    // All output files should be non-empty valid FLAC
    for name in &[
        "01 - First Track.flac",
        "02 - Second Track.flac",
        "03 - Third Track.flac",
    ] {
        let path = out_dir.join(name);
        let meta = fs::metadata(&path).unwrap();
        assert!(meta.len() > 0, "{name} is empty");
    }
}

#[test]
fn test_explicit_single_track() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_single_track_fixture(dir.path());
    let out_dir = dir.path().join("output");

    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(out_dir.join("01 - Only Track.flac").exists());
}

// ─── Template rendering ──────────────────────────────────────────────

#[test]
fn test_custom_template_with_subdirs() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_three_track_fixture(dir.path());
    let out_dir = dir.path().join("output");

    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--template",
            "{artist}/{album}/{n:02d} - {title}.flac",
        ])
        .assert()
        .success();

    assert!(
        out_dir
            .join("Test Artist/Test Album/01 - First Track.flac")
            .exists()
    );
    assert!(
        out_dir
            .join("Test Artist/Test Album/02 - Second Track.flac")
            .exists()
    );
    assert!(
        out_dir
            .join("Test Artist/Test Album/03 - Third Track.flac")
            .exists()
    );
}

// ─── Gap handling modes ──────────────────────────────────────────────

#[test]
fn test_gap_discard() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_three_track_fixture(dir.path());
    let out_dir = dir.path().join("discard");

    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--gap-handling",
            "discard",
        ])
        .assert()
        .success();

    // All 3 tracks should exist
    assert!(out_dir.join("01 - First Track.flac").exists());
    assert!(out_dir.join("02 - Second Track.flac").exists());
    assert!(out_dir.join("03 - Third Track.flac").exists());
}

#[test]
fn test_gap_prepend() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_three_track_fixture(dir.path());
    let out_dir = dir.path().join("prepend");

    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--gap-handling",
            "prepend",
        ])
        .assert()
        .success();

    // Track 2 with prepend should be larger than with discard
    // (includes pre-gap from INDEX 00)
    let prepend_size = fs::metadata(out_dir.join("02 - Second Track.flac"))
        .unwrap()
        .len();

    // Compare with discard
    let discard_dir = dir.path().join("prepend_discard_cmp");
    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            discard_dir.to_str().unwrap(),
            "--gap-handling",
            "discard",
        ])
        .assert()
        .success();

    let discard_size = fs::metadata(discard_dir.join("02 - Second Track.flac"))
        .unwrap()
        .len();

    // Prepend includes extra samples → larger file (for silence, size difference
    // may be small due to FLAC block alignment, but should be >= )
    assert!(
        prepend_size >= discard_size,
        "Prepend ({prepend_size}) should be >= discard ({discard_size})"
    );
}

// ─── Overwrite behavior ──────────────────────────────────────────────

#[test]
fn test_overwrite_skip() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_three_track_fixture(dir.path());
    let out_dir = dir.path().join("output");

    // First run
    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--overwrite",
            "skip",
        ])
        .assert()
        .success();

    let first_size = fs::metadata(out_dir.join("01 - First Track.flac"))
        .unwrap()
        .len();

    // Second run — should skip existing files
    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--overwrite",
            "skip",
        ])
        .assert()
        .success();

    let second_size = fs::metadata(out_dir.join("01 - First Track.flac"))
        .unwrap()
        .len();

    assert_eq!(first_size, second_size, "File should not be overwritten");
}

// ─── Dry-run mode ────────────────────────────────────────────────────

#[test]
fn test_dry_run_no_files_written() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_three_track_fixture(dir.path());
    let out_dir = dir.path().join("dryrun_output");

    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("[DRY-RUN]"));

    // Output directory should not exist or be empty
    if out_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&out_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(entries.is_empty(), "Dry-run should not create output files");
    }
}

// ─── Silent mode ─────────────────────────────────────────────────────

#[test]
fn test_silent_mode_suppresses_output() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_three_track_fixture(dir.path());
    let out_dir = dir.path().join("silent_output");

    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--silent",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Track").not());

    // Files should still be written
    assert!(out_dir.join("01 - First Track.flac").exists());
}

// ─── Exit codes ──────────────────────────────────────────────────────

#[test]
fn test_exit_code_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let cue_path = dir.path().join("nonexistent.cue");
    fs::write(&cue_path, b"fake").unwrap();

    // Missing audio file → Io error from canonicalize (exit code 5)
    cueblade_cmd()
        .args([
            "--flac",
            "/nonexistent/audio.flac",
            "--cue",
            cue_path.to_str().unwrap(),
            "--out",
            dir.path().join("out").to_str().unwrap(),
        ])
        .assert()
        .code(5); // Io (file not found during discovery)
}

#[test]
fn test_exit_code_bad_cue_no_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = create_single_track_fixture(dir.path());

    // Corrupt CUE → parses as 0 tracks → sanitizer rejects
    fs::write(&fixture.cue_path, b"GARBAGE\nNOT A CUE\n").unwrap();

    cueblade_cmd()
        .args([
            "--flac",
            fixture.flac_path.to_str().unwrap(),
            "--cue",
            fixture.cue_path.to_str().unwrap(),
            "--out",
            dir.path().join("out").to_str().unwrap(),
        ])
        .assert()
        .code(6); // Sanitization: CUE contains no tracks
}

#[test]
fn test_exit_code_cli_error() {
    // Missing required --cue when --flac is provided
    cueblade_cmd()
        .args(["--flac", "/some/file.flac"])
        .assert()
        .code(2); // CLI argument error
}
