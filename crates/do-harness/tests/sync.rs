//! Acceptance fixtures for `do-harness sync`.
//!
//! Every scenario builds a real worktree and bare `origin`, then drives the
//! compiled CLI. The cases pin the two safety rules the command exists for:
//! a dirty checkout is never touched, and a diverged `main` is never reset
//! without `--reset-diverged`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use serde_json::Value;

mod support;

use support::{git, git_command, harness, run};

/// Worktree plus the bare remote it pushes to.
struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    bare: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("work");
    let bare = temp.path().join("origin.git");
    std::fs::create_dir_all(&root).expect("create worktree");
    git(&root, &["init", "-q", "--initial-branch=main"]);
    std::fs::write(root.join("README.md"), "one\n").expect("write readme");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    git(
        temp.path(),
        &[
            "init",
            "--bare",
            "-q",
            "--initial-branch=main",
            "origin.git",
        ],
    );
    git(&root, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&root, &["push", "-q", "-u", "origin", "main"]);
    Fixture { temp, root, bare }
}

/// Pushes one upstream commit from a throwaway clone and returns its id.
fn advance_origin(fx: &Fixture, subject: &str) -> String {
    let clone = fx.temp.path().join("upstream");
    git(
        fx.temp.path(),
        &[
            "clone",
            "-q",
            fx.bare.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    std::fs::write(clone.join("upstream.txt"), format!("{subject}\n")).expect("write upstream");
    git(&clone, &["add", "."]);
    git(&clone, &["commit", "-q", "-m", subject]);
    git(&clone, &["push", "-q", "origin", "main"]);
    rev(&clone, "HEAD")
}

/// Runs a git command and returns trimmed stdout.
fn rev(root: &Path, reference: &str) -> String {
    let output = git_command(root)
        .args(["rev-parse", reference])
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git rev-parse {reference} failed in {}",
        root.display()
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Commits `file` with `content` and returns the new commit id.
fn commit(root: &Path, file: &str, content: &str, subject: &str) -> String {
    std::fs::write(root.join(file), content).expect("write file");
    git(root, &["add", file]);
    git(root, &["commit", "-q", "-m", subject]);
    rev(root, "HEAD")
}

/// Runs `sync --format json` and parses its report.
fn sync_json(fx: &Fixture, args: &[&str]) -> (Option<i32>, Value, String) {
    let mut command = harness(&fx.root);
    command.args(["sync", "--format", "json"]).args(args);
    let (code, stdout, stderr) = run(&mut command);
    let report = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("sync must emit JSON ({err}):\n{stdout}\n{stderr}"));
    (code, report, stderr)
}

#[test]
fn up_to_date_main_reports_aligned() {
    let fx = fixture();
    let (code, report, stderr) = sync_json(&fx, &[]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(report["state"], "up_to_date");
    assert_eq!(report["ahead"], 0);
    assert_eq!(report["behind"], 0);
    assert_eq!(report["aligned"], true);
    assert_eq!(report["clean"], true);
}

#[test]
fn strictly_behind_main_fast_forwards() {
    let fx = fixture();
    let upstream = advance_origin(&fx, "upstream change");
    let (code, report, stderr) = sync_json(&fx, &[]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(report["state"], "synced");
    assert_eq!(report["fast_forwarded"], true);
    assert_eq!(report["advertised"], upstream);
    assert_eq!(report["aligned"], true);
    assert_eq!(rev(&fx.root, "HEAD"), upstream);
}

#[test]
fn switch_flag_moves_a_clean_feature_branch_to_main() {
    let fx = fixture();
    git(&fx.root, &["switch", "-q", "-c", "feature"]);
    commit(&fx.root, "feature.txt", "feature\n", "feature work");
    let (code, report, stderr) = sync_json(&fx, &["--switch"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(report["state"], "switched");
    assert_eq!(report["switched"], true);
    assert_eq!(report["previous_branch"], "feature");
    assert_eq!(branch(&fx.root), "main");
}

#[test]
fn feature_branch_without_switch_is_only_fetched() {
    let fx = fixture();
    git(&fx.root, &["switch", "-q", "-c", "feature"]);
    let (code, report, stderr) = sync_json(&fx, &[]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(report["state"], "fetched");
    assert_eq!(report["switched"], false);
    assert_eq!(branch(&fx.root), "feature");
}

#[test]
fn dirty_feature_branch_is_refused_without_switching() {
    let fx = fixture();
    git(&fx.root, &["switch", "-q", "-c", "feature"]);
    std::fs::write(fx.root.join("dirty.txt"), "uncommitted\n").expect("write dirty file");
    let (code, report, _) = sync_json(&fx, &["--switch"]);
    assert_eq!(code, Some(1));
    assert_eq!(report["state"], "refused_dirty");
    assert_eq!(report["switched"], false);
    assert_eq!(branch(&fx.root), "feature");
}

#[test]
fn diverged_main_is_refused_without_resetting() {
    let fx = fixture();
    let local = commit(&fx.root, "local.txt", "local\n", "local only");
    advance_origin(&fx, "upstream change");
    let (code, report, _) = sync_json(&fx, &[]);
    assert_eq!(code, Some(1));
    assert_eq!(report["state"], "refused_diverged");
    assert_eq!(report["ahead"], 1);
    assert_eq!(report["behind"], 1);
    assert_eq!(report["aligned"], false);
    assert!(
        report["local_commits"][0]
            .as_str()
            .unwrap()
            .contains("local only")
    );
    assert_eq!(rev(&fx.root, "HEAD"), local, "refusal must not reset");
}

#[test]
fn reset_diverged_backs_up_then_resets() {
    let fx = fixture();
    let local = commit(&fx.root, "local.txt", "local\n", "local only");
    let upstream = advance_origin(&fx, "upstream change");
    let (code, report, stderr) = sync_json(&fx, &["--reset-diverged"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(report["state"], "synced");
    assert_eq!(report["reset"], true);
    let backup = report["backup_ref"].as_str().expect("backup ref");
    assert!(backup.starts_with("backup/main-pre-sync-"), "{backup}");
    assert_eq!(rev(&fx.root, backup), local);
    assert_eq!(rev(&fx.root, "HEAD"), upstream);
}

#[test]
fn reset_diverged_switches_from_a_clean_feature_branch() {
    let fx = fixture();
    let local = commit(&fx.root, "local.txt", "local\n", "local only");
    let upstream = advance_origin(&fx, "upstream change");
    git(&fx.root, &["switch", "-q", "-c", "feature"]);
    commit(&fx.root, "feature.txt", "feature\n", "feature work");
    let (code, report, stderr) = sync_json(&fx, &["--reset-diverged"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(report["state"], "synced");
    assert_eq!(report["switched"], true);
    assert_eq!(report["previous_branch"], "feature");
    assert_eq!(report["reset"], true);
    assert_eq!(branch(&fx.root), "main");
    let backup = report["backup_ref"].as_str().expect("backup ref");
    assert_eq!(rev(&fx.root, backup), local);
    assert_eq!(rev(&fx.root, "HEAD"), upstream);
}

#[test]
fn switch_refuses_a_diverged_main_without_leaving_the_branch() {
    let fx = fixture();
    let local = commit(&fx.root, "local.txt", "local\n", "local only");
    advance_origin(&fx, "upstream change");
    git(&fx.root, &["switch", "-q", "-c", "feature"]);
    let (code, report, _) = sync_json(&fx, &["--switch"]);
    assert_eq!(code, Some(1));
    assert_eq!(report["state"], "refused_diverged");
    assert_eq!(report["switched"], false);
    assert_eq!(report["ahead"], 1);
    assert_eq!(
        branch(&fx.root),
        "feature",
        "a refusal must not move the checkout"
    );
    assert_eq!(rev(&fx.root, "main"), local, "refusal must not move main");
}

#[test]
fn read_only_reports_without_writing_refs() {
    let fx = fixture();
    let before = rev(&fx.root, "origin/main");
    let upstream = advance_origin(&fx, "upstream change");
    let (code, report, stderr) = sync_json(&fx, &["--read-only"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(report["state"], "read_only");
    assert_eq!(report["advertised"], upstream);
    assert_eq!(
        rev(&fx.root, "origin/main"),
        before,
        "read-only must not update the tracking ref"
    );
}

#[test]
fn global_dry_run_is_read_only() {
    let fx = fixture();
    let before = rev(&fx.root, "origin/main");
    let upstream = advance_origin(&fx, "upstream change");
    let mut command = harness(&fx.root);
    command.args(["--dry-run", "sync", "--format", "json"]);
    let (code, stdout, stderr) = run(&mut command);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let report: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("sync must emit JSON ({err}):\n{stdout}\n{stderr}"));
    assert_eq!(report["state"], "read_only");
    assert_eq!(report["advertised"], upstream);
    assert_eq!(
        rev(&fx.root, "origin/main"),
        before,
        "--dry-run must not update the tracking ref"
    );
}

#[test]
fn missing_origin_is_a_usage_error() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("work");
    std::fs::create_dir_all(&root).expect("create worktree");
    git(&root, &["init", "-q", "--initial-branch=main"]);
    std::fs::write(root.join("README.md"), "one\n").expect("write readme");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    let (code, _, _) = run(harness(&root).arg("sync"));
    assert_eq!(code, Some(2));
}

#[test]
fn status_flag_embeds_verification_freshness() {
    let fx = fixture();
    std::fs::write(
        fx.root.join("do-harness.toml"),
        "[[sensors]]\nname = \"noop\"\nargv = [\"true\"]\nwhen-changed = [\"**/*\"]\n",
    )
    .expect("write config");
    let (code, report, stderr) = sync_json(&fx, &["--status"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(report["state"], "up_to_date");
    assert_eq!(report["verification"]["state"], "missing");
}

/// Current branch name reported by git.
fn branch(root: &Path) -> String {
    let output = git_command(root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git rev-parse --abbrev-ref HEAD failed in {}",
        root.display()
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}
