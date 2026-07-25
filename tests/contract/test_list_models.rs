//! T027: `--list-models` lists every registered model with cache state and
//! marks exactly one default.

use crate::support::para_bin;
use std::process::Command;

#[test]
fn lists_every_model_with_exactly_one_default() {
    let output = Command::new(para_bin())
        .arg("--list-models")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("parakeet-tdt-0.6b-v3"), "stdout: {stdout}");
    assert!(stdout.contains("parakeet-tdt-0.6b-v2"), "stdout: {stdout}");
    assert_eq!(stdout.matches("(default)").count(), 1, "stdout: {stdout}");
    assert_eq!(
        stdout.matches("Cache state:").count(),
        2,
        "stdout: {stdout}"
    );
}
