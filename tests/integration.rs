//! End-to-end tests against a real cached model (feature = `integration`;
//! `cargo test --features integration`). Uses macOS `say` to generate real
//! speech at test time — consistent with this project's macOS/Apple Silicon
//! first-class design (Constitution Principle VI) — rather than asserting
//! only on silence, so these actually exercise decode correctness.

#![cfg(feature = "integration")]

use std::path::Path;
use std::process::Command;

fn para_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_para"))
}

fn say_fixture(dir: &Path, text: &str) -> std::path::PathBuf {
    let path = dir.join("speech.aiff");
    let status = Command::new("say")
        .args(["-o", path.to_str().unwrap(), text])
        .status()
        .expect("macOS `say` must be available to generate integration-test fixtures");
    assert!(status.success());
    path
}

#[test]
fn end_to_end_text_transcription_produces_real_text() {
    let dir = tempfile::tempdir().unwrap();
    let input = say_fixture(dir.path(), "Testing one two three.");

    let output = Command::new(para_bin())
        .args(["-i", input.to_str().unwrap(), "--device", "cpu"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    assert!(text.contains("test"), "transcript: {text}");
    // The model applies inverse text normalization, so "one two three" may
    // legitimately come back as "123" — accept either form.
    assert!(
        text.contains("one") || text.contains("123"),
        "transcript: {text}"
    );
}

#[test]
fn end_to_end_json_transcription_has_real_segments() {
    let dir = tempfile::tempdir().unwrap();
    let input = say_fixture(dir.path(), "Testing json output.");

    let output = Command::new(para_bin())
        .args([
            "-i",
            input.to_str().unwrap(),
            "--device",
            "cpu",
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
    let text = parsed["text"].as_str().unwrap().to_lowercase();
    assert!(
        text.contains("json") || text.contains("testing"),
        "transcript: {text}"
    );
    assert!(!parsed["segments"].as_array().unwrap().is_empty());
}

#[test]
fn end_to_end_srt_transcription_has_valid_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let input = say_fixture(dir.path(), "Testing subtitle output.");

    let output = Command::new(para_bin())
        .args([
            "-i",
            input.to_str().unwrap(),
            "--device",
            "cpu",
            "-f",
            "srt",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("-->"), "stdout: {stdout}");
    assert!(stdout.contains(','), "stdout: {stdout}");
}
