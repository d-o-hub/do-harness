//! `dora --source gh`: the opt-in `GitHub` enrichment path, driven end to end
//! against a fake `gh` on `PATH`.
//!
//! The inline unit tests in `src/dora/gh.rs` cover timestamp parsing only. The
//! enrichment itself — release-run reconciliation (including the rule that a
//! success outside the window still counts as shipped), the per-head
//! conclusion fold, and PR-open → merge lead time — had no coverage at all,
//! and it is the part of the metric that talks to the network in production.
//!
//! The fake `gh` answers exactly the two queries the module issues
//! (`run list --workflow release.yml …`, `pr list --state merged …`) and fails
//! loudly on anything else, so a changed flag shape cannot quietly pass.

#![allow(clippy::unwrap_used, clippy::expect_used)]
// The fake `gh` is a bash script, so these fixtures are Unix-only by
// construction (the Windows job runs the suite under Git Bash, but Rust spawns
// `gh` directly and cannot exec a shebang there).
#![cfg(unix)]

use std::path::{Path, PathBuf};

use serde_json::Value;

mod support;

use support::dora::{T0, deployed_fixture, harness, run};

/// A fake `gh` that serves `runs`/`prs` JSON.
fn fake_gh(dir: &Path, runs: &str, prs: &str) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let runs_path = dir.join("runs.json");
    let prs_path = dir.join("prs.json");
    std::fs::write(&runs_path, runs).unwrap();
    std::fs::write(&prs_path, prs).unwrap();
    let script = format!(
        "#!/usr/bin/env bash\n\
         set -euo pipefail\n\
         case \"${{1:-}} ${{2:-}}\" in\n\
         \"run list\") cat \"{}\" ;;\n\
         \"pr list\") cat \"{}\" ;;\n\
         *) echo \"fake gh: unexpected call: $*\" >&2; exit 2 ;;\n\
         esac\n",
        runs_path.display(),
        prs_path.display()
    );
    let gh = bin.join("gh");
    std::fs::write(&gh, script).unwrap();
    let mut perms = std::fs::metadata(&gh).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&gh, perms).unwrap();
    bin
}

/// Runs `dora --source gh` with `bin` first on `PATH`, returning the exit code
/// and the parsed snapshot.
///
/// The fixture's own thresholds may breach (exit 1) depending on the policy,
/// and a breach still emits the snapshot these assertions read; a usage or
/// discovery failure (exit 2) or a crash is what must not happen here.
fn dora_gh_json(root: &Path, bin: &Path, now: i64) -> (Option<i32>, Value) {
    let joined = std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .expect("join PATH");
    let (code, stdout, stderr) = run(harness(root)
        .arg("dora")
        .arg("--source")
        .arg("gh")
        .arg("--format")
        .arg("json")
        .arg("--days")
        .arg("30")
        .arg("--now")
        .arg(now.to_string())
        .env("PATH", joined));
    assert!(
        matches!(code, Some(0 | 1)),
        "dora must emit a snapshot (0 within thresholds, 1 breached), not {code:?}:\n{stdout}\n{stderr}"
    );
    let value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("dora stdout is not JSON:\n{stdout}\n{stderr}"));
    (code, value)
}

