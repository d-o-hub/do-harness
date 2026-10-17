//! The libSQL upstream-issue watch must parse issue state, not assume it.
//!
//! `scripts/check-libsql-watch.sh` tracks the tursodatabase/libsql issues that
//! affect the vendored 0.9.30 line (`plans/libsql-distribution.md`): #2254,
//! #2271, #2272, and #2285. The harness uses libSQL as its persistence engine,
//! so the rebase decision depends on which of those are still open. A watch that
//! cannot read the issues must say so rather than report a clean line, and a
//! network failure must never turn into a failed run.
//!
//! These fixtures are hermetic — no network, no clock — so the verdict logic is
//! pinned rather than observed from a live tracker that is expected to change
//! under us.
//!
//! Gating: the source-level check runs everywhere. The cases that execute the
//! script are `#[cfg(unix)]`, because the watch is a `bash` script that requires
//! `jq` and reads fixture paths through Git Bash on Windows, where path
//! translation is not the behaviour under test. `scripts/check-shell.sh` lints
//! the script on every platform.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::process::Output;

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-libsql-watch.sh")
}

#[cfg(unix)]
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/libsql-watch")
        .join(name)
}

/// Runs the watch in fixture mode.
#[cfg(unix)]
fn run(fixtures: &str) -> Output {
    Command::new("/bin/bash")
        .arg(script())
        .arg("--fixtures")
        .arg(fixture(fixtures))
        .output()
        .expect("spawn check-libsql-watch.sh")
}

#[cfg(unix)]
fn verdict(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("VERDICT="))
        .unwrap_or_default()
        .to_owned()
}

/// The watch must query the repository that actually hosts the vendored fork,
/// and track every issue the plan names.
///
/// A watch pointed anywhere else reports state for a tracker that has nothing
/// to do with the vendored line, which is indistinguishable from a genuine
/// clean result. Pin the repository and the issue set at the source level, so
/// a future edit that drops one fails here instead of narrowing coverage
/// silently.
#[test]
fn watch_tracks_the_official_repo_and_every_named_issue() {
    // `--help` prints the header comment; the URLs live in the body, so read
    // the script itself.
    let source = std::fs::read_to_string(script()).expect("read check-libsql-watch.sh");

    assert!(
        source.contains("api.github.com/repos/$REPO"),
        "the issues URL must be built from the REPO variable"
    );
    assert!(
        source.contains(r#"REPO="tursodatabase/libsql""#),
        "the watched repository must be tursodatabase/libsql"
    );
    let tracked = source
        .lines()
        .find(|line| line.starts_with("TRACKED_ISSUES="))
        .expect("the script must declare TRACKED_ISSUES");
    for issue in ["2254", "2271", "2272", "2285"] {
        assert!(
            tracked.contains(issue),
            "the tracked issue set must include #{issue}: {tracked}"
        );
    }
}

/// The watch prints which sources it queried, so blind watching is visible.
#[cfg(unix)]
#[test]
fn watch_reports_its_sources() {
    let out = run("issues-open.json");
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("source: repo=https://api.github.com/repos/tursodatabase/libsql"));
    assert!(stdout.contains("source: issues=2254 2271 2272 2285"));
}

/// An open issue keeps the rebase decision due, and the summary names it.
#[cfg(unix)]
#[test]
fn open_issues_keep_the_rebase_decision_due() {
    let out = run("issues-open.json");
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert_eq!(verdict(&out), "OPEN");
    for issue in ["2254", "2271", "2272", "2285"] {
        assert!(
            stdout.contains(&format!("issue: #{issue} state=open")),
            "every open issue must be reported: {stdout}"
        );
    }
    assert!(
        stdout.contains(
            "summary: still-open issues affecting the 0.9.30 base: #2254 #2271 #2272 #2285"
        )
    );
}

/// A partial fix reports exactly the issues that are still open.
///
/// This is the case an operator acts on: #2254 and #2272 closed upstream must
/// not appear in the "still-open" summary, and the verdict must stay `OPEN`
/// while any tracked issue remains.
#[cfg(unix)]
#[test]
fn the_summary_names_only_the_still_open_issues() {
    let out = run("issues-mixed.json");
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert_eq!(verdict(&out), "OPEN");
    assert!(stdout.contains("summary: still-open issues affecting the 0.9.30 base: #2271 #2285"));
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("summary:"))
        .expect("summary line");
    assert!(
        !summary.contains("#2254") && !summary.contains("#2272"),
        "closed issues must not be summarised as open: {summary}"
    );
}

/// Every tracked issue closed clears the line.
#[cfg(unix)]
#[test]
fn all_issues_closed_reports_clear() {
    let out = run("issues-closed.json");
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert_eq!(verdict(&out), "CLEAR");
    assert!(stdout.contains("summary: no tracked issue is open"));
}

/// A parse failure is `UNKNOWN` and still exit 0.
///
/// The watch is informational: an operator reads the verdict, so an unreadable
/// fixture must not be reported as `CLEAR` (a false clean line) and must not
/// fail the surrounding run either.
#[cfg(unix)]
#[test]
fn a_parse_failure_is_unknown_and_does_not_fail_the_run() {
    let temp = tempfile::tempdir().unwrap();
    let bad = temp.path().join("bad.json");
    std::fs::write(&bad, "not json at all").unwrap();

    let out = Command::new("/bin/bash")
        .arg(script())
        .arg("--fixtures")
        .arg(&bad)
        .output()
        .expect("spawn check-libsql-watch.sh");

    assert!(out.status.success(), "a parse failure must exit 0");
    assert_eq!(verdict(&out), "UNKNOWN");
}

/// A missing fixture file is a usage error, not a verdict.
#[cfg(unix)]
#[test]
fn a_missing_fixture_is_a_usage_error() {
    let out = Command::new("/bin/bash")
        .arg(script())
        .arg("--fixtures")
        .arg(fixture("does-not-exist.json"))
        .output()
        .expect("spawn check-libsql-watch.sh");

    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("fixture file not found"));
}

/// `--help` prints the header comment and never a line of code.
#[cfg(unix)]
#[test]
fn help_prints_the_header_and_never_code() {
    let out = Command::new("/bin/bash")
        .arg(script())
        .arg("--help")
        .output()
        .expect("spawn check-libsql-watch.sh --help");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Documents what is watched and the fixture interface, so an operator
    // learns how to run it hermetically.
    assert!(stdout.contains("libSQL"), "{stdout}");
    assert!(stdout.contains("--fixtures"), "{stdout}");
    // Every printed line is a comment; no code may leak.
    for line in stdout.lines() {
        assert!(
            line.is_empty() || line.starts_with('#'),
            "help leaked a non-comment line: {line:?}"
        );
    }
}
