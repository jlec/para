//! T028: selecting a specific model actually uses that model (echoed via the
//! stderr status line), never silently substituted.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn selected_model_is_echoed_and_used() {
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
            "parakeet-tdt-0.6b-v2",
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
        stderr.contains("using model: parakeet-tdt-0.6b-v2"),
        "stderr: {stderr}"
    );
}
