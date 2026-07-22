//! T013: running para against a short clip shows a progress-related stderr
//! line before the transcript appears (spec.md SC-005 / User Story 2) —
//! every run gets a model-loading indicator, not just long ones.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn short_clip_still_shows_model_loading_feedback() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("short.wav");
    write_silence_wav(&input, 0.5);

    let output = Command::new(para_bin())
        .args([
            "-i",
            input.to_str().unwrap(),
            "--device",
            "cpu",
            "--model",
            "parakeet-ctc-0.6b",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("loading model"),
        "expected a model-loading indicator even for a short clip, got: {stderr}"
    );
}
