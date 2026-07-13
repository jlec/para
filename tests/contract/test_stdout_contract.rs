//! T019: `--format text` (default) writes only the transcript to stdout,
//! nothing else. Gated behind `integration` — needs a real cached model.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn stdout_is_only_the_transcript() {
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
    // Status/progress lines ("using model: ...") must go to stderr only.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("using model:"), "stderr: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("using model:"), "stdout: {stdout}");
}
