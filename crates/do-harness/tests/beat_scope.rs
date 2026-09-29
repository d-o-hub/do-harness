//! Acceptance fixtures for branch-scoped beats (#239): `verify --record` on a
//! branch scopes beats to that branch without flags, and `metrics` filters by
//! current branch by default, not summing across workstreams unless asked.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::{TempDir, tempdir};

mod support;

use support::git_command;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs a git command inside `root`, asserting success.
fn git(root: &Path, args: &[&str]) {
    let status = git_command(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .expect("spawn git");
    assert!(
        status.success(),
        "git {args:?} failed in {}",
        root.display()
    );
}

fn fixture_repo() -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    git(&root, &["init", "-q", "-b", "main"]);
    let config = r#"
language = "generic"

[signal-sets]
all = ["s1"]

[[sensors]]
name = "s1"
argv = ["true"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "base"]);
    (dir, root)
}

#[test]
fn verify_record_on_branch_scopes_and_metrics_isolates() {
    let (_dir, root) = fixture_repo();

    // 1. Run verify --record on main without flags:
    let (code, stdout, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--record"));
    assert_eq!(code, Some(0), "verify failed:\n{stdout}\n{stderr}");
    // Should NOT warn about global namespace since it is scoped to branch:main:
    assert!(
        !stderr.contains("records beats in the global namespace"),
        "must not warn on branch:\n{stderr}"
    );

    // Default metrics on main:
    let (code, stdout, _) = run(harness(&root).arg("metrics").arg("--format").arg("json"));
    assert_eq!(code, Some(0));
    let metrics: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(metrics["scope"], "branch:main");
    assert_eq!(metrics["sensors"].as_array().unwrap().len(), 1);
    assert_eq!(metrics["sensors"][0]["name"], "s1");
    assert_eq!(metrics["sensors"][0]["runs"], 1);

    // Filtered by global should show 0 runs:
    let (code, stdout, _) = run(harness(&root)
        .arg("metrics")
        .arg("--scope")
        .arg("global")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0));
    let metrics_global: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(metrics_global["scope"], "global");
    assert_eq!(metrics_global["sensors"].as_array().unwrap().len(), 0);

    // 2. Switch to a feature branch:
    git(&root, &["switch", "-q", "-c", "feat/my-workstream"]);
    // Metrics on feature branch (no runs yet):
    let (code, stdout, _) = run(harness(&root).arg("metrics").arg("--format").arg("json"));
    assert_eq!(code, Some(0));
    let metrics_feat: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(metrics_feat["scope"], "branch:feat/my-workstream");
    assert_eq!(metrics_feat["sensors"].as_array().unwrap().len(), 0);

    // Record two runs on the feature branch:
    run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--record"));
    run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--record"));

    // Default metrics on feature branch now reports 2 runs:
    let (code, stdout, _) = run(harness(&root).arg("metrics").arg("--format").arg("json"));
    assert_eq!(code, Some(0));
    let metrics_feat2: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(metrics_feat2["sensors"][0]["runs"], 2);

    // Filter by branch:main still shows 1 run:
    let (code, stdout, _) = run(harness(&root)
        .arg("metrics")
        .arg("--branch")
        .arg("main")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0));
    let metrics_main: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(metrics_main["sensors"][0]["runs"], 1);

    // --all sums across workstreams (1 on main + 2 on feature = 3):
    let (code, stdout, _) = run(harness(&root)
        .arg("metrics")
        .arg("--all")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0));
    let metrics_all: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(metrics_all["scope"], "all");
    assert_eq!(metrics_all["sensors"][0]["runs"], 3);

    // 3. Explicit --global records into global namespace:
    let (code, _, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--record")
        .arg("--global"));
    assert_eq!(code, Some(0));
    // Explicit --global does not warn:
    assert!(!stderr.contains("warning: verify --record without --task"));

    let (code, stdout, _) = run(harness(&root)
        .arg("metrics")
        .arg("--scope")
        .arg("global")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0));
    let metrics_g: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(metrics_g["sensors"][0]["runs"], 1);
}

/// Like [`fixture_repo`], plus a sensor that always fails, so strikes accrue.
fn fixture_repo_with_failing_sensor() -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    git(&root, &["init", "-q", "-b", "main"]);
    let config = r#"
language = "generic"

[signal-sets]
all = ["s1", "s2"]

[[sensors]]
name = "s1"
argv = ["true"]

[[sensors]]
name = "s2"
argv = ["false"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "base"]);
    (dir, root)
}

/// Fail-fast strikes are workstream-scoped: three failures on one branch must
/// not halt the sensor on another branch, and `metrics` must not show the
/// other workstream's strikes.
#[test]
fn fail_fast_strikes_do_not_cross_workstreams() {
    let (_dir, root) = fixture_repo_with_failing_sensor();

    for _ in 0..3 {
        let (code, stdout, stderr) = run(harness(&root)
            .arg("verify")
            .arg("--set")
            .arg("all")
            .arg("--record"));
        assert_eq!(
            code,
            Some(1),
            "the failing sensor must fail the run:\n{stdout}\n{stderr}"
        );
    }
    let (_, stdout, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--record"));
    assert!(
        stderr.contains("halted: sensor 's2'"),
        "main must be halted after three strikes:\n{stdout}\n{stderr}"
    );

    // A different branch starts clean: strikes are not shared.
    git(&root, &["switch", "-q", "-c", "feat/other"]);
    let (code, stdout, _) = run(harness(&root).arg("metrics").arg("--format").arg("json"));
    assert_eq!(code, Some(0));
    let metrics: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(metrics["scope"], "branch:feat/other");
    assert_eq!(
        metrics["strikes"].as_array().unwrap().len(),
        0,
        "another branch's strikes must not leak into this snapshot: {stdout}"
    );

    // Recording here fails once and strikes only this branch.
    let (_, stdout, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--record"));
    assert!(
        !stderr.contains("halted: sensor 's2'"),
        "another branch must not inherit the strikes:\n{stdout}\n{stderr}"
    );
    let (_, stdout, _) = run(harness(&root).arg("metrics").arg("--format").arg("json"));
    let metrics_feat: Value = serde_json::from_str(&stdout).unwrap();
    let feat_strikes = metrics_feat["strikes"].as_array().unwrap();
    assert_eq!(feat_strikes.len(), 1, "this branch has its own strike");
    assert_eq!(feat_strikes[0]["scope"], "branch:feat/other");
    assert_eq!(feat_strikes[0]["attempt_count"], 1);

    // Back on `main` the strike is still recorded, under its own scope.
    git(&root, &["switch", "-q", "main"]);
    let (code, stdout, _) = run(harness(&root).arg("metrics").arg("--format").arg("json"));
    assert_eq!(code, Some(0));
    let metrics_main: Value = serde_json::from_str(&stdout).unwrap();
    let strikes = metrics_main["strikes"].as_array().unwrap();
    assert_eq!(strikes.len(), 1, "main keeps its strike: {stdout}");
    assert_eq!(strikes[0]["scope"], "branch:main");
    assert_eq!(strikes[0]["signature"], "sensor:s2");
    assert_eq!(strikes[0]["attempt_count"], 3);
}
