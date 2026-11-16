//! Fixture and helpers for the `do-harness cleanup` integration tests.
//!
//! The fixture builds a real git repository with a configurable remote default
//! branch, a fake `gh` on `PATH`, and worktrees with plausible build trees, so
//! the tests exercise the command rather than a mock of it.

#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

/// A repository with a configurable default branch, a fake `gh`, and helpers to
/// add worktrees.
pub struct Fixture {
    pub dir: TempDir,
    pub root: PathBuf,
    pub gh: PathBuf,
    pub default_branch: String,
}

impl Fixture {
    pub fn new() -> Self {
        Self::on_default_branch("main")
    }

    /// Builds a fixture whose remote default branch is `branch`.
    pub fn on_default_branch(branch: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        git(&root, &["init", "-b", branch]);
        git(&root, &["config", "user.email", "fixture@example.invalid"]);
        git(&root, &["config", "user.name", "Fixture"]);
        std::fs::write(root.join(".gitignore"), "/target\n").unwrap();
        std::fs::write(root.join("README.md"), "fixture\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "init"]);
        let head = git(&root, &["rev-parse", "HEAD"]);
        let reference = format!("refs/remotes/origin/{branch}");
        git(&root, &["update-ref", &reference, head.trim()]);
        git(
            &root,
            &["symbolic-ref", "refs/remotes/origin/HEAD", &reference],
        );
        let gh = fake_gh(dir.path());
        Self {
            dir,
            root,
            gh,
            default_branch: branch.to_owned(),
        }
    }

    /// The remote default branch this fixture pretends to track.
    pub fn default_ref(&self) -> String {
        format!("origin/{}", self.default_branch)
    }

    pub fn mock(&self) -> PathBuf {
        self.dir.path().join("mock")
    }

    /// Adds a worktree on a new branch with one commit and a fake build tree.
    pub fn worktree(&self, branch: &str) -> PathBuf {
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

    /// Merges `branch` into the default branch and republishes it.
    ///
    /// `--no-ff` keeps the merge working when several worktrees branched from
    /// the same commit; a fast-forward is not available for the second one.
    pub fn merge(&self, branch: &str) {
        git(&self.root, &["checkout", "-q", &self.default_branch]);
        git(&self.root, &["merge", "--no-ff", "-m", "merge", branch]);
        let head = git(&self.root, &["rev-parse", "HEAD"]);
        let reference = format!("refs/remotes/{}", self.default_ref());
        git(&self.root, &["update-ref", &reference, head.trim()]);
    }

    /// Records the fake `gh` answers: open PRs and merged PRs.
    pub fn prs(&self, open: &str, merged: &str) {
        let mock = self.mock();
        std::fs::create_dir_all(&mock).unwrap();
        std::fs::write(mock.join("open.json"), open).unwrap();
        std::fs::write(mock.join("merged.json"), merged).unwrap();
    }

    pub fn head(&self, branch: &str) -> String {
        git(&self.root, &["rev-parse", branch]).trim().to_owned()
    }

    pub fn run(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let path = std::env::var("PATH").unwrap_or_default();
        let output = super::harness(&self.root)
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

    pub fn json(&self, args: &[&str]) -> (Option<i32>, Value, String) {
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
pub fn build_tree(worktree: &Path) {
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

pub fn git(dir: &Path, args: &[&str]) -> String {
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
pub fn fake_gh(dir: &Path) -> PathBuf {
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

pub fn merged_pr(pr: u64, head: &str) -> String {
    format!(r#"[{{"number":{pr},"headRefOid":"{head}","mergedAt":"2026-11-16T00:00:00Z"}}]"#)
}

/// A full page of merged pull requests, none of which merged `head`.
///
/// `--limit 100` returning 100 rows is the signal that the list may be
/// truncated, so the proof must fail closed rather than assert the head was
/// never merged.
pub fn full_page_of_unrelated_merges(head: &str) -> String {
    let unrelated = "0".repeat(40);
    let rows: Vec<String> = (0..100)
        .map(|index| {
            format!(
                r#"{{"number":{},"headRefOid":"{unrelated}","mergedAt":"2026-11-16T00:00:00Z"}}"#,
                index + 1
            )
        })
        .collect();
    assert_ne!(unrelated, head);
    format!("[{}]", rows.join(","))
}

pub fn candidate<'a>(document: &'a Value, worktree: &Path) -> &'a Value {
    let worktree = worktree.display().to_string();
    document["candidates"]
        .as_array()
        .expect("candidates array")
        .iter()
        .find(|candidate| candidate["worktree"] == worktree.as_str())
        .unwrap_or_else(|| panic!("no candidate for {worktree}"))
}
