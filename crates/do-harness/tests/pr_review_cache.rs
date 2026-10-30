//! Integration tests for review cache lookups and metadata rebinding (WF-04).

#![allow(clippy::unwrap_used, clippy::expect_used)]
// The instrumented `git` tracker is a bash script, so these fixtures are Unix-only
// by construction (the Windows runner cannot exec a shebang directly from Command).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const EXEC_PERMS: u32 = 0o755;

mod support;

fn git(dir: &Path, args: &[&str]) {
    let output = support::git_command(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
}

fn commit_file(dir: &Path, name: &str, content: &str, message: &str) {
    if let Some(parent) = Path::new(name).parent() {
        let full = dir.join(parent);
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(full).unwrap();
        }
    }
    fs::write(dir.join(name), content).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

fn review_json(cmd: &mut Command) -> Value {
    let output = cmd.output().expect("spawn do-harness");
    assert!(
        output.status.success(),
        "review failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse json report")
}

struct InstrumentedGit {
    _bin_dir: tempfile::TempDir,
    path_env: String,
    log_file: PathBuf,
}

impl InstrumentedGit {
    fn new() -> Self {
        let bin_dir = tempfile::tempdir().unwrap();
        let log_file = bin_dir.path().join("git_invocations.log");
        let shim = bin_dir.path().join("git");
        let script = format!(
            r#"#!/bin/sh
for arg in "$@"; do
    if [ "$arg" = "diff" ]; then
        echo "diff" >> "{}"
    fi
done
exec /usr/bin/git "$@"
"#,
            log_file.display()
        );
        fs::write(&shim, script).unwrap();
        let mut perms = fs::metadata(&shim).unwrap().permissions();
        perms.set_mode(EXEC_PERMS);
        fs::set_permissions(&shim, perms).unwrap();

        let old_path = std::env::var("PATH").unwrap_or_default();
        let path_env = format!("{}:{}", bin_dir.path().display(), old_path);
        Self {
            _bin_dir: bin_dir,
            path_env,
            log_file,
        }
    }

    fn diff_count(&self) -> usize {
        if let Ok(content) = fs::read_to_string(&self.log_file) {
            content.lines().filter(|line| *line == "diff").count()
        } else {
            0
        }
    }
}

#[test]
fn test_warm_cache_avoids_git_diff_subprocess() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "file.txt", "v1\n", "base commit");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "file.txt", "v2\n", "feature commit");

    let git_tracker = InstrumentedGit::new();

    // Cold run: must invoke git diff
    let rep1 = review_json(
        harness(dir.path())
            .env("PATH", &git_tracker.path_env)
            .args([
                "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
            ]),
    );
    assert_eq!(rep1["cached"], false);
    let cold_diffs = git_tracker.diff_count();
    assert!(cold_diffs > 0, "cold run must invoke git diff");

    // Warm run: must NOT invoke git diff
    let rep2 = review_json(
        harness(dir.path())
            .env("PATH", &git_tracker.path_env)
            .args([
                "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
            ]),
    );
    assert_eq!(rep2["cached"], true);
    let warm_diffs = git_tracker.diff_count();
    assert_eq!(
        warm_diffs, cold_diffs,
        "warm review must not invoke git diff subprocess"
    );

    // Substantive residual and measurements must match
    assert_eq!(rep1["residual"], rep2["residual"]);
    assert_eq!(rep1["measurement"]["t_raw"], rep2["measurement"]["t_raw"]);
    assert_eq!(rep1["measurement"]["t_res"], rep2["measurement"]["t_res"]);
}

#[test]
fn test_warm_cache_rebinds_caller_target_metadata() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "file.txt", "v1\n", "base commit");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "file.txt", "v2\n", "feature commit");

    // Cold run with base "main"
    let rep1 = review_json(harness(dir.path()).args([
        "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
    ]));
    assert_eq!(rep1["cached"], false);
    assert_eq!(rep1["base"], "main");
    assert_eq!(rep1["head"], "feature");
    assert_eq!(rep1["mode"], "range");
    assert!(rep1["pr"].is_null());

    // Warm run using a different base ref syntax that resolves to the same merge base commit
    let rep2 = review_json(harness(dir.path()).args([
        "pr", "review", "--base", "HEAD~1", "--head", "feature", "--format", "json",
    ]));
    assert_eq!(rep2["cached"], true);
    assert_eq!(
        rep2["base"], "HEAD~1",
        "warm cache hit must rebind caller's base ref"
    );
    assert_eq!(rep2["head"], "feature");
    assert_eq!(rep2["mode"], "range");
    assert!(rep2["pr"].is_null());

    // Substantive residual must be preserved
    assert_eq!(rep1["residual"], rep2["residual"]);
}

#[test]
fn test_changed_head_recomputes_and_invokes_diff() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "file.txt", "v1\n", "base commit");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "file.txt", "v2\n", "feature commit 1");

    let git_tracker = InstrumentedGit::new();

    let rep1 = review_json(
        harness(dir.path())
            .env("PATH", &git_tracker.path_env)
            .args([
                "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
            ]),
    );
    assert_eq!(rep1["cached"], false);
    let diffs1 = git_tracker.diff_count();

    // Advance feature branch with new commit
    commit_file(dir.path(), "file.txt", "v3\n", "feature commit 2");

    let rep2 = review_json(
        harness(dir.path())
            .env("PATH", &git_tracker.path_env)
            .args([
                "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
            ]),
    );
    assert_eq!(rep2["cached"], false, "changed head must recompute");
    let diffs2 = git_tracker.diff_count();
    assert!(diffs2 > diffs1, "changed head must invoke git diff");
}

#[test]
fn test_changed_policy_recomputes_and_invokes_diff() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"*.lock\"]\n",
        "policy 1",
    );
    commit_file(dir.path(), "file.txt", "v1\n", "file");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "file.txt", "v2\n", "change file");

    let rep1 = review_json(harness(dir.path()).args([
        "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
    ]));
    assert_eq!(rep1["cached"], false);
    let key1_sha = rep1["policy"]["sha256"].as_str().unwrap().to_owned();

    // Switch back to main and change policy
    git(dir.path(), &["switch", "-q", "main"]);
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"*.lock\", \"*.md\"]\n",
        "policy 2",
    );

    // Switch to feature and rebase onto updated main so merge-base advances to policy 2
    git(dir.path(), &["switch", "-q", "feature"]);
    git(dir.path(), &["rebase", "-q", "main"]);

    // Review again against updated main
    let rep2 = review_json(harness(dir.path()).args([
        "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
    ]));
    assert_eq!(rep2["cached"], false, "changed policy must recompute");
    let key2_sha = rep2["policy"]["sha256"].as_str().unwrap().to_owned();
    assert_ne!(key1_sha, key2_sha);
}
