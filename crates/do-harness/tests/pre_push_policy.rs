//! Integration tests verifying that the repository's pre-push hook policy is
//! bounded to the verification set and excludes scheduled/release sensors (WF-07).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

mod support;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn harness() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    support::isolate_command(&mut cmd);
    cmd
}

#[test]
fn do_harness_toml_pre_push_matches_verification_set_and_excludes_scheduled() {
    let root = workspace_root();
    let config_path = root.join("do-harness.toml");
    let content = std::fs::read_to_string(&config_path).expect("read do-harness.toml");
    let value: toml::Value = toml::from_str(&content).expect("parse toml");

    let hooks = value.get("hooks").expect("hooks table");
    let pre_push = hooks
        .get("pre-push")
        .expect("pre-push array")
        .as_array()
        .expect("pre-push is array");

    let pre_push_sensors: Vec<&str> = pre_push.iter().map(|v| v.as_str().unwrap()).collect();

    assert!(
        !pre_push_sensors.is_empty(),
        "pre-push must not be empty; empty means full suite including heavy scheduled coverage"
    );

    let signal_sets = value.get("signal-sets").expect("signal-sets table");
    let verification = signal_sets
        .get("verification")
        .expect("verification set")
        .as_array()
        .expect("verification is array");
    let verification_sensors: Vec<&str> =
        verification.iter().map(|v| v.as_str().unwrap()).collect();

    assert_eq!(
        pre_push_sensors, verification_sensors,
        "pre-push hook sensor list must match signal-sets.verification"
    );

    // Scheduled and heavy release-only checks must not be in pre-push
    assert!(
        !pre_push_sensors.contains(&"coverage"),
        "pre-push must exclude heavy coverage sensor to avoid local push OOM"
    );
    assert!(
        !pre_push_sensors.contains(&"powerset"),
        "pre-push must exclude heavy powerset builds"
    );
    assert!(
        !pre_push_sensors.contains(&"release-manager-e2e"),
        "pre-push must exclude release-manager-e2e"
    );
    assert!(
        !pre_push_sensors.contains(&"publish-npm"),
        "pre-push must exclude publish-npm"
    );
}

#[test]
fn hook_install_renders_pre_push_with_bounded_only_flags() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    support::git_command(root)
        .args(["init", "-q"])
        .output()
        .expect("git init");
    support::git_command(root)
        .args(["config", "core.hooksPath", ".git/hooks"])
        .output()
        .expect("config hookspath");

    // Copy repository do-harness.toml to test fixture
    let repo_config = workspace_root().join("do-harness.toml");
    std::fs::copy(repo_config, root.join("do-harness.toml")).expect("copy config");

    let out = harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("hook")
        .arg("install")
        .output()
        .expect("hook install");
    assert!(
        out.status.success(),
        "hook install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let pre_push_path = root.join(".git/hooks/pre-push");
    let script = std::fs::read_to_string(pre_push_path).expect("read installed pre-push");

    assert!(
        script.contains("--only fmt"),
        "installed pre-push must carry --only flags"
    );
    assert!(
        script.contains("--only check"),
        "installed pre-push must carry --only check"
    );
    assert!(
        !script.contains("--only coverage"),
        "installed pre-push must not contain coverage"
    );
    assert!(
        !script.contains("--only powerset"),
        "installed pre-push must not contain powerset"
    );
}
