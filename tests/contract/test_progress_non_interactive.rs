//! T018: with stderr not connected to an interactive terminal (the default
//! for `std::process::Command`, matching a redirected-to-file or piped
//! real-world invocation), progress is reported as plain newline-terminated
//! lines with no ANSI/cursor-control bytes, and stdout is still exactly the
//! transcript.
//!
//! The `TERM=dumb`-with-a-real-pty-attached sub-case (T019 — research.md
//! §2's finding that an unset/`dumb` `TERM` is treated as non-interactive
//! even when stderr *is* a real terminal device) was verified manually
//! during implementation via `TERM=dumb script -q out.txt para ...`: the
//! captured output showed the same plain-line behavior asserted here, with
//! no ANSI escape bytes. Not automated here to avoid adding a pty-emulation
//! dependency for one sub-case of the same `is_interactive()` check this
//! test already exercises the other half of.

use crate::support::{para_bin, write_silence_wav};
use std::process::Command;

#[test]
fn non_interactive_stderr_has_no_escape_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("silence.wav");
    write_silence_wav(&input, 1.0);

    let output = Command::new(para_bin())
        .args(["-i", input.to_str().unwrap(), "--device", "cpu"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    const ESC: u8 = 0x1b;
    assert!(
        !output.stderr.contains(&ESC),
        "stderr contained an ANSI escape byte: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("loading model") && stderr.contains("transcribing"),
        "expected plain progress milestones, got: {stderr}"
    );

    assert!(!output.stdout.contains(&ESC));
}
