//! The committed `man/do-harness.1` must match what the CLI generates.
//!
//! Generated artifacts rot silently. This page still announced
//! `0.2.0 (01f6e3e-dirty 2026-10-03)` while the crate was `0.3.0`, and nothing
//! noticed — unlike `completions/`, which `completions_sync.rs` guards.
//!
//! The build fingerprint (short revision, `-dirty`, date) changes on every
//! commit, so it is normalised away; the **semver is not**, so a page that still
//! announces an old release fails here. Regenerating is cheap: the binary is
//! already built for the test run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

/// Replaces a build fingerprint `(<rev>[-dirty] <date>)` with `(<build>)`.
///
/// Both spellings appear in the page: the `.TH` header writes the fingerprint
/// bare, and the `VERSION` section escapes its hyphens.
fn normalize(text: &str) -> String {
    text.lines()
        .map(|line| match (line.find(" ("), line.rfind(')')) {
            (Some(open), Some(close)) if close > open + 2 => {
                let inside = &line[open + 2..close];
                let dated = inside.split_whitespace().any(|token| {
                    let token = token.replace('\\', "");
                    token.len() == 10
                        && token.as_bytes()[4] == b'-'
                        && token.as_bytes()[7] == b'-'
                        && token.chars().filter(char::is_ascii_digit).count() == 8
                });
                if dated {
                    format!("{}(<build>){}", &line[..open], &line[close + 1..])
                } else {
                    line.to_owned()
                }
            }
            _ => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn committed_man_page_matches_the_cli() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("man")
        .arg(out.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`do-harness man <dir>` must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = std::fs::read_to_string(out.path().join("do-harness.1")).unwrap();
    let committed = std::fs::read_to_string(root.join("man/do-harness.1")).unwrap();
    assert_eq!(
        normalize(&generated),
        normalize(&committed),
        "man/do-harness.1 is stale; regenerate with `do-harness man man`"
    );

    // The index must name invocations: one page is shipped, so a `(1)` ref
    // would point at a page that is never generated.
    let dangling: Vec<&str> = committed
        .lines()
        .filter(|line| line.starts_with(r"do\-harness\-") && line.ends_with("(1)"))
        .collect();
    assert!(dangling.is_empty(), "dangling man references: {dangling:?}");

    // The normalisation must not have swallowed the semver: both sides still
    // carry the crate's version.
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        committed.contains(version),
        "the committed page must announce {version}: {}",
        committed.lines().take(4).collect::<Vec<_>>().join(" | ")
    );
}
