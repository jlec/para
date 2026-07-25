//! T021: `--no-progress` produces zero progress-related stderr output.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn no_progress_flag_suppresses_all_progress_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("silence.wav");
    write_silence_wav(&input, 1.0);

    let output = Command::new(para_bin())
        .args([
            "-i",
            input.to_str().unwrap(),
            "--device",
            "cpu",
            "--no-progress",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for marker in ["loading model", "transcribing", "reading stdin"] {
        assert!(
            !stderr.contains(marker),
            "expected no progress output with --no-progress, found {marker:?} in: {stderr}"
        );
    }
}
