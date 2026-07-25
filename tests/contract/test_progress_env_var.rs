//! T022: `PARA_NO_PROGRESS` env var has the same suppression effect as
//! `--no-progress`. Any non-empty value works (contracts/cli-interface.md)
//! — handled manually in `run()`, not via clap's `env` attribute, since
//! clap's bool+env parsing only accepts literal "true"/"false" and errors
//! on other values like "1" (found during implementation).

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn env_var_with_value_one_suppresses_progress() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("silence.wav");
    write_silence_wav(&input, 1.0);

    let output = Command::new(para_bin())
        .args(["-i", input.to_str().unwrap(), "--device", "cpu"])
        .env("PARA_NO_PROGRESS", "1")
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
            "expected no progress output with PARA_NO_PROGRESS=1, found {marker:?} in: {stderr}"
        );
    }
}
