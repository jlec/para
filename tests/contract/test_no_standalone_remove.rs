//! T029: the CLI's flag surface has no standalone command whose only effect
//! is removing a cached model without also re-fetching it (guards FR-021).
//! `--refresh-model` deletes-then-redownloads; nothing deletes-only.

use crate::support::para_bin;
use std::process::Command;

#[test]
fn help_has_no_remove_or_uninstall_only_flag() {
    let output = Command::new(para_bin()).arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout).to_lowercase();

    assert!(help.contains("refresh-model"), "help: {help}");
    for forbidden in ["--remove", "--delete", "--uninstall", "--clear-cache"] {
        assert!(
            !help.contains(forbidden),
            "help unexpectedly has {forbidden}: {help}"
        );
    }
}
