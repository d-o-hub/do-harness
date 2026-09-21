//! The committed `completions/` tree must match what the CLI generates.
//!
//! Generated artifacts rot silently: before this guard existed, every committed
//! file had drifted from the CLI's actual surface, so the tree documented
//! subcommands that no longer existed and missed the ones that did. Regenerating
//! here is cheap because the binary is already built for the test run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// `(shell, committed artifact)` pairs, one per generated file.
const SHELLS: [(&str, &str); 5] = [
    ("bash", "completions/do-harness.bash"),
    ("zsh", "completions/_do-harness"),
    ("fish", "completions/do-harness.fish"),
    ("powershell", "completions/_do-harness.ps1"),
    ("elvish", "completions/do-harness.elv"),
];

#[test]
fn committed_completions_match_the_cli() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (shell, relative) in SHELLS {
        let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
            .args(["completions", shell])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`do-harness completions {shell}` must succeed"
        );
        let committed = fs::read_to_string(root.join(relative)).unwrap();
        let generated = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            generated, committed,
            "{relative} is stale; regenerate with `do-harness completions {shell} > {relative}`"
        );
    }
}
