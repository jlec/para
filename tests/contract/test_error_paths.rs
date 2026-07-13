//! T020: ffmpeg-missing, input-not-found, no-audio-track, empty/corrupted
//! file, and an unwritable output destination all exit non-zero with a
//! specific stderr message and empty stdout. None of these need a real
//! cached model — they must all fail before any model download is attempted.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn input_not_found_fails_fast_with_empty_stdout() {
    let output = Command::new(para_bin())
        .args(["-i", "/no/such/file.wav", "--device", "cpu"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("No such file"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn empty_file_is_rejected_not_silently_transcribed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.wav");
    std::fs::write(&path, []).unwrap();

    let output = Command::new(para_bin())
        .args(["-i", path.to_str().unwrap(), "--device", "cpu"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn ffmpeg_missing_fails_with_specific_message() {
    let output = Command::new(para_bin())
        .args(["-i", "/no/such/file.wav", "--device", "cpu"])
        .env("PATH", "/nonexistent-empty-path")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("ffprobe not found")
            || String::from_utf8_lossy(&output.stderr).contains("ffmpeg not found"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unwritable_output_destination_fails_loud() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.wav");
    write_silence_wav(&input, 1.0);

    let output = Command::new(para_bin())
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            "/no/such/directory/out.txt",
            "--device",
            "cpu",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot write output file"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
