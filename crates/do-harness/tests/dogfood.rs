//! CLI dogfood for greenfield adoption: the real binary must computationally
//! prove the `init` -> `verify` outcomes — never assume them.
//!
//! - rust: `init` scaffolds a minimal crate, so the full sensor suite runs and
//!   exits 0 on a truly empty tree.

#![allow(clippy::unwrap_used, clippy::expect_used)]
//! - red: removing the crate must flip `verify` to a non-zero exit with named
//!   failing sensors (proves the sensors actually execute).
//! - generic: zero sensors means `verify` exits 0 without running any command
//!   — a documented vacuous pass, asserted as such.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (success, stdout).
fn run(cmd: &mut Command) -> (bool, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// Runs `verify --format json` (which must exit 0) and parses the report.
fn verify_json(root: &Path) -> Value {
    let (ok, stdout) = run(harness(root).arg("verify").arg("--format").arg("json"));
    assert!(ok, "verify exited non-zero:\n{stdout}");
    serde_json::from_str(&stdout).expect("verify stdout is one JSON object")
}

#[test]
fn rust_init_then_full_verify_is_green() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let report = verify_json(dir.path());
    assert_eq!(
        report["ok"],
        serde_json::json!(true),
        "greenfield rust init + verify must be green, not assumed"
    );
    let sensors: Vec<&str> = report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .map(|s| s["name"].as_str().expect("sensor name"))
        .collect();
    assert!(
        !sensors.is_empty(),
        "sensors must actually run; a green report with no sensors is vacuous"
    );
    for want in ["fmt", "check", "clippy", "test", "loc", "commitlint"] {
        assert!(
            sensors.contains(&want),
            "missing sensor {want} in {sensors:?}"
        );
        let ran = report["sensors"]
            .as_array()
            .expect("sensors array")
            .iter()
            .find(|s| s["name"] == want)
            .expect("sensor entry");
        assert_eq!(ran["ok"], serde_json::json!(true), "sensor {want} failed");
        assert_eq!(
            ran["exit_code"],
            serde_json::json!(0),
            "sensor {want} exit code"
        );
    }
}

#[test]
fn rust_verify_fails_without_a_crate() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");
    std::fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
    std::fs::remove_dir_all(dir.path().join("src")).unwrap();

    let (ok, stdout) = run(harness(dir.path())
        .arg("verify")
        .arg("--format")
        .arg("json"));
    assert!(!ok, "verify must exit non-zero without a crate");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ok"], serde_json::json!(false));
    let failed = report["failed"].as_array().expect("failed array");
    assert!(
        !failed.is_empty(),
        "failed sensors must be listed, not assumed"
    );
    assert!(
        failed.iter().any(|name| name == "fmt"),
        "cargo sensor must fail without Cargo.toml: {failed:?}"
    );
}

#[test]
fn rust_verify_with_evidence_writes_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let evidence_path = dir.path().join("evidence.json");
    let (ok, _out) = run(harness(dir.path())
        .arg("verify")
        .arg("--evidence")
        .arg(&evidence_path)
        .arg("--strict"));
    assert!(ok, "verify --evidence --strict failed:\n{out}");
    assert!(evidence_path.exists(), "evidence file was not created");

    let text = std::fs::read_to_string(&evidence_path).unwrap();
    let doc: Value = serde_json::from_str(&text).expect("valid evidence JSON");
    assert_eq!(doc["schema_version"], serde_json::json!(1));
    assert_eq!(doc["tool"], serde_json::json!("do-harness"));
    assert_eq!(doc["summary"]["verdict"], serde_json::json!("pass"));
    assert_eq!(doc["summary"]["skip"], serde_json::json!(0));
}

#[test]
fn rust_verify_strict_fails_on_failing_sensor_and_writes_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");
    std::fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
    std::fs::remove_dir_all(dir.path().join("src")).unwrap();

    let evidence_path = dir.path().join("failing_evidence.json");
    let (ok, _out) = run(harness(dir.path())
        .arg("verify")
        .arg("--evidence")
        .arg(&evidence_path)
        .arg("--strict"));
    assert!(!ok, "verify --strict must fail on failing sensors");
    assert!(
        evidence_path.exists(),
        "failing run must still write evidence artifact"
    );

    let text = std::fs::read_to_string(&evidence_path).unwrap();
    let doc: Value = serde_json::from_str(&text).expect("valid evidence JSON");
    assert_eq!(doc["summary"]["verdict"], serde_json::json!("fail"));
}

#[test]
fn rust_verify_strict_default_path() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let (ok, out) = run(harness(dir.path()).arg("verify").arg("--strict"));
    assert!(ok, "verify --strict failed:\n{out}");

    let default_path = dir.path().join(".do-harness/evidence.json");
    assert!(
        default_path.exists(),
        "default evidence file .do-harness/evidence.json was not created"
    );
}

#[test]
fn generic_init_verify_is_vacuously_green() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path())
        .arg("init")
        .arg("--language")
        .arg("generic"));
    assert!(ok, "generic init failed:\n{out}");
    assert!(
        !dir.path().join("Cargo.toml").exists(),
        "generic pack must not scaffold a crate"
    );

    let report = verify_json(dir.path());
    assert_eq!(report["ok"], serde_json::json!(true));
    assert!(
        report["sensors"].as_array().expect("sensors").is_empty(),
        "generic pack ships zero sensors; the pass is vacuous by design"
    );
}
