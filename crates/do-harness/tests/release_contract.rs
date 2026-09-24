//! Release contract integration tests: CLI commands, exit codes, single JSON stdout,
//! evidence v3/v4 backward compatibility, config backward compatibility, and fail-closed checks.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn harness_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_do-harness"))
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/release-contract")
}

fn run_cmd(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Creates a minimal workspace root with git repo, do-harness.toml, dummy Rust source, and plans/dora.json.
fn setup_workspace(dir: &Path) {
    let _ = Command::new("git").args(["init"]).current_dir(dir).output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .output();

    fs::create_dir_all(dir.join("src")).expect("src dir");
    fs::write(dir.join("src/lib.rs"), "// dummy\n").expect("write src/lib.rs");
    fs::create_dir_all(dir.join("plans")).expect("plans dir");
    fs::write(
        dir.join("plans/dora.json"),
        r#"{
  "window_days": 30,
  "tag_glob": "refs/tags/v*",
  "merge_strategy": "squash",
  "percentile_method": "nearest-rank",
  "revert_pattern": "^revert(\\(|:)",
  "bot_allowlist": [],
  "min_deploys": 1,
  "max_lead_p90_days": 30,
  "max_change_failure_rate": 0.15,
  "max_mttr_hours": 24,
  "max_unrestored": 0
}"#,
    )
    .expect("write plans/dora.json");
    fs::write(
        dir.join("do-harness.toml"),
        r#"
language = "rust"

[signal-sets]
verification = ["fmt"]

[[sensors]]
name = "fmt"
argv = ["true"]
"#,
    )
    .expect("write do-harness.toml in temp workspace");

    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "feat: initial commit"])
        .current_dir(dir)
        .output();
}

/// Verifies that every command in `cli-required.json` exists and supports required flags/options.
#[test]
fn test_cli_commands_and_required_options() {
    let spec_path = fixture_dir().join("cli-required.json");
    let content = fs::read_to_string(&spec_path).expect("read cli-required.json");
    let json: Value = serde_json::from_str(&content).expect("parse cli-required.json");
    let commands = json["commands"]
        .as_object()
        .expect("commands map in cli-required.json");

    for (cmd_name, req_flags) in commands {
        let (code, stdout, stderr) = run_cmd(harness_bin().args([cmd_name.as_str(), "--help"]));
        assert_eq!(
            code,
            Some(0),
            "Command '{cmd_name} --help' failed with stderr:\n{stderr}"
        );

        let help_text = format!("{stdout}\n{stderr}");
        let flags = req_flags.as_array().expect("array of flags");
        for flag in flags {
            let flag_str = flag.as_str().expect("string flag");
            let is_positional = matches!(flag_str, "action" | "shell" | "dir" | "file" | "paths");
            if is_positional {
                let upper = flag_str.to_uppercase();
                let singular_upper = upper.trim_end_matches('S');
                assert!(
                    help_text.contains(&upper)
                        || help_text.contains(singular_upper)
                        || help_text.contains("Commands")
                        || help_text.contains("COMMAND"),
                    "Help for '{cmd_name}' must reference positional or subcommand placeholder for '{flag_str}', got:\n{help_text}"
                );
            } else {
                let kebab = flag_str.replace('_', "-");
                let expected_flag = format!("--{kebab}");
                assert!(
                    help_text.contains(&expected_flag),
                    "Help for '{cmd_name}' must contain option '{expected_flag}', got:\n{help_text}"
                );
            }
        }
    }
}

