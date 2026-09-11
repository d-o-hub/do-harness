//! Acceptance fixtures for evidence freshness (`status`).
//!
//! These drive the real `do-harness` binary through the full behavioral
//! contract: `green -> edit -> stale -> verify -> green`, plus config
//! freshness, red/missing states, and the cheap-status guarantee.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

mod support;

use support::git_command;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs a git command inside `root`, asserting success.
fn git(root: &Path, args: &[&str]) {
    let status = git_command(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .expect("spawn git");
    assert!(
        status.success(),
        "git {args:?} failed in {}",
        root.display()
    );
}

/// Creates a committed git repo with fast sensors and a verification set.
fn fixture_repo() -> (tempfile::TempDir, PathBuf) {
    fixture_repo_with_sensors(
        &[("a", &["true"] as &[_])],
        &[("verification", &["a"] as &[_])],
    )
}

/// Creates a committed git repo with the given sensors and signal sets.
fn fixture_repo_with_sensors(
    sensors: &[(&str, &[&str])],
    sets: &[(&str, &[&str])],
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut config = String::from("[signal-sets]\n");
    for (set, names) in sets {
        use std::fmt::Write as _;
        let list = names
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        write!(config, "{set} = [{list}]").unwrap();
        config.push('\n');
    }
    for (name, argv) in sensors {
        use std::fmt::Write as _;
        let args = argv
            .iter()
            .map(|a| format!("\"{}\"", a.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            config,
            "\n[[sensors]]\nname = \"{name}\"\nargv = [{args}]\n"
        )
        .unwrap();
    }
    std::fs::write(root.join("do-harness.toml"), &config).unwrap();
    std::fs::write(root.join("lib.rs"), "v1\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);
    (dir, root)
}

/// Runs `verify --set <set> --format json`, asserting the exit code.
fn verify(root: &Path, set: &str, expect_code: i32) -> Value {
    verify_with_env(root, set, expect_code, &[])
}

/// Runs `verify` with extra environment variables for the sensor processes.
fn verify_with_env(root: &Path, set: &str, expect_code: i32, envs: &[(&str, &str)]) -> Value {
    let mut cmd = harness(root);
    cmd.arg("verify")
        .arg("--set")
        .arg(set)
        .arg("--format")
        .arg("json");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(
        code,
        Some(expect_code),
        "verify --set {set} exited {code:?}:\n{stdout}\n{stderr}"
    );
    serde_json::from_str(&stdout).expect("json report")
}

/// Runs `status --set <set> --format json`, returning (exit code, document).
fn status(root: &Path, set: &str) -> (Option<i32>, Value) {
    let (code, stdout, stderr) = run(harness(root)
        .arg("status")
        .arg("--set")
        .arg(set)
        .arg("--format")
        .arg("json"));
    (
        code,
        serde_json::from_str(&stdout)
            .unwrap_or_else(|_| panic!("status stdout is not JSON:\n{stdout}\n{stderr}")),
    )
}

/// The central contract: green -> edit -> stale -> verify -> green.
#[test]
fn workspace_edit_cycles_green_stale_green() {
    let (_dir, root) = fixture_repo();

    verify(&root, "verification", 0);
    let (code, doc) = status(&root, "verification");
    assert_eq!(code, Some(0), "fresh verification must be green: {doc}");
    assert_eq!(doc["state"], serde_json::json!("green"));

    std::fs::write(root.join("lib.rs"), "v2\n").unwrap();
    let (code, doc) = status(&root, "verification");
    assert_eq!(code, Some(1), "edited workspace must not be green: {doc}");
    assert_eq!(doc["state"], serde_json::json!("stale"));
    assert_eq!(doc["reason"], serde_json::json!("workspace_changed"));

    verify(&root, "verification", 0);
    let (code, doc) = status(&root, "verification");
    assert_eq!(code, Some(0), "re-verified workspace must be green: {doc}");
    assert_eq!(doc["state"], serde_json::json!("green"));
}

/// Changing `do-harness.toml` after verification invalidates the evidence.
///
/// The change is committed so the working tree is clean: only the policy
/// fingerprint differs, proving policy freshness is tracked independently.
#[test]
fn config_change_invalidates_evidence() {
    let (_dir, root) = fixture_repo();

    verify(&root, "verification", 0);
    assert_eq!(
        status(&root, "verification").1["state"],
        serde_json::json!("green")
    );

    let mut config = std::fs::read_to_string(root.join("do-harness.toml")).unwrap();
    config.push_str("\n# policy comment changes the policy fingerprint\n");
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: policy tweak"]);

    let (code, doc) = status(&root, "verification");
    assert_eq!(code, Some(1));
    assert_eq!(doc["state"], serde_json::json!("stale"));
    assert_eq!(doc["reason"], serde_json::json!("policy_changed"));
}

/// A relevant untracked file changes the workspace fingerprint.
#[test]
fn untracked_file_invalidates_evidence() {
    let (_dir, root) = fixture_repo();

    verify(&root, "verification", 0);
    assert_eq!(
        status(&root, "verification").1["state"],
        serde_json::json!("green")
    );

    std::fs::write(root.join("scratch.rs"), "untracked\n").unwrap();
    let (code, doc) = status(&root, "verification");
    assert_eq!(code, Some(1));
    assert_eq!(doc["state"], serde_json::json!("stale"));
    assert_eq!(doc["reason"], serde_json::json!("workspace_changed"));
}

/// Failing verification surfaces as red with the failed sensors listed.
#[test]
fn failing_verification_reports_red() {
    let (_dir, root) = fixture_repo_with_sensors(
        &[("bad", &["false"] as &[_])],
        &[("verification", &["bad"])],
    );

    verify(&root, "verification", 1);
    let (code, doc) = status(&root, "verification");
    assert_eq!(code, Some(1));
    assert_eq!(doc["state"], serde_json::json!("red"));
    assert_eq!(doc["evidence"]["failed"], serde_json::json!(["bad"]));
}

/// No evidence at all reports missing.
#[test]
fn no_evidence_reports_missing() {
    let (_dir, root) = fixture_repo();

    let (code, doc) = status(&root, "verification");
    assert_eq!(code, Some(1));
    assert_eq!(doc["state"], serde_json::json!("missing"));
}

/// `status` never executes sensors: a probe-writing sensor must not run.
///
/// The probe file lives outside the repository so the sensor has no
/// working-tree side effect that could disturb the fingerprint.
#[test]
fn status_does_not_execute_sensors() {
    let probed = tempfile::tempdir().unwrap();
    let probe = probed.path().join("probe").display().to_string();
    let (_dir, root) = fixture_repo_with_sensors(
        &[(
            "probe-writer",
            &["sh", "-c", "touch \"$STATUS_PROBE_FILE\""] as &[_],
        )],
        &[("verification", &["probe-writer"])],
    );

    verify_with_env(&root, "verification", 0, &[("STATUS_PROBE_FILE", &probe)]);
    assert!(
        probed.path().join("probe").exists(),
        "verify must run the sensor"
    );
    std::fs::remove_file(probed.path().join("probe")).unwrap();

    let (code, doc) = status(&root, "verification");
    assert_eq!(code, Some(0), "status must be green: {doc}");
    assert!(
        !probed.path().join("probe").exists(),
        "status must not run sensors"
    );
}

/// Status is deterministic across repeated invocations without changes.
#[test]
fn status_is_deterministic() {
    let (_dir, root) = fixture_repo();

    verify(&root, "verification", 0);
    let (_, first) = status(&root, "verification");
    let (_, second) = status(&root, "verification");
    assert_eq!(first, second, "status output must be deterministic");
}

/// Evidence carries workspace/policy fingerprints and schema v3.
#[test]
fn evidence_carries_fingerprints_and_schema_v3() {
    let (_dir, root) = fixture_repo();

    verify(&root, "verification", 0);
    let path = root.join(".do-harness/evidence.verification.json");
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).expect("evidence json");
    assert_eq!(doc["schema_version"], serde_json::json!(3));
    assert_eq!(doc["signal_set"], serde_json::json!("verification"));
    for field in [
        "workspace_fingerprint",
        "policy_fingerprint",
        "config_fingerprint",
    ] {
        assert!(
            doc[field]
                .as_str()
                .is_some_and(|v| v.starts_with("sha256:") && v.len() > 7),
            "evidence must carry {field}: {doc}"
        );
    }
    assert!(doc["chain_hash"].as_str().is_some_and(|h| !h.is_empty()));
}
