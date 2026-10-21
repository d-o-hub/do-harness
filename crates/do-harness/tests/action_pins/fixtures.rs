//! Shared fixtures for the `action-pins` sensor tests.
//!
//! The checker is a process boundary, so every case runs the real script
//! against a temporary repository: the fixtures are what the sensor decides
//! on, and a unit test of the reference grammar would not prove that
//! discovery, YAML parsing, and the exit code agree with the policy.
//!
//! The positive controls use full commit SHAs that this repository actually
//! references, so a passing fixture cannot rest on a made-up placeholder that
//! happens to look like a SHA.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::support;

/// The full commit SHA of `actions/checkout`, as this repository pins it.
pub const CHECKOUT_SHA: &str = "3d3c42e5aac5ba805825da76410c181273ba90b1";
/// A sha256 image digest, the immutable form of a Docker action image.
pub const IMAGE_DIGEST: &str =
    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The checker under test.
pub fn checker() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-action-pins.py")
}

/// A temporary repository root with a `.github/workflows` directory.
pub fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::create_dir_all(dir.path().join(".github/workflows")).expect("create workflows dir");
    dir
}

/// A temporary repository root that is a real git repository, so the
/// checker's git-based discovery runs against working-tree state.
pub fn git_workspace() -> tempfile::TempDir {
    let dir = workspace();
    let status = support::git_command(dir.path())
        .args(["init", "--quiet"])
        .status()
        .expect("run git init");
    assert!(status.success(), "git init must succeed");
    dir
}

/// Writes a workflow file below the root, creating parent directories.
pub fn write_workflow(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("workflow parent")).expect("create parent");
    std::fs::write(path, body).expect("write workflow");
}

/// Runs the checker against a root, returning its exit code and output.
pub fn run(root: &Path) -> (i32, String) {
    let mut cmd = Command::new("python3");
    cmd.arg(checker()).arg("--root").arg(root);
    support::isolate_command(&mut cmd);
    let output: Output = cmd.output().expect("spawn check-action-pins.py");
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), text)
}

/// A workflow with one pinned step, used as the base of the negative cases.
pub fn workflow_with(reference: &str) -> String {
    format!(
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {reference}\n"
    )
}
