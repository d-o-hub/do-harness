//! Acceptance fixtures for `do-harness cleanup` (#569, #570, #571, #572).
//!
//! Every case drives the real binary against a real git repository with real
//! linked worktrees, and injects `gh` answers through a fake on `PATH` so the
//! merge-proof matrix runs offline.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

mod support;

/// A repository with a `main` branch, a fake `gh`, and helpers to add worktrees.
struct Fixture {
    dir: TempDir,
    root: PathBuf,
    gh: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        git(&root, &["init", "-b", "main"]);
        git(&root, &["config", "user.email", "fixture@example.invalid"]);
        git(&root, &["config", "user.name", "Fixture"]);
        std::fs::write(root.join(".gitignore"), "/target\n").unwrap();
        std::fs::write(root.join("README.md"), "fixture\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "init"]);
        let head = git(&root, &["rev-parse", "HEAD"]);
        git(
            &root,
            &["update-ref", "refs/remotes/origin/main", head.trim()],
        );
        let gh = fake_gh(dir.path());
        Self { dir, root, gh }
    }

    fn mock(&self) -> PathBuf {
        self.dir.path().join("mock")
    }

    /// Adds a worktree on a new branch with one commit and a fake build tree.
    fn worktree(&self, branch: &str) -> PathBuf {
        let path = self.dir.path().join(format!("wt-{branch}"));
        git(
            &self.root,
            &["worktree", "add", "-b", branch, path.to_str().unwrap()],
        );
        std::fs::create_dir_all(path.join("src")).unwrap();
        std::fs::write(
            path.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(path.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
        build_tree(&path);
        git(&path, &["add", "."]);
        git(&path, &["commit", "-m", "work"]);
        path
    }

    /// Fast-forwards `main` to `branch` and republishes `origin/main`.
    fn merge(&self, branch: &str) {
        git(&self.root, &["merge", "--ff-only", branch]);
        let head = git(&self.root, &["rev-parse", "HEAD"]);
        git(
            &self.root,
            &["update-ref", "refs/remotes/origin/main", head.trim()],
        );
    }

    /// Records the fake `gh` answers: open PRs and merged PRs.
    fn prs(&self, open: &str, merged: &str) {
        let mock = self.mock();
        std::fs::create_dir_all(&mock).unwrap();
        std::fs::write(mock.join("open.json"), open).unwrap();
        std::fs::write(mock.join("merged.json"), merged).unwrap();
    }

    fn head(&self, branch: &str) -> String {
        git(&self.root, &["rev-parse", branch]).trim().to_owned()
    }

    fn run(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let path = std::env::var("PATH").unwrap_or_default();
        let output = support::harness(&self.root)
            .args(args)
            .env("PATH", format!("{}:{path}", self.gh.display()))
            .env("GH_MOCK", self.mock())
            .output()
            .expect("spawn do-harness");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    fn json(&self, args: &[&str]) -> (Option<i32>, Value, String) {
        let (code, stdout, stderr) = self.run(args);
        if code != Some(0) {
            eprintln!("DEBUG cleanup exit {code:?}: {stdout}\n{stderr}");
        }
        let document: Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|error| panic!("cleanup JSON: {error}\n{stdout}\n{stderr}"));
        (code, document, stderr)
    }
}

/// Creates a plausible Cargo build tree: cache tag, profile lock, object file.
fn build_tree(worktree: &Path) {
    let debug = worktree.join("target/debug");
    std::fs::create_dir_all(&debug).unwrap();
    std::fs::write(
        worktree.join("target/CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    std::fs::write(debug.join(".cargo-lock"), "").unwrap();
    std::fs::write(debug.join("blob.rlib"), vec![0u8; 4096]).unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Writes a fake `gh` that answers `pr list --state <state>` from a mock file.
fn fake_gh(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
if [ -n "$GH_FAIL" ]; then
    echo "gh: not authenticated" >&2
    exit 1
fi
state=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --state) state="$2"; shift 2 ;;
        *) shift ;;
    esac
done
file="$GH_MOCK/$state.json"
if [ -f "$file" ]; then cat "$file"; else echo '[]'; fi
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn merged_pr(pr: u64, head: &str) -> String {
    format!(r#"[{{"number":{pr},"headRefOid":"{head}","mergedAt":"2026-11-16T00:00:00Z"}}]"#)
}

fn candidate<'a>(document: &'a Value, worktree: &Path) -> &'a Value {
    let worktree = worktree.display().to_string();
    document["candidates"]
        .as_array()
        .expect("candidates array")
        .iter()
        .find(|candidate| candidate["worktree"] == worktree.as_str())
        .unwrap_or_else(|| panic!("no candidate for {worktree}"))
}

#[test]
fn a_merged_worktree_is_proven_by_its_pr_and_cleaned() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("merged");
    let head = fixture.head("merged");
    fixture.merge("merged");
    fixture.prs("[]", &merged_pr(552, &head));

    let (code, document, stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0), "{stderr}\n{document}");
    let candidate = candidate(&document, &worktree);
    assert_eq!(candidate["verdict"], "cleaned", "{document}");
    assert_eq!(candidate["merge_proof"]["kind"], "pr-merged");
    assert_eq!(candidate["merge_proof"]["pr"], 552);
    assert!(candidate["reclaimed_bytes"].as_u64().unwrap() >= 4096);
    assert!(!worktree.join("target").exists(), "build tree must be gone");
    assert!(
        !git(&fixture.root, &["worktree", "list"]).contains(worktree.to_str().unwrap()),
        "worktree registration must be gone"
    );
}

#[test]
fn an_unmerged_worktree_is_refused_and_untouched() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("unmerged");
    fixture.prs("[]", "[]");

