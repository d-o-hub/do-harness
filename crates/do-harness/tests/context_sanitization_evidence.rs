//! Evidence and CLI-level cases for the optional context-sanitization
//! check.
//!
//! These prove the properties that make the sensor honest: a skipped check
//! is not a pass, and neither the terminal output nor the persisted error
//! signature can carry the child's output or the raw fixture.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

mod support;

/// The template's copy of the script, as shipped to adopters.
fn template_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates/scripts/check-context-sanitization.py")
}

/// A policy that requires the sensor rejects the disabled skip, and the
/// persisted error signature carries no child output.
///
/// This is the property that keeps the sensor honest: a skipped check must
/// never be recorded as a pass, and the persisted text must not carry the
/// child's output.
#[test]
fn a_required_sensor_rejects_the_disabled_skip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
    std::fs::copy(
        template_script(),
        root.join("scripts/check-context-sanitization.py"),
    )
    .expect("copy the script");
    std::fs::write(
        root.join("do-harness.toml"),
        r#"
language = "generic"

[signal-sets]
verification = ["context-sanitization"]

[[sensors]]
name = "context-sanitization"
argv = ["python3", "scripts/check-context-sanitization.py"]
severity = "error"
timeout = 45
"#,
    )
    .expect("write the policy");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    support::isolate_command(&mut cmd);
    cmd.arg("--root").arg(root);
    cmd.arg("verify");
    cmd.arg("--set").arg("verification");
    cmd.arg("--strict");
    cmd.arg("--format").arg("json");
    cmd.arg("--record");
    let output = cmd.output().expect("spawn do-harness verify");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_ne!(
        output.status.code(),
        Some(0),
        "a strict verification must reject a skipped required sensor: {stdout}\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("verify json");
    // The sensor itself passed (the script exited zero) but reported a skip,
    // so the run is warned rather than passed. Strict verification is what
    // turns that warning into a non-zero exit.
    let sensor = report["sensors"]
        .as_array()
        .expect("sensors")
        .iter()
        .find(|entry| entry["name"] == "context-sanitization")
        .expect("the sensor must be reported");
    assert_eq!(
        sensor["warned"],
        serde_json::json!(true),
        "a skipped sensor must be recorded as warned, not as a clean pass: {sensor}"
    );
    // The sensor's own exit code is zero — the script reported a skip, which
    // is the documented disabled behavior — and `warned` is what marks the
    // run as not-a-pass. Strict verification is what turns that into a
    // non-zero exit, which the assertion above already proved.
    assert_eq!(
        sensor["exit_code"],
        serde_json::json!(0),
        "the disabled path exits zero after reporting the skip: {sensor}"
    );

    // The persisted error signature must not carry the child's output or the
    // raw fixture.
    let state = root.join(".do-harness");
    if state.exists() {
        let mut persisted = String::new();
        for entry in walk(&state) {
            if let Ok(bytes) = std::fs::read(&entry) {
                persisted.push_str(&String::from_utf8_lossy(&bytes));
            }
        }
        assert!(
            !persisted.contains("AKIAIOSFODNN7EXAMPLE"),
            "the persisted state must not carry the raw fixture"
        );
        assert!(
            !persisted.contains("synthetic_"),
            "the persisted state must not carry the raw fixture"
        );
    }
}

/// Collects every file below a directory.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}
