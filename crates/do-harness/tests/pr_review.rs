//! Integration coverage for `do-harness pr review` in local range mode.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

mod support;

fn git(dir: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    command.current_dir(dir);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
    ] {
        command.env_remove(key);
    }
    let output = command.args(args).output().expect("spawn git");
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

fn review(root: &Path, extra: &[&str]) -> std::process::Output {
    let mut cmd = harness(root);
    cmd.args(["pr", "review", "--base", "main", "--head", "feature"]);
    cmd.args(extra);
    cmd.output().expect("spawn do-harness")
}

fn json(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "exit must be 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json report")
}

fn feature_with_two_files(dir: &Path) {
    init_repo(dir);
    commit_file(dir, "a.txt", "one\n", "base");
    git(dir, &["switch", "-q", "-c", "feature"]);
    commit_file(dir, "a.txt", "two\n", "change a");
    commit_file(dir, "b.txt", "new\n", "add b");
}

#[test]
fn range_review_lists_residual_units_and_serves_cache() {
    let dir = tempfile::tempdir().unwrap();
    feature_with_two_files(dir.path());

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["mode"], serde_json::json!("range"));
    assert_eq!(report["cached"], serde_json::json!(false));
    assert_eq!(report["policy"]["present"], serde_json::json!(false));
    assert_eq!(report["skipped"].as_array().unwrap().len(), 0);
    assert_eq!(report["false_proven"].as_array().unwrap().len(), 0);
    assert!(!report["merge_base"].as_str().unwrap().is_empty());
    let measurement = &report["measurement"];
    assert!(measurement["t_raw"].as_u64().unwrap() > 0);
    assert!(measurement["t_res"].as_u64().unwrap() > 0);
    assert!(measurement["ratio"].as_f64().unwrap() > 0.0);
    assert!(measurement["verdict"].is_string());
    let residual = report["residual"].as_array().unwrap();
    assert!(residual.len() >= 2, "residual: {residual:?}");
    assert!(residual.iter().all(|unit| unit["id"].is_string()));
    assert!(
        residual
            .iter()
            .any(|unit| unit["path"] == serde_json::json!("b.txt"))
    );

    let cached = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(cached["cached"], serde_json::json!(true));

    let fresh = json(&review(dir.path(), &["--format", "json", "--recompute"]));
    assert_eq!(fresh["cached"], serde_json::json!(false));
}

#[test]
fn no_effect_range_reports_empty_residual() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    git(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "empty"],
    );

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["residual"].as_array().unwrap().len(), 0);
    assert!(report["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn policy_is_read_from_the_merge_base_not_the_head() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"**/Cargo.lock\"]\n",
        "policy base",
    );
    let base_sha = String::from_utf8(
        support::git_command(dir.path())
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("rev-parse")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[[[ malformed head policy\n",
        "policy head",
    );

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["policy"]["present"], serde_json::json!(true));
    assert_eq!(report["policy"]["source_rev"], serde_json::json!(base_sha));
    assert!(
        report["warnings"].as_array().unwrap().is_empty(),
        "head policy must not be parsed: {}",
        report["warnings"]
    );
}

#[test]
fn malformed_base_policy_warns_but_keeps_reviewing() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[[[ malformed\n",
        "policy base",
    );
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "one\n", "change");

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["policy"]["present"], serde_json::json!(true));
    assert!(!report["warnings"].as_array().unwrap().is_empty());
    assert!(report["skipped"].as_array().unwrap().is_empty());
}

#[test]
fn policy_skips_mechanical_units_and_audits_them() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "src/lib.rs", "fn a() {}\n", "src base");
    commit_file(dir.path(), "Cargo.lock", "# lock\n", "lock base");
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"**/Cargo.lock\"]\n",
        "policy",
    );
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "Cargo.lock", "# lock\n# dep\n", "lock change");
    commit_file(dir.path(), "src/lib.rs", "fn a() { b(); }\n", "src change");

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["policy"]["present"], serde_json::json!(true));
    let paths = |key: &str| -> Vec<String> {
        report[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| unit["path"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(paths("skipped"), vec!["Cargo.lock"]);
    assert_eq!(paths("residual"), vec!["src/lib.rs"]);
    assert!(report["false_proven"].as_array().unwrap().is_empty());
    assert!(report["warnings"].as_array().unwrap().is_empty());
    assert_eq!(
        report["measurement"]["verdict"],
        serde_json::json!("reduced")
    );
    let t_raw = report["measurement"]["t_raw"].as_u64().unwrap();
    let t_res = report["measurement"]["t_res"].as_u64().unwrap();
    assert!(t_res < t_raw, "t_res={t_res} t_raw={t_raw}");
}

#[test]
fn seeded_defect_is_never_skipped() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        "crates/core/src/lib.rs",
        "pub fn a() {}\n",
        "base",
    );
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"**/*.rs\"]\nbehavioral = [\"crates/**\"]\n",
        "policy",
    );
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(
        dir.path(),
        "crates/core/src/lib.rs",
        "pub fn a() { panic!(); }\n",
        "defect",
    );

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert!(report["skipped"].as_array().unwrap().is_empty());
    assert_eq!(report["residual"].as_array().unwrap().len(), 1);
    assert!(!report["false_proven"].as_array().unwrap().is_empty());
}

#[test]
fn invalid_glob_proves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "Cargo.lock", "# lock\n", "lock base");
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"**/Cargo.lock\"]\nbehavioral = [\"[\"]\n",
        "policy",
    );
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "Cargo.lock", "# lock\n# dep\n", "lock change");

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert!(report["skipped"].as_array().unwrap().is_empty());
    assert!(!report["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn structural_rename_is_proven_with_policy() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "old.txt", "content\n", "base");
    commit_file(dir.path(), ".github/pr-gate.toml", "[proof]\n", "policy");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "rename"]);

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert!(report["residual"].as_array().unwrap().is_empty());
    let skipped = report["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["path"], serde_json::json!("new.txt"));
    assert_eq!(skipped[0]["header"], serde_json::json!("rename-only"));
}

#[test]
fn missing_revision_is_an_analysis_error() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");

    let output = harness(dir.path())
        .args(["pr", "review", "--base", "missing", "--head", "main"])
        .output()
        .expect("spawn do-harness");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn mixed_modes_and_missing_target_are_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mixed = harness(dir.path())
        .args(["pr", "review", "42", "--base", "main", "--head", "HEAD"])
        .output()
        .expect("spawn do-harness");
    assert_eq!(mixed.status.code(), Some(2));

    let missing = harness(dir.path())
        .args(["pr", "review"])
        .output()
        .expect("spawn do-harness");
    assert_eq!(missing.status.code(), Some(2));
}

#[test]
fn head_change_invalidates_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    feature_with_two_files(dir.path());

    let first = json(&review(dir.path(), &["--format", "json"]));
    let count = first["residual"].as_array().unwrap().len();

    commit_file(dir.path(), "c.txt", "third\n", "add c");
    let second = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(second["cached"], serde_json::json!(false));
    assert!(second["residual"].as_array().unwrap().len() > count);
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
