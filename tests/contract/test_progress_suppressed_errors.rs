//! T023: with `--no-progress` set, an error case still produces its usual
//! specific stderr message and non-zero exit — suppression affects only
//! progress reporting, never errors (FR-009). No cached model needed since
//! this fails before model resolution.

use crate::support::para_bin;
use std::process::Command;

#[test]
fn no_progress_does_not_suppress_error_messages() {
    let output = Command::new(para_bin())
        .args([
            "-i",
            "/no/such/file.wav",
            "--device",
            "cpu",
            "--no-progress",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("No such file"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
