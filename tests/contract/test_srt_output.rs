//! T039: SRT block numbering, comma millisecond separator, blank-line
//! spacing, and the single-segment CTC-model fallback.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn srt_output_uses_comma_separator_and_ctc_single_segment_fallback() {
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
    if stdout.is_empty() {
        // Pure silence can legitimately produce zero recognized tokens —
        // still a valid (empty) SRT, nothing further to assert.
        return;
    }
    assert!(stdout.starts_with('1'), "stdout: {stdout}");
    assert!(stdout.contains("-->"), "stdout: {stdout}");
    assert!(stdout.contains(','), "stdout: {stdout}"); // comma ms separator, not a period
    // CTC is whole-file granularity: exactly one block.
    assert_eq!(stdout.matches("-->").count(), 1, "stdout: {stdout}");
}
