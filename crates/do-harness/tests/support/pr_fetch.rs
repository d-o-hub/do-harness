//! Shared fixtures for the `pr no-effect` prefetch tests (#520).
//!
//! Unlike the sweep fixtures, these drive a **real** remote: a local bare
//! repository, cloned shallowly. That is the only way to prove the properties
//! the issue is about.
//!
//! # The shape that matters
//!
//! The obvious fixture — shallow-clone, base missing — is the *easy* case. The
//! one that actually breaks is subtler, and it is the common one in production:
//!
//! 1. the pull request is opened from `main`'s tip;
//! 2. a shallow clone is taken at that same tip, so it has the reported
//!    `baseRefOid` **and a `.git/shallow` graft at it**;
//! 3. the head, reachable only through `refs/pull/N/head`, is absent.
//!
//! Fetching the head then brings it in as its own root, because the graft at the
//! base tip severs ancestry. Both commits exist and `git merge-tree` still
//! refuses:
//!
//! ```text
//! fatal: refusing to merge unrelated histories
//! ```
//!
//! So [`setup`] builds exactly that, and [`setup_with_base_ahead`] builds the
//! case where `main` has also moved past the clone. A fixture that only covers
//! the second case passes with or without the ancestry repair, which is how the
//! repair first went untested.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The PR number every fixture refers to.
pub const PR: u64 = 1263;

/// Commits of history before the fork point.
///
/// More than one on purpose: the repair works by *walking history back* to a
/// common ancestor, so a single-commit repository would have nothing to walk to
/// and the shallow-graft repair could not be exercised at all.
const HISTORY: usize = 8;

/// Commits `main` gains after the clone, for the base-absent shape.
const AFTER_CLONE: usize = 2;

/// A remote plus a shallow clone of it, with the state the proof must repair.
pub struct Fixture {
    _dir: tempfile::TempDir,
    /// The bare repository the clone fetches from.
    pub remote: PathBuf,
    /// The shallow clone, which is `--root` for the CLI.
    pub clone: PathBuf,
    /// The base commit `GitHub` reports.
    pub base: String,
    /// The pull request head commit: reachable only through `refs/pull/N/head`.
    pub head: String,
    /// The commit the pull request forked from.
    pub fork_point: String,
    /// A seed working tree, for advancing the remote mid-test.
    seed: PathBuf,
}

