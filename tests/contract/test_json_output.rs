//! T035: `--format json` output validates against
//! contracts/output-json-schema.json, and every segment has `end > start`.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn json_output_has_required_fields_and_valid_segments() {
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
            "-f",
            "json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(parsed.get("text").is_some());
    assert!(parsed.get("model").is_some());
    assert!(parsed.get("duration_seconds").is_some());
    let segments = parsed["segments"].as_array().unwrap();
    for segment in segments {
        assert!(segment["end"].as_f64().unwrap() > segment["start"].as_f64().unwrap());
    }
}
