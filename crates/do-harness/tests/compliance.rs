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
    assert!(stdout.contains("SOC 2"));
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
    assert_eq!(v["catalog_version"], 1);
    assert!(v["controls"].as_array().expect("controls array").len() >= 5);
}

#[test]
fn compliance_command_framework_filter() {
    let frameworks = [
        ("soc2", "soc2"),
        ("eu-ai-act", "eu-ai-act"),
        ("nist-ai-rmf", "nist-ai-rmf"),
        ("owasp-agentic-top10", "owasp-agentic-top10"),
    ];

    for (arg, slug) in frameworks {
        let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
            .args(["compliance", "--framework", arg, "--format", "json"])
            .output()
            .expect("failed to execute do-harness compliance --framework");

        assert!(output.status.success(), "failed for framework arg: {}", arg);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let v: serde_json::Value =
            serde_json::from_str(&stdout).expect("stdout should be valid json");
        assert_eq!(v["catalog_version"], 1);
        let controls = v["controls"].as_array().expect("controls array");
        assert!(!controls.is_empty());

        for control in controls {
            let mappings = control["frameworks"].as_array().expect("mappings array");
            assert!(
                mappings.iter().any(|m| m["framework"] == slug),
                "control missing expected framework slug {slug}"
            );
        }
    }
}

#[test]
fn compliance_command_invalid_framework_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .args(["compliance", "--framework", "invalid"])
        .output()
        .expect("failed to execute do-harness compliance --framework invalid");

    assert_eq!(output.status.code(), Some(2));
}