/// Verifies exit codes classification:
/// 0: Success
/// 1: Verification/check failure
/// 2: Usage, configuration, or discovery error
#[test]
fn test_exit_code_classification() {
    let tmp = tempfile::tempdir().unwrap();
    setup_workspace(tmp.path());

    // Exit code 0: Success
    let (code_0, stdout_0, stderr_0) = run_cmd(harness_bin().args(["version", "--format", "json"]));
    assert_eq!(
        code_0,
        Some(0),
        "Success command must exit 0:\nstdout: {stdout_0}\nstderr: {stderr_0}"
    );

    // Exit code 1: Verification or check failure (doctor --strict in workspace with strict issues)
    let target_dir = tmp.path().join("target");
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(target_dir.join("artifact"), "dummy").unwrap();

    let (code_1, stdout_1, stderr_1) = run_cmd(
        harness_bin()
            .arg("--root")
            .arg(tmp.path())
            .args(["doctor", "--strict", "--format", "json"]),
    );
    assert_eq!(
        code_1,
        Some(1),
        "Verification/check failure must exit 1:\nstdout: {stdout_1}\nstderr: {stderr_1}"
    );

    // Exit code 2: Usage error (e.g. invalid flag)
    let (code_2, stdout_2, stderr_2) = run_cmd(harness_bin().args(["--invalid-flag-nonexistent"]));
    assert_eq!(
        code_2,
        Some(2),
        "Usage error must exit 2:\nstdout: {stdout_2}\nstderr: {stderr_2}"
    );

    // Exit code 2: Config error (e.g. nonexistent config file)
    let (code_2_cfg, stdout_2_cfg, stderr_2_cfg) =
        run_cmd(harness_bin().arg("--root").arg(tmp.path()).args([
            "verify",
            "--config",
            "nonexistent-file.toml",
        ]));
    assert_eq!(
        code_2_cfg,
        Some(2),
        "Config error must exit 2:\nstdout: {stdout_2_cfg}\nstderr: {stderr_2_cfg}"
    );
}

/// Verifies that every JSON mode emits exactly one JSON value on stdout and diagnostics on stderr.
#[test]
fn test_single_json_value_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    setup_workspace(tmp.path());

    let json_commands: &[&[&str]] = &[
        &["version", "--format", "json"],
        &["list", "--format", "json"],
        &["compliance", "--format", "json"],
        &["explain", "--format", "json"],
        &["loc", "--format", "json"],
        &["audit-chain", "--format", "json"],
        &["dora", "--format", "json"],
    ];

    for cmd_args in json_commands {
        let (code, stdout, stderr) =
            run_cmd(harness_bin().arg("--root").arg(tmp.path()).args(*cmd_args));
        assert!(
            code == Some(0) || code == Some(1),
            "Command {cmd_args:?} exited with code {code:?}\nstderr:\n{stderr}"
        );

        let trimmed = stdout.trim();
        assert!(
            !trimmed.is_empty(),
            "Command {cmd_args:?} JSON mode emitted empty stdout"
        );

        let mut stream = serde_json::Deserializer::from_str(trimmed).into_iter::<Value>();
        let first = stream.next();
        assert!(
            first.is_some() && first.as_ref().unwrap().is_ok(),
            "Command {cmd_args:?} stdout is not a valid JSON value: {trimmed}"
        );
        assert!(
            stream.next().is_none(),
            "Command {cmd_args:?} stdout emitted multiple JSON values on stdout"
        );
    }
}