impl Fixture {
    /// The `gh` shim directory to put on `PATH`.
    #[must_use]
    pub fn bin(&self) -> PathBuf {
        self.clone.join("mock-bin")
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = git_output(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn git_output(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("spawn git")
}

/// Takes the shallow clone the whole fixture is about.
fn clone_shallow(root: &Path, remote: &Path, clone: &Path) {
    git(
        root,
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            "--branch",
            "main",
            &format!("file://{}", remote.display()),
            clone.to_str().unwrap(),
        ],
    );
}

fn commit(dir: &Path, file: &str, content: &str, message: &str) {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// The shape that severs ancestry: the clone sits on the current base tip while
/// the pull request forked from a commit strictly below it.
///
/// The reported base is **present** (the clone is at it) and the head is absent,
/// so nothing looks missing. Fetching the head still leaves the pair
/// disconnected, because the graft marks the base tip as a root and the head's
/// parent — the fork point — is therefore unreachable. This is the case that
/// needs the history deepened, and the one a "fetch the objects" fix gets wrong.
///
/// `head_changes_tree` decides whether the PR commit modifies a file, which is
/// what separates `TREE_EFFECT` from `NO_TREE_EFFECT`.
pub fn setup(head_changes_tree: bool) -> Fixture {
    build(head_changes_tree, true)
}

/// The shape the issue reported: the clone sits at the fork point and `main` has
/// moved past it, so the reported base is absent locally.
///
/// The weaker test. Fetching the base by object id pulls in the commits between
/// it and the graft, so the pair usually connects without any deepening — which
/// is precisely why covering only this case once left the ancestry repair
/// untested. Kept because it is what production reported and because it
/// exercises the base-by-object-id fetch.
pub fn setup_with_base_absent(head_changes_tree: bool) -> Fixture {
    build(head_changes_tree, false)
}

/// Builds a fixture.
///
/// `clone_after_advance` decides where the shallow clone sits, and that single
/// choice is what distinguishes the two shapes above:
///
/// * `true` — `main` advances first, then the clone is taken at the new tip, so
///   the base is present but the fork point is an unreachable ancestor;
/// * `false` — the clone is taken first, then `main` advances, so the base is
///   absent.
fn build(head_changes_tree: bool, clone_after_advance: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let remote = root.join("origin.git");
    let seed = root.join("seed");

    git(root, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    // Fetching a commit by object id is what the base strategy needs, and a
    // local test remote refuses it unless asked. Real `GitHub` already allows it.
    git(
        &remote,
        &["config", "uploadpack.allowAnySHA1InWant", "true"],
    );

    std::fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-q", "-b", "main"]);
    for index in 0..HISTORY {
        commit(
            &seed,
            "history.txt",
            &format!("v{index}\n"),
            &format!("c{index}"),
        );
    }
    git(
        &seed,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&seed, &["push", "-q", "origin", "main"]);
    let fork_point = git(&seed, &["rev-parse", "HEAD"]);

    // The PR branch, cut from the fork point.
    git(&seed, &["checkout", "-q", "-b", "topic"]);
    if head_changes_tree {
        commit(&seed, "a.txt", "pr change\n", "pr change");
    } else {
        git(&seed, &["commit", "-q", "--allow-empty", "-m", "empty pr"]);
    }
    let head = git(&seed, &["rev-parse", "HEAD"]);
    git(&seed, &["push", "-q", "origin", "topic"]);
    // Publish the ref `GitHub` exposes for a pull request, so the head is
    // reachable only through `refs/pull/N/head` — exactly as in production.
    git(
        &seed,
        &["push", "-q", "origin", &format!("HEAD:refs/pull/{PR}/head")],
    );
    git(&seed, &["checkout", "-q", "main"]);
    let clone = root.join("clone");
    let advance = |seed: &Path| {
        for index in 0..AFTER_CLONE {
            commit(seed, "b.txt", &format!("after {index}\n"), "main moves on");
        }
        let moved = git(seed, &["rev-parse", "HEAD"]);
        git(seed, &["push", "-q", "origin", "main"]);
        moved
    };

    let base = if clone_after_advance {
        // Advance first: the clone will sit on the new tip, with the fork point
        // stranded below the graft.
        let moved = advance(&seed);
        clone_shallow(root, &remote, &clone);
        moved
    } else {
        // Clone first, at the fork point, then advance `main` away from it.
        clone_shallow(root, &remote, &clone);
        advance(&seed)
    };

    let fixture = Fixture {
        _dir: dir,
        remote,
        clone,
        base,
        head,
        fork_point,
        seed,
    };
    write_gh_shim(&fixture, &fixture.base, &fixture.head, false);
    fixture
}

impl Fixture {
    /// Advances the remote's `main` without touching the clone.
    ///
    /// The shim keeps reporting the original base until
    /// [`Fixture::report_moved_base`] is called, which models the base moving
    /// *after* the proof was planned.
    pub fn advance_base(&mut self) -> String {
        git(&self.seed, &["checkout", "-q", "main"]);
        commit(&self.seed, "c.txt", "even later\n", "main moves again");
        let sha = git(&self.seed, &["rev-parse", "HEAD"]);
        git(&self.seed, &["push", "-q", "origin", "main"]);
        self.base.clone_from(&sha);
        sha
    }

    /// Makes the shim answer `planned` on the first `pr view` and `moved` on
    /// every read afterwards, which is what this command observes when a push
    /// lands between planning and the re-read.
    pub fn report_moved_base(&self, planned: &str, moved: &str) {
        std::fs::write(self.clone.join("moved_base"), moved).unwrap();
        let _ = std::fs::remove_file(self.clone.join("view_seen"));
        write_gh_shim(self, planned, &self.head, true);
    }
}

/// Writes a fake `gh` that answers the two commands `pr no-effect` needs.
///
/// `gh pr view` reports the fixture's real object ids, and `gh api compare`
/// answers a file count. A `gh` reaching the network here would make the test
/// depend on a live pull request, so everything is answered locally.
///
/// With `moving`, the first view reports `first_base` and every later one reports
/// the contents of `<clone>/moved_base`, so a re-read observes a move.
fn write_gh_shim(fixture: &Fixture, first_base: &str, head: &str, moving: bool) {
    let bin = fixture.bin();
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    let seen = fixture.clone.join("view_seen");
    let moved = fixture.clone.join("moved_base");
    let base_branch = if moving {
        format!(
            r#"if [ -f "{seen}" ]; then
    base=$(cat "{moved}")
else
    : > "{seen}"
    base="{first_base}"
fi"#,
            seen = seen.display(),
            moved = moved.display(),
        )
    } else {
        format!("base=\"{first_base}\"")
    };
    let body = format!(
        r#"#!/bin/sh
# Fake `gh` for the #520 prefetch fixtures. Answers from the fixture's real
# object ids; never touches the network.
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
{base_branch}
    printf '{{"number":{PR},"baseRefName":"main","baseRefOid":"%s","headRefOid":"{head}"}}' "$base"
    exit 0
fi
if [ "$1" = "api" ]; then
    case "$*" in
        *compare*) printf '%s' "1"; exit 0 ;;
    esac
fi
echo "unexpected gh invocation: $*" >&2
exit 1
"#,
    );
    std::fs::write(&script, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
