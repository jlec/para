//! T007: while progress is being reported (model loading, transcribing),
//! stdout stays byte-for-byte the transcript — nothing progress-related
//! ever lands there.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn stdout_is_untouched_by_progress_reporting() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("silence.wav");
    write_silence_wav(&input, 1.0);

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
        stderr.contains("loading model") && stderr.contains("transcribing"),
        "expected progress output on stderr, got: {stderr}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for marker in ["loading model", "transcribing", "reading stdin", "%", "eta"] {
        assert!(
            !stdout.contains(marker),
            "stdout leaked progress-related content ({marker:?}): {stdout:?}"
        );
    }
}
