//! Integration tests for non-executing repository inspection (`init --inspect`) (WF-11).

#![allow(clippy::unwrap_used, clippy::expect_used)]
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXEC_PERMS: u32 = 0o755;
const EXPECTED_SENTINEL_EXIT_CODE: i32 = 99;

struct SentinelEnv {
    _bin_dir: tempfile::TempDir,
    path_env: OsString,
    tripwire: PathBuf,
}

impl SentinelEnv {
    fn new() -> Self {
        let bin_dir = tempfile::tempdir().unwrap();
        let tripwire = bin_dir.path().join("tripwire.fired");
        for tool in ["yarn", "npm", "pnpm", "node", "cargo", "python3"] {
            let script = bin_dir.path().join(tool);
            let content = format!(
                "#!/bin/sh\necho 'SENTINEL EXECUTED: {tool}' >&2\ntouch '{}'\nexit {EXPECTED_SENTINEL_EXIT_CODE}\n",
                tripwire.display()
            );
            fs::write(&script, &content).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&script).unwrap().permissions();
                perms.set_mode(EXEC_PERMS);
                fs::set_permissions(&script, perms).unwrap();
            }
            #[cfg(windows)]
            {
                let cmd_shim = bin_dir.path().join(format!("{tool}.cmd"));
                let content = format!(
                    "@echo off\r\necho SENTINEL EXECUTED: {tool} >&2\r\ntype nul > \"{}\"\r\nexit /b {EXPECTED_SENTINEL_EXIT_CODE}\r\n",
                    tripwire.display()
                );
                fs::write(&cmd_shim, content).unwrap();
            }
        }
        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let path_env = std::env::join_paths(
            std::iter::once(bin_dir.path().to_path_buf()).chain(std::env::split_paths(&old_path)),
        )
        .expect("join_paths");
        Self {
            _bin_dir: bin_dir,
            path_env,
            tripwire,
        }
    }

    fn assert_not_executed(&self, label: &str) {
        assert!(
            !self.tripwire.exists(),
            "Sentinel was executed during non-executing inspection ({label})!"
        );
    }
}
#[test]
fn test_sentinel_tripwire_positive_control_fires_on_execution() {
    let sentinels = SentinelEnv::new();
    assert!(!sentinels.tripwire.exists());

    #[cfg(unix)]
    let output = Command::new("cargo")
        .env("PATH", &sentinels.path_env)
        .output()
        .expect("spawn sentinel tool");

    #[cfg(windows)]
    let output = Command::new("cmd")
        .args(["/c", "cargo"])
        .env("PATH", &sentinels.path_env)
        .output()
        .expect("spawn sentinel tool");

    assert_eq!(output.status.code(), Some(EXPECTED_SENTINEL_EXIT_CODE));
    assert!(
        sentinels.tripwire.exists(),
        "positive control failed: sentinel did not create tripwire"
    );
}

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}
#[test]
fn test_node_fixture_yields_actionable_candidates_without_execution() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let pkg_json = r#"{
  "name": "sample-node-app",
  "version": "1.0.0",
  "scripts": {
    "test": "vitest run",
    "lint": "eslint ."
  }
}"#;
    fs::write(root.join("package.json"), pkg_json).unwrap();

    let sentinels = SentinelEnv::new();

    let output = harness(root)
        .env("PATH", &sentinels.path_env)
        .args(["init", "--inspect", "--format", "json"])
        .output()
        .expect("run init --inspect");

    assert!(
        output.status.success(),
        "init --inspect failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: Value = serde_json::from_slice(&output.stdout).expect("parse json report");
    assert_eq!(parsed["resolved_language"], "generic");
    let detected = parsed["detected"].as_array().expect("detected facts");
    assert!(
        detected
            .iter()
            .any(|f| f.as_str().unwrap().contains("package.json"))
    );

    let commands = parsed["candidate_commands"]
        .as_array()
        .expect("candidate commands");
    assert!(
        commands.iter().any(|c| c["name"] == "test"),
        "candidate commands should include 'test': {commands:?}"
    );
    assert!(
        commands.iter().any(|c| c["name"] == "lint"),
        "candidate commands should include 'lint': {commands:?}"
    );

    let guidance = parsed["guidance"].as_str().expect("guidance string");
    assert!(
        guidance.contains("--language node"),
        "guidance must advise using --language node: {guidance}"
    );

    // Invariant: non-executing inspection must write ZERO files
    assert!(
        !root.join("do-harness.toml").exists(),
        "must not write do-harness.toml"
    );
    assert!(!root.join("AGENTS.md").exists(), "must not write AGENTS.md");
    assert!(
        !root.join(".do-harness").exists(),
        "must not create .do-harness"
    );
    sentinels.assert_not_executed("default node fixture inspection");
}

#[test]
fn test_explicit_language_node_inspection() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let pkg_json = r#"{
  "name": "sample-node-app",
  "version": "1.0.0",
  "scripts": {
    "test": "node --test"
  }
}"#;
    fs::write(root.join("package.json"), pkg_json).unwrap();

    let sentinels = SentinelEnv::new();

    let output = harness(root)
        .env("PATH", &sentinels.path_env)
        .args([
            "init",
            "--inspect",
            "--language",
            "node",
            "--format",
            "json",
        ])
        .output()
        .expect("run init --inspect");

    assert!(output.status.success());
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["resolved_language"], "node");
    assert!(!root.join("do-harness.toml").exists());
    sentinels.assert_not_executed("explicit --language node inspection");
}

#[test]
fn test_python_and_go_fixtures_remain_honestly_unconfigured() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    fs::write(root.join("requirements.txt"), "pytest>=7.0.0\n").unwrap();
    fs::write(root.join("go.mod"), "module example.com/demo\n\ngo 1.22\n").unwrap();

    let output = harness(root)
        .args(["init", "--inspect", "--format", "json"])
        .output()
        .expect("run init --inspect");

    assert!(output.status.success());
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["resolved_language"], "generic");

    let candidates = parsed["candidates"].as_array().unwrap();
    assert!(
        candidates.is_empty(),
        "generic pack must not infer untrusted sensor commands for Python/Go without explicit policy"
    );

    let guidance = parsed["guidance"].as_str().unwrap();
    assert!(guidance.contains("do-harness.toml") || guidance.contains("[[sensors]]"));

    assert!(!root.join("do-harness.toml").exists());
}

#[test]
fn test_rust_inspection() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let output = harness(root)
        .args(["init", "--inspect", "--format", "json"])
        .output()
        .expect("run init --inspect");

    assert!(output.status.success());
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["resolved_language"], "rust");

    let candidates = parsed["candidates"].as_array().unwrap();
    assert!(candidates.iter().any(|c| c["name"] == "check"));
    assert!(!root.join("do-harness.toml").exists());
}
