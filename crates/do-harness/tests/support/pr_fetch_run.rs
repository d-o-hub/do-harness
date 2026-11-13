//! Driving `pr no-effect` against the #520 prefetch fixtures.
//!
//! Shared by `pr_fetch.rs` (does the feature work?) and `pr_fetch_safety.rs`
//! (does it stay inside its bounds?). Both need the same view of what the tool
//! did to the repository, and in particular the same *honest* definition of
//! "untouched": tracked state plus the ref database, never untracked files,
//! which include the fixture's own scaffolding.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

use super::harness;
use super::pr_fetch::Fixture;

/// Runs `pr no-effect <PR>` with the fixture's fake `gh` first on `PATH`.
pub fn run_no_effect(fixture: &Fixture, extra: &[&str]) -> (Option<i32>, Value, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let mut args = vec!["pr", "no-effect", "1263", "--format", "json"];
    args.extend_from_slice(extra);
    let output = harness(&fixture.clone)
        .env("PATH", format!("{}:{path}", fixture.bin().display()))
        .args(&args)
        .output()
        .expect("spawn do-harness");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let report = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    (output.status.code(), report, stderr)
}

/// Whether `rev` resolves to a commit in `dir`.
pub fn rev_exists(dir: &Path, rev: &str) -> bool {
    super::git_command(dir)
        .args(["cat-file", "-e", &format!("{rev}^{{commit}}")])
        .output()
        .expect("spawn git")
        .status
        .success()
}

/// The repository state a prefetch must not disturb.
///
/// Tracked state only. The fixture writes its own `mock-bin/`, `slow-bin/` and
/// `moved_base` scaffolding inside the clone, so an untracked-file comparison
/// would report the test's own files as a mutation by the tool — the assertion
/// has to describe what the *tool* could have changed.
///
/// Ref *kinds* are checked separately by [`assert_refs_untouched`]: a fetch that
/// created a branch or tag would leave the worktree status clean and pass a
/// status-only assertion, which is exactly the mistake the issue's risk section
/// warns about.
pub fn snapshot(dir: &Path) -> (String, String, String) {
    let git = |args: &[&str]| {
        let output = super::git_command(dir)
            .args(args)
            .output()
            .expect("spawn git");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    (
        // Tracked changes and staged changes: these are what a checkout, a
        // reset, or a bad fetch would show up in.
        git(&["status", "--porcelain", "--untracked-files=no"]),
        git(&["branch", "--show-current"]),
        git(&["rev-parse", "HEAD"]),
    )
}

/// Asserts no local branch or tag was created, moved, or deleted.
///
/// This is the safety property the issue's risk section names: only
/// `refs/remotes/pr-triage/*` may be written. `git status` cannot see a bad
/// branch creation, so it is asserted against the ref database directly.
pub fn assert_refs_untouched(dir: &Path, before: &[String]) {
    let output = super::git_command(dir)
        .args([
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/heads/",
            "refs/tags/",
        ])
        .output()
        .expect("spawn git");
    let after: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        after, before,
        "a prefetch must not create, move, or delete a branch or tag"
    );
}

/// The local branches and tags, for [`assert_refs_untouched`].
pub fn refs(dir: &Path) -> Vec<String> {
    let output = super::git_command(dir)
        .args([
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/heads/",
            "refs/tags/",
        ])
        .output()
        .expect("spawn git");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Whether two commits share a merge base.
///
/// `git merge-tree` needs one; two objects fetched into a grafted clone are each
/// their own root, so this is false even when both objects exist. The graft test
/// asserts this *before* the command runs, which is what makes it a real
/// reproduction rather than a description of one.
pub fn connected(dir: &Path, base: &str, head: &str) -> bool {
    super::git_command(dir)
        .args(["merge-base", base, head])
        .output()
        .expect("spawn git")
        .status
        .success()
}