/// `rev-parse` in `root`, trimmed.
fn rev(root: &Path, spec: &str) -> String {
    let out = support::git_command(root)
        .args(["rev-parse", spec])
        .output()
        .expect("spawn git rev-parse");
    assert!(out.status.success(), "rev-parse {spec} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// The fixture's deploy heads. `deployed_fixture` tags `v0.1.0` and `v0.1.1`,
/// and those are the deploy tags the enrichment reconciles against runs.
fn fixture_heads(root: &Path) -> (String, String) {
    (rev(root, "v0.1.0^{commit}"), rev(root, "v0.1.1^{commit}"))
}

#[test]
fn gh_enrichment_folds_runs_and_measures_pr_lead_time() {
    let (dir, root) = deployed_fixture();
    let (head0, head1) = fixture_heads(&root);

    // `gh run list` is newest-first, and the same head appears twice with the
    // *older* row succeeding: the fold must still report `success`.
    let runs = format!(
        r#"[
  {{"conclusion":"failure","createdAt":"2023-11-15T22:13:20Z","headSha":"{head0}"}},
  {{"conclusion":"success","createdAt":"2023-11-15T22:13:20Z","headSha":"{head0}"}},
  {{"conclusion":"success","createdAt":"2023-11-15T22:13:20Z","headSha":"{head1}"}}
]"#
    );
    let prs = r#"[
  {"number":1,"createdAt":"2023-11-15T22:13:20Z","mergedAt":"2023-11-15T22:14:20Z"},
  {"number":2,"createdAt":"2023-11-15T22:13:20Z","mergedAt":"2023-11-15T22:23:20Z"}
]"#;
    let bin = fake_gh(dir.path(), &runs, prs);
    // Five days after the fixture's base epoch, so the 30-day window covers the
    // fixture history and every row above.
    let (_code, snapshot) = dora_gh_json(&root, &bin, T0 + 5 * 86_400);

    assert_eq!(snapshot["source"], serde_json::json!("gh"));
    let github = &snapshot["github"];
    assert_eq!(github["workflow"], serde_json::json!("release.yml"));
    assert_eq!(github["runs_total"], serde_json::json!(3));
    assert_eq!(github["runs_failed"], serde_json::json!(1));
    assert_eq!(
        github["runs_by_head"][&head0],
        serde_json::json!("success"),
        "a success anywhere in a head's runs wins regardless of run order"
    );
    assert_eq!(github["runs_by_head"][&head1], serde_json::json!("success"));
    assert!(
        github["deploy_head_mismatches"]
            .as_array()
            .expect("mismatches")
            .is_empty(),
        "every deploy tag's head released: {github}"
    );

    // Nearest-rank percentiles over [60, 600]: p50 is the first sample, not an
    // interpolated median, and p90 is the last.
    assert_eq!(github["pr_samples"], serde_json::json!(2));
    assert_eq!(github["pr_lead_p50_seconds"], serde_json::json!(60));
    assert_eq!(github["pr_lead_p90_seconds"], serde_json::json!(600));
}

#[test]
fn gh_reconciliation_counts_a_success_outside_the_window_and_names_a_real_mismatch() {
    let (dir, root) = deployed_fixture();
    let (head0, head1) = fixture_heads(&root);

    // `v0.1.0`'s head succeeded before the window (a deploy just before the
    // measurement started still shipped, so it is not a mismatch), while
    // `v0.1.1`'s head only ever failed inside it.
    let runs = format!(
        r#"[
  {{"conclusion":"failure","createdAt":"2023-11-18T22:13:20Z","headSha":"{head1}"}},
  {{"conclusion":"success","createdAt":"2023-10-15T22:13:20Z","headSha":"{head0}"}}
]"#
    );
    let bin = fake_gh(dir.path(), &runs, "[]");
    let (_code, snapshot) = dora_gh_json(&root, &bin, T0 + 5 * 86_400);

    let github = &snapshot["github"];
    let mismatches: Vec<&str> = github["deploy_head_mismatches"]
        .as_array()
        .expect("mismatches")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        mismatches,
        vec!["v0.1.1"],
        "the tag whose head never succeeded did not ship: {github}"
    );
    // The window bounds the counters but not the reconciliation: the success
    // that shipped `v0.1.0` is older than `window_start` and still counts.
    assert_eq!(github["runs_total"], serde_json::json!(1));
    assert_eq!(github["runs_failed"], serde_json::json!(1));
    assert_eq!(
        github["pr_samples"],
        serde_json::json!(0),
        "no merged PRs in the window is not an error"
    );
    assert_eq!(
        github["pr_lead_p50_seconds"],
        Value::Null,
        "no samples reports null rather than a zero-second lead time"
    );
}

#[test]
fn gh_failure_is_an_error_not_an_empty_snapshot() {
    let (dir, root) = deployed_fixture();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let gh = bin.join("gh");
    std::fs::write(
        &gh,
        "#!/usr/bin/env bash\necho 'gh: not logged in' >&2\nexit 1\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&gh).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&gh, perms).unwrap();

    let joined = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("join PATH");
    let (code, stdout, stderr) = run(harness(&root)
        .arg("dora")
        .arg("--source")
        .arg("gh")
        .arg("--format")
        .arg("json")
        .env("PATH", joined));
    assert_ne!(
        code,
        Some(0),
        "a failing gh must not produce a green snapshot:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("gh") || stdout.contains("gh"),
        "the failure must name gh:\nstdout: {stdout}\nstderr: {stderr}"
    );
}