    let (code, document, stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0), "a safety refusal is not an error: {stderr}");
    let candidate = candidate(&document, &worktree);
    assert_eq!(candidate["verdict"], "refused");
    assert_eq!(candidate["reason"], "unmerged");
    assert!(worktree.join("target/debug/blob.rlib").exists());
}

#[test]
fn a_dirty_worktree_is_refused() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("dirty");
    let head = fixture.head("dirty");
    fixture.merge("dirty");
    fixture.prs("[]", &merged_pr(1, &head));
    std::fs::write(worktree.join("scratch.txt"), "uncommitted\n").unwrap();

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "dirty");
    assert!(worktree.join("target").exists());
}

#[test]
fn a_locked_worktree_is_refused_with_its_reason() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("locked");
    let head = fixture.head("locked");
    fixture.merge("locked");
    fixture.prs("[]", &merged_pr(2, &head));
    git(
        &fixture.root,
        &[
            "worktree",
            "lock",
            "--reason",
            "network share",
            worktree.to_str().unwrap(),
        ],
    );

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "locked");
    assert!(worktree.join("target").exists());
}

#[test]
fn a_held_cargo_lock_refuses_the_build_tree() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("busy");
    let head = fixture.head("busy");
    fixture.merge("busy");
    fixture.prs("[]", &merged_pr(3, &head));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(worktree.join("target/debug/.cargo-lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "in_use");
    assert!(worktree.join("target/debug/blob.rlib").exists());
    fs2::FileExt::unlock(&lock).unwrap();
}

#[test]
fn a_reused_branch_name_is_proof_stale() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("reused");
    fixture.merge("reused");
    fixture.prs("[]", &merged_pr(4, &"0".repeat(40)));

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "proof_stale");
    assert!(worktree.join("target").exists());
}

#[test]
fn a_missing_gh_fails_closed() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("unproven");
    let head = fixture.head("unproven");
    fixture.merge("unproven");
    fixture.prs("[]", &merged_pr(5, &head));

    let path = std::env::var("PATH").unwrap_or_default();
    let output = support::harness(&fixture.root)
        .args(["cleanup", "--format", "json"])
        .env("PATH", format!("{}:{path}", fixture.gh.display()))
        .env("GH_MOCK", fixture.mock())
        .env("GH_FAIL", "1")
        .output()
        .expect("spawn do-harness");
    assert_eq!(output.status.code(), Some(1), "unproven is error-class");
    let document: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(candidate(&document, &worktree)["reason"], "unproven");
    assert!(worktree.join("target").exists());
}

#[test]
fn a_dry_run_reclaims_nothing_and_repeats_itself() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("dry");
    let head = fixture.head("dry");
    fixture.merge("dry");
    fixture.prs("[]", &merged_pr(6, &head));

    let (code, first, _) = fixture.json(&["cleanup", "--dry-run", "--format", "json"]);
    assert_eq!(code, Some(0));
    let (_, second, _) = fixture.json(&["cleanup", "--dry-run", "--format", "json"]);
    assert_eq!(candidate(&first, &worktree)["verdict"], "eligible");
    assert!(worktree.join("target").exists());
    assert_eq!(normalize(&first), normalize(&second));
}

/// Removes the two fields a dry run may legitimately vary.
fn normalize(document: &Value) -> Value {
    let mut document = document.clone();
    document["generated_at_unix"] = Value::from(0);
    if let Some(candidates) = document["candidates"].as_array_mut() {
        for candidate in candidates {
            candidate["duration_ms"] = Value::from(0);
        }
    }
    document
}

#[test]
fn keep_retains_the_most_recently_used_tree() {
    let fixture = Fixture::new();
    let older = fixture.worktree("older");
    let newer = fixture.worktree("newer");
    fixture.merge("older");
    fixture.merge("newer");
    // Both branches are ancestors of origin/main, so the local fallback proves
    // them without a pull request; the fixture then sets distinct mtimes.
    fixture.prs("[]", "[]");
    set_modified(&older.join("target"), 1_000_000);
    set_modified(&newer.join("target"), 2_000_000);

    let (code, document, _stderr) = fixture.json(&["cleanup", "--keep", "1", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &older)["reason"], "kept");
    assert_eq!(candidate(&document, &newer)["verdict"], "cleaned");
    assert!(older.join("target").exists());
    assert!(!newer.join("target").exists());
}

/// Sets a directory's modification time to `seconds` after the Unix epoch.
fn set_modified(path: &Path, seconds: u64) {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    std::fs::File::open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

#[test]
fn the_main_worktree_is_protected() {
    let fixture = Fixture::new();
    fixture.prs("[]", "[]");
    let root = fixture.root.display().to_string();
    let (code, document, _stderr) =
        fixture.json(&["cleanup", "--worktree", &root, "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &fixture.root)["reason"], "protected");
}
