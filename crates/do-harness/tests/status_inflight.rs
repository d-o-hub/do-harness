//! Marker contract for `status --set <set>` while a verify run is in flight.
//!
//! These hold a marker lock from the test process instead of spawning a shell,
//! so the Windows runner exercises the same probe.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use serde_json::Value;

mod support;

use support::{git, harness, run};

/// Creates a committed repo with one set, returning the running-marker dir.
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(
        root.join("do-harness.toml"),
        "[signal-sets]\nverification = [\"a\"]\n\n[[sensors]]\nname = \"a\"\nargv = [\"true\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("lib.rs"), "v1\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);
    let markers = root.join(".do-harness/evidence.verification.running");
    std::fs::create_dir_all(&markers).unwrap();
    (dir, root, markers)
}

/// Runs `status --set verification --format json`.
fn status(root: &Path) -> (Option<i32>, Value) {
    let (code, stdout, stderr) =
        run(harness(root).args(["status", "--set", "verification", "--format", "json"]));
    (
        code,
        serde_json::from_str(&stdout)
            .unwrap_or_else(|_| panic!("status stdout is not JSON:\n{stdout}\n{stderr}")),
    )
}

/// Returns a marker file held under a shared lock, as a live run holds it.
fn held_marker(markers: &Path, contents: &str) -> std::fs::File {
    let path = markers.join("999999-1.json");
    std::fs::write(&path, contents).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    // Fully qualified: `File::lock_shared` is newer than the MSRV, and the
    // inherent method would shadow the fs2 trait.
    fs2::FileExt::lock_shared(&held).unwrap();
    held
}

/// A held marker reports `running` with the published metadata.
#[test]
fn held_marker_lock_reports_running() {
    let (_dir, root, markers) = fixture();
    let held = held_marker(
        &markers,
        "{\"pid\":999999,\"started_at\":1,\"set\":\"verification\"}",
    );

    let (code, doc) = status(&root);
    assert_eq!(code, Some(3), "a held marker must be running: {doc}");
    assert_eq!(doc["state"], serde_json::json!("running"));
    assert_eq!(doc["reason"], serde_json::json!("in_progress"));
    assert_eq!(doc["running"]["pid"], serde_json::json!(999_999));
    assert_eq!(
        doc["running"]["set"],
        serde_json::json!("verification"),
        "running metadata must name the set: {doc}"
    );

    fs2::FileExt::unlock(&held).unwrap();
}

/// A marker left by a run that died holds no lock; status must not report
/// running, and a later verify still succeeds.
#[test]
fn stale_marker_and_lock_are_not_running() {
    let (_dir, root, markers) = fixture();
    std::fs::write(markers.join("999999-1.json"), "{\"pid\":999999}").unwrap();

    let (code, doc) = status(&root);
    assert_eq!(code, Some(1), "a stale marker is not running: {doc}");
    assert_eq!(doc["state"], serde_json::json!("missing"));

    let (code, _stdout, stderr) = run(harness(&root).args(["verify", "--set", "verification"]));
    assert_eq!(code, Some(0), "verify must still run: {stderr}");
    assert!(
        !markers.join("999999-1.json").exists(),
        "a new run must prune markers no run holds"
    );
    let (code, doc) = status(&root);
    assert_eq!(code, Some(0), "completed run must be green: {doc}");
    assert_eq!(doc["state"], serde_json::json!("green"));
}
