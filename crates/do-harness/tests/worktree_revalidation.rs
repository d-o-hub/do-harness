//! Working-tree evidence is compared against the selection that recorded it.

#![allow(clippy::unwrap_used)]

use std::path::Path;

mod support;

use support::{git, harness, run};

fn status(root: &Path, expected: i32) -> serde_json::Value {
    let (code, stdout, stderr) =
        run(harness(root).args(["status", "--set", "verification", "--format", "json"]));
    assert_eq!(code, Some(expected), "{stdout}\n{stderr}");
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn status_revalidates_exact_worktree_selection_after_a_rerun() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"
        [signal-sets]
        verification = ["check"]
        [[sensors]]
        name = "check"
        argv = ["true"]
        when-changed = ["spec.md"]
        inputs = ["spec.md"]
    "#,
    )
    .unwrap();
    let spec = root.join("spec.md");
    std::fs::write(&spec, "original\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "--all"]);
    git(root, &["commit", "-qm", "test: establish worktree"]);

    std::fs::write(&spec, "first revision\n").unwrap();
    let verify = ["verify", "--set", "verification", "--changed", "--strict"];
    let (code, stdout, stderr) = run(harness(root).args(verify));
    assert_eq!(code, Some(0), "{stdout}\n{stderr}");
    assert_eq!(status(root, 0)["state"], "green");

    std::fs::write(&spec, "second revision\n").unwrap();
    assert_eq!(status(root, 1)["state"], "stale");
    let (code, stdout, stderr) = run(harness(root).args(verify));
    assert_eq!(code, Some(0), "{stdout}\n{stderr}");
    assert_eq!(status(root, 0)["state"], "green");
}
