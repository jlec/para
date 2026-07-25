//! T026: an unrecognized `--model` value exits non-zero, lists valid ids,
//! and attempts no transcription (no download, no ffmpeg invocation).

use crate::support::para_bin;
use std::process::Command;

#[test]
fn unknown_model_lists_valid_ids_and_does_not_transcribe() {
    let output = Command::new(para_bin())
        .args([
            "-i",
            "/no/such/file.wav",
            "--model",
            "totally-bogus-model",
            "--device",
            "cpu",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown model"), "stderr: {stderr}");
    assert!(stderr.contains("parakeet-tdt-0.6b-v3"), "stderr: {stderr}");
    assert!(stderr.contains("parakeet-tdt-0.6b-v2"), "stderr: {stderr}");
    // Confirms model resolution failed before even touching the (nonexistent)
    // input file — otherwise the error would be about the missing file, not
    // the unknown model.
    assert!(!stderr.contains("No such file"), "stderr: {stderr}");
}
