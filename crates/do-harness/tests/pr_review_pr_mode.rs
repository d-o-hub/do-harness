//! Integration coverage for `do-harness pr review` in PR mode (with fake `gh`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

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
            std::fs::create_dir_all(full).unwrap();
        }
    }
    std::fs::write(dir.join(name), content).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

fn json(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "exit must be 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json report")
}

/// Writes a fake `gh` that reports `head` as PR 7's head and `main` as its base.
///
/// Returns the directory to prepend to `PATH`; nothing in these fixtures talks
/// to `GitHub`.
fn fake_gh(dir: &Path, head: &str) -> std::path::PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s' '{{\"number\":7,\"baseRefName\":\"main\",\"headRefOid\":\"{head}\"}}'\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

/// Runs a `pr` action in PR mode with the fake `gh` first in `PATH`.
fn pr_mode(root: &Path, bin: &Path, args: &[&str]) -> std::process::Output {
    let path = std::env::var("PATH").unwrap_or_default();
    harness(root)
        .args(args)
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("spawn do-harness")
}

/// The revision `rev` resolves to inside `root`.
fn rev(root: &Path, rev: &str) -> String {
    String::from_utf8(
        support::git_command(root)
            .args(["rev-parse", rev])
            .output()
            .expect("rev-parse")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned()
}

/// A stale local base branch must not become the comparison base: a PR's base is
/// the remote branch, so `origin/<base>` wins whenever that ref exists. A stale
/// local `main` reports already-merged upstream commits as this PR's change.
// The fake `gh` is a POSIX shell script, so PR mode is exercised on Unix; the
// Windows job covers the rest of the suite, and these fixtures need no network.
#[cfg(unix)]
#[test]
fn pr_mode_prefers_the_remote_base_over_a_stale_local_branch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit_file(root, "a.txt", "one\n", "base");
    commit_file(root, "merged.txt", "upstream\n", "already merged upstream");
    let remote_sha = rev(root, "HEAD");
    git(
        root,
        &["update-ref", "refs/remotes/origin/main", &remote_sha],
    );
    // The PR branch is based on the remote commit; only the local base branch
    // lags behind it.
    git(root, &["switch", "-q", "-c", "feature"]);
    commit_file(root, "feature.txt", "mine\n", "my change");
    let head = rev(root, "HEAD");
    // `HEAD~2` is the commit *before* the already-merged one.
    git(root, &["branch", "-q", "-f", "main", "HEAD~2"]);
    let bin = fake_gh(root, &head);

    let report = json(&pr_mode(
        root,
        &bin,
        &["pr", "review", "7", "--format", "json"],
    ));
    assert_eq!(report["mode"], serde_json::json!("pr"));
    assert_eq!(
        report["merge_base"],
        serde_json::json!(remote_sha),
        "the remote base is the PR's base: {report}"
    );
    let residual = report["residual"].as_array().unwrap();
    assert!(
        !residual
            .iter()
            .any(|unit| unit["path"] == serde_json::json!("merged.txt")),
        "an already-merged upstream commit leaked into the review: {residual:?}"
    );
    assert!(
        residual
            .iter()
            .any(|unit| unit["path"] == serde_json::json!("feature.txt")),
        "the PR's own change must stay in the review: {residual:?}"
    );
}

/// The same base resolution feeds the no-effect gate: a PR whose head already is
/// the remote base has no effect, even while the local base branch lags behind.
// The fake `gh` is a POSIX shell script, so PR mode is exercised on Unix; the
// Windows job covers the rest of the suite, and these fixtures need no network.
#[cfg(unix)]
#[test]
fn pr_mode_no_effect_uses_the_remote_base() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit_file(root, "a.txt", "one\n", "base");
    commit_file(root, "merged.txt", "upstream\n", "already merged upstream");
    let remote_sha = rev(root, "HEAD");
    git(
        root,
        &["update-ref", "refs/remotes/origin/main", &remote_sha],
    );
    git(root, &["reset", "-q", "--hard", "HEAD~1"]);
    let bin = fake_gh(root, &remote_sha);

    let report = json(&pr_mode(
        root,
        &bin,
        &["pr", "no-effect", "7", "--format", "json"],
    ));
    assert_eq!(
        report["effective_change"],
        serde_json::json!(false),
        "head already equals the remote base: {report}"
    );
}
