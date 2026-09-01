//! Integration tests for the `do-harness compliance` command.

use std::process::Command;

#[test]
fn compliance_command_text() {
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("compliance")
        .output()
        .expect("failed to execute do-harness compliance");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# Compliance mapping"));
    assert!(stdout.contains("OWASP Agentic Top 10"));
    assert!(stdout.contains("NIST AI Risk Management Framework"));
    assert!(stdout.contains("EU AI Act"));
}

#[test]
fn compliance_command_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .args(["compliance", "--format", "json"])
        .output()
        .expect("failed to execute do-harness compliance --format json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert!(v.get("doc").is_some());
    assert!(v["frameworks"].is_array());
}
