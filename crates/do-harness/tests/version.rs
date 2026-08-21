//! Integration tests for version command and flags.

use std::process::Command;

use serde_json::Value;

fn harness_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_do-harness"))
}

fn run_cmd(cmd: &mut Command) -> (bool, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn version_flags_and_subcommand_match() {
    let (ok1, out1) = run_cmd(harness_bin().arg("-V"));
    assert!(ok1, "do-harness -V failed");

    let (ok2, out2) = run_cmd(harness_bin().arg("--version"));
    assert!(ok2, "do-harness --version failed");

    let (ok3, out3) = run_cmd(harness_bin().arg("version"));
    assert!(ok3, "do-harness version failed");

    assert_eq!(out1, out2, "-V and --version stdout must match");
    assert_eq!(
        out2, out3,
        "--version and version subcommand stdout must match"
    );
    assert!(
        out1.starts_with("do-harness 0.1.0"),
        "version output should start with 'do-harness 0.1.0', got:\n{out1}"
    );
}

#[test]
fn version_json_format() {
    let (ok, stdout) = run_cmd(harness_bin().arg("version").arg("--format").arg("json"));
    assert!(ok, "do-harness version --format json failed:\n{stdout}");

    let value: Value = serde_json::from_str(&stdout).expect("version json is valid JSON");
    assert_eq!(value["name"], serde_json::json!("do-harness"));
    assert_eq!(value["version"], serde_json::json!("0.1.0"));
    assert!(value.get("commit").is_some());
    assert!(value.get("commit_date").is_some());
    assert!(value.get("dirty").is_some());
}

#[test]
fn version_works_outside_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();

    let (ok1, out1) = run_cmd(harness_bin().current_dir(temp_dir.path()).arg("--version"));
    assert!(ok1, "--version outside workspace failed");

    let (ok2, out2) = run_cmd(harness_bin().current_dir(temp_dir.path()).arg("version"));
    assert!(ok2, "version subcommand outside workspace failed");

    assert_eq!(out1, out2);

    let (ok3, json_out) = run_cmd(
        harness_bin()
            .current_dir(temp_dir.path())
            .arg("version")
            .arg("--format")
            .arg("json"),
    );
    assert!(ok3, "version --format json outside workspace failed");

    let value: Value = serde_json::from_str(&json_out).expect("version json is valid JSON");
    assert_eq!(value["name"], serde_json::json!("do-harness"));
    assert_eq!(value["version"], serde_json::json!("0.1.0"));
}
