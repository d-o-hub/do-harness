//! Acceptance tests for `do-harness metrics pr` (#245).
//!
//! The fixtures carry measured values from `d-o-hub/do-harness` pull requests
//! #268 and #269 plus a documented control (#270), served by a fake `gh`, so the
//! whole path — fetch, measure, cache, render — runs without network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]
// The fixture serves `gh` through a POSIX shell script on `PATH`; on Windows the
// name does not resolve to it (no extension) and the runner's real `gh` would
// answer without a token. The command itself is platform-neutral — only these
// fixtures are not, matching the other fake-binary suites.
#![cfg(unix)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::pr_metrics::{copy_fixtures, fake_gh_router, run_metrics};

/// A fixture root with a git directory (the snapshot cache needs one).
fn fixture_root() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    copy_fixtures(dir.path());
    // `git init` must not inherit a hook's `GIT_DIR`/`GIT_WORK_TREE`, or it
    // re-initializes the repository running the tests instead of the fixture.
    let status = support::git_command(dir.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    assert!(status.success());
    let bin = fake_gh_router(dir.path());
    (dir, bin)
}

/// JSON report of a fixture run.
fn report(root: &Path, bin: &Path, extra: &[&str]) -> Value {
    let mut args = vec!["--repo", "d-o-hub/do-harness", "--since", "30d"];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["--format", "json"]);
    let (code, stdout, stderr) = run_metrics(root, bin, &args);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    serde_json::from_str(&stdout).expect("report JSON")
}

/// Field of one measured pull request.
fn row(report: &Value, number: u64) -> &Value {
    report["prs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["number"] == number)
        .unwrap_or_else(|| panic!("pr {number} missing"))
}

#[test]
fn the_text_report_shows_the_measured_rows_and_totals() {
    let (dir, bin) = fixture_root();
    let (code, stdout, stderr) = run_metrics(
        dir.path(),
        &bin,
        &["--repo", "d-o-hub/do-harness", "--since", "30d"],
    );
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert!(
        stdout.starts_with("metrics pr: d-o-hub/do-harness since 30d (3 PR(s), 2 green)"),
        "{stdout}"
    );
    // The measured rows: #269 went green 26m45s after its first push, #268 after
    // 1h39m20s, and the control never did.
    assert!(stdout.contains("#269"), "{stdout}");
    assert!(stdout.contains("26m45s"), "{stdout}");
    assert!(stdout.contains("1h39m"), "{stdout}");
    assert!(stdout.contains("guarded-arm"), "{stdout}");
    assert!(
        stdout.contains("time-to-green: p50 26m45s, p95 1h39m"),
        "{stdout}"
    );
    assert!(stdout.contains("pushes/green: 4.0"), "{stdout}");
    assert!(
        stdout.contains("comments: actionable 2, informational 4"),
        "{stdout}"
    );
    assert!(
        stdout.contains("runs: 6, cancelled 1 (16.7%)    reruns: 1"),
        "{stdout}"
    );
}

#[test]
fn the_json_report_carries_the_raw_counts() {
    let (dir, bin) = fixture_root();
    let report = report(dir.path(), &bin, &[]);
    assert_eq!(report["repo"], "d-o-hub/do-harness");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["cached"], false);
    assert_eq!(report["totals"]["prs"], 3);
    assert_eq!(report["totals"]["green_prs"], 2);
    assert_eq!(report["totals"]["pushes"], 8);
    assert_eq!(report["totals"]["cancelled_runs"], 1);
    assert_eq!(report["totals"]["reruns"], 1);
    assert_eq!(report["totals"]["waivers"]["macro-field"], 2);
    assert_eq!(report["totals"]["waivers"]["guarded-arm"], 2);
    assert_eq!(report["totals"]["waivers"]["feature-gated"], 1);

    let merged = row(&report, 269);
    assert_eq!(merged["state"], "closed");
    assert_eq!(merged["pushes"], 3);
    assert_eq!(merged["time_to_green_secs"], 1_605);
    assert_eq!(merged["cancelled_runs"], 0);

    let control = row(&report, 270);
    assert_eq!(control["green_at"], Value::Null);
    assert_eq!(control["time_to_green_secs"], Value::Null);
    assert_eq!(control["cancelled_runs"], 1);
    assert_eq!(control["reruns"], 1);
    assert_eq!(control["actionable_comments"], 1);
    assert_eq!(control["informational_comments"], 3);
}

#[test]
fn a_second_run_serves_the_cache_and_recompute_refetches() {
    let (dir, bin) = fixture_root();
    let first = report(dir.path(), &bin, &[]);
    assert_eq!(first["cached"], false);
    let second = report(dir.path(), &bin, &[]);
    assert_eq!(second["cached"], true);
    assert_eq!(second["totals"], first["totals"]);
    let recomputed = report(dir.path(), &bin, &["--recompute"]);
    assert_eq!(recomputed["cached"], false);
}

#[test]
fn the_limit_bounds_the_measured_window() {
    let (dir, bin) = fixture_root();
    let report = report(dir.path(), &bin, &["--limit", "2"]);
    assert_eq!(report["totals"]["prs"], 2);
    let numbers: Vec<u64> = report["prs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["number"].as_u64().unwrap())
        .collect();
    // The list endpoint is `updated desc`, so the newest two are kept.
    assert_eq!(numbers, vec![270, 269]);
}

#[test]
fn an_invalid_window_is_a_usage_error() {
    let (dir, bin) = fixture_root();
    let (code, _, stderr) = run_metrics(
        dir.path(),
        &bin,
        &["--repo", "d-o-hub/do-harness", "--since", "last month"],
    );
    assert_eq!(code, Some(2));
    assert!(stderr.contains("--since must be a duration"), "{stderr}");
}

#[test]
fn a_failing_gh_read_fails_the_run_instead_of_reporting_zero() {
    let (dir, bin) = fixture_root();
    // Remove the pull-request list: `gh api` cannot answer, so the measure must
    // fail loudly rather than report an empty, all-zero window.
    std::fs::remove_file(dir.path().join("pulls.json")).unwrap();
    let (code, stdout, stderr) = run_metrics(
        dir.path(),
        &bin,
        &["--repo", "d-o-hub/do-harness", "--since", "30d"],
    );
    assert_eq!(code, Some(1), "stdout: {stdout}");
    assert!(stderr.contains("cannot list pull requests"), "{stderr}");
}

#[test]
fn sensor_report_filters_are_rejected_next_to_the_pr_subcommand() {
    let (dir, bin) = fixture_root();
    let path = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(dir.path())
        .args([
            "metrics",
            "--sensor",
            "fmt",
            "pr",
            "--repo",
            "d-o-hub/do-harness",
        ])
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("run do-harness");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("do not apply to `metrics pr`"), "{stderr}");
}
