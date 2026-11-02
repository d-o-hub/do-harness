//! A sensor whose `argv` names a missing path must not be recorded as a pass.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use serde_json::Value;

mod support;

use support::{harness, run};

/// A one-sensor workspace whose command exits 0 regardless of its arguments.
fn fixture(argv: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::write(
        root.join("do-harness.toml"),
        format!(
            "[signal-sets]\nfeedback = [\"ghost\"]\n\n[[sensors]]\nname = \"ghost\"\nargv = {argv}\n"
        ),
    )
    .unwrap();
    (dir, root)
}

#[test]
fn verify_fails_a_sensor_whose_argv_path_is_missing() {
    // The command exits 0, so only the argv path check can fail the gate.
    let (_dir, root) = fixture("[\"bash\", \"-c\", \"exit 0\", \"scripts/does-not-exist.sh\"]");

    let (code, stdout, stderr) =
        run(harness(&root).args(["verify", "--set", "feedback", "--format", "json"]));

    assert_eq!(code, Some(1), "stdout={stdout}\nstderr={stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("verify JSON");
    assert_eq!(report["ok"], false);
    assert_eq!(report["sensors"][0]["ok"], false);
    assert_eq!(report["sensors"][0]["execution"], "not_run");
    assert!(
        stderr.contains("scripts/does-not-exist.sh"),
        "stderr must name the missing path: {stderr}"
    );
}

#[test]
fn verify_runs_a_sensor_whose_argv_path_exists() {
    let (_dir, root) = fixture("[\"bash\", \"-c\", \"exit 0\", \"scripts/check.sh\"]");
    std::fs::write(root.join("scripts/check.sh"), "exit 0\n").unwrap();

    let (code, stdout, stderr) =
        run(harness(&root).args(["verify", "--set", "feedback", "--format", "json"]));

    assert_eq!(code, Some(0), "stdout={stdout}\nstderr={stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("verify JSON");
    assert_eq!(report["ok"], true);
    assert_eq!(report["sensors"][0]["execution"], "ran");
}