/// Verifies that committed evidence v3 and v4 fixtures deserialize successfully,
/// and evidence below `MIN_CURRENT_SCHEMA_VERSION` is rejected as legacy/stale.
#[test]
fn test_evidence_v3_and_v4_compatibility() {
    let tmp = tempfile::tempdir().unwrap();
    setup_workspace(tmp.path());

    let v3_fixture = fixture_dir().join("evidence-v3.json");
    let v4_fixture = fixture_dir().join("evidence-v4.json");

    assert!(v3_fixture.exists(), "evidence-v3.json fixture must exist");
    assert!(v4_fixture.exists(), "evidence-v4.json fixture must exist");

    let (code_v3, stdout_v3, stderr_v3) =
        run_cmd(harness_bin().arg("--root").arg(tmp.path()).args([
            "status",
            "--evidence",
            v3_fixture.to_str().unwrap(),
            "--format",
            "json",
        ]));
    assert!(
        code_v3 == Some(0) || code_v3 == Some(1),
        "v3 evidence read status code: {code_v3:?}\nstderr: {stderr_v3}"
    );
    let val_v3: Value = serde_json::from_str(&stdout_v3).expect("status json for v3 evidence");
    assert_ne!(val_v3["state"], serde_json::json!("missing"));

    let (code_v4, stdout_v4, stderr_v4) =
        run_cmd(harness_bin().arg("--root").arg(tmp.path()).args([
            "status",
            "--evidence",
            v4_fixture.to_str().unwrap(),
            "--format",
            "json",
        ]));
    assert!(
        code_v4 == Some(0) || code_v4 == Some(1),
        "v4 evidence read status code: {code_v4:?}\nstderr: {stderr_v4}"
    );
    let val_v4: Value = serde_json::from_str(&stdout_v4).expect("status json for v4 evidence");
    assert_ne!(val_v4["state"], serde_json::json!("missing"));

    let v2_file = tmp.path().join("evidence-v2.json");
    fs::write(
        &v2_file,
        r#"{
            "schema_version": 2,
            "tool": "do-harness",
            "harness_version": "0.0.1",
            "started_at": 1000,
            "finished_at": 1005,
            "root": "/tmp",
            "sensors": [],
            "summary": {"pass":0,"fail":0,"skip":0,"verdict":"pass"}
        }"#,
    )
    .unwrap();

    let (_code_v2, stdout_v2, _stderr_v2) =
        run_cmd(harness_bin().arg("--root").arg(tmp.path()).args([
            "status",
            "--evidence",
            v2_file.to_str().unwrap(),
            "--format",
            "json",
        ]));
    let val_v2: Value = serde_json::from_str(&stdout_v2).expect("status json for v2 evidence");
    assert!(
        val_v2["state"] == serde_json::json!("stale")
            || (val_v2["state"] == serde_json::json!("missing")
                && val_v2["reason"] == serde_json::json!("legacy_schema")),
        "Schema version 2 evidence must be stale or report legacy_schema, got:\n{stdout_v2}"
    );
}

/// Verifies that committed previous-release config fixtures parse successfully,
/// and unknown configuration fields fail closed.
#[test]
fn test_config_fixtures_and_fail_closed() {
    let tmp = tempfile::tempdir().unwrap();
    setup_workspace(tmp.path());

    let min_cfg = fixture_dir().join("config-v0.1-minimal.toml");
    let full_cfg = fixture_dir().join("config-v0.1-full.toml");

    assert!(min_cfg.exists(), "config-v0.1-minimal.toml must exist");
    assert!(full_cfg.exists(), "config-v0.1-full.toml must exist");

    let (code_min, stdout_min, stderr_min) =
        run_cmd(harness_bin().arg("--root").arg(tmp.path()).args([
            "list",
            "--config",
            min_cfg.to_str().unwrap(),
            "--format",
            "json",
        ]));
    assert_eq!(
        code_min,
        Some(0),
        "Minimal config failed:\nstdout: {stdout_min}\nstderr: {stderr_min}"
    );

    let (code_full, stdout_full, stderr_full) =
        run_cmd(harness_bin().arg("--root").arg(tmp.path()).args([
            "list",
            "--config",
            full_cfg.to_str().unwrap(),
            "--format",
            "json",
        ]));
    assert_eq!(
        code_full,
        Some(0),
        "Full config failed:\nstdout: {stdout_full}\nstderr: {stderr_full}"
    );

    let unknown_cfg = tmp.path().join("unknown-field-config.toml");
    fs::write(
        &unknown_cfg,
        r#"
language = "rust"
unrecognized_field_abc = "invalid_value"
"#,
    )
    .unwrap();

    let (code_bad, stdout_bad, stderr_bad) =
        run_cmd(harness_bin().arg("--root").arg(tmp.path()).args([
            "list",
            "--config",
            unknown_cfg.to_str().unwrap(),
            "--format",
            "json",
        ]));
    assert_eq!(
        code_bad,
        Some(2),
        "Unknown config field must fail closed with exit code 2:\nstdout: {stdout_bad}\nstderr: {stderr_bad}"
    );
    assert!(
        stderr_bad.contains("unknown field") || stderr_bad.contains("invalid config file"),
        "stderr should mention config error, got:\n{stderr_bad}"
    );
}
