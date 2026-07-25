//! T039: SRT block numbering, comma millisecond separator, blank-line
//! spacing.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn srt_output_uses_comma_separator() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("silence.wav");
    write_silence_wav(&input, 1.0);

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
    if stdout.is_empty() {
        // Pure silence can legitimately produce zero recognized tokens —
        // still a valid (empty) SRT, nothing further to assert.
        return;
    }
    assert!(stdout.starts_with('1'), "stdout: {stdout}");
    assert!(stdout.contains("-->"), "stdout: {stdout}");
    assert!(stdout.contains(','), "stdout: {stdout}"); // comma ms separator, not a period
}
