//! Adoption contract integration tests for all supported harness packs and
//! fail-closed policy enforcement across edge cases.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

mod support;

use support::harness;

fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn git(root: &Path, args: &[&str]) {
    let status = support::git_command(root)
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

#[test]
fn node_pack_initialization_preserves_sources_and_verifies() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let package_json = r#"{
  "name": "fixture-node",
  "version": "1.0.0",
  "scripts": {
    "typecheck": "echo typecheck-ok",
    "lint": "echo lint-ok",
    "test": "echo test-ok",
    "build": "echo build-ok"
  }
}
"#;
    std::fs::write(root.join("package.json"), package_json).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/index.js"), "console.log('hello');\n").unwrap();

    let (code, stdout, stderr) = run(harness(root).args(["init", "--language", "node"]));
    assert_eq!(
        code,
        Some(0),
        "node init must succeed: stdout={stdout} stderr={stderr}"
    );

    assert_eq!(
        std::fs::read_to_string(root.join("package.json")).unwrap(),
        package_json,
        "init must preserve existing package.json"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/index.js")).unwrap(),
        "console.log('hello');\n",
        "init must preserve existing application source"
    );

    let config = std::fs::read_to_string(root.join("do-harness.toml")).unwrap();
    assert!(
        config.contains("language = \"node\""),
        "config must select node pack"
    );

    // Verification set runs typecheck, lint, test
    let (code, stdout, stderr) = run(harness(root).args([
        "verify",
        "--set",
        "verification",
        "--format",
        "json",
        "--strict",
    ]));
    assert_eq!(
        code,
        Some(0),
        "strict verify on node verification set must pass: stdout={stdout} stderr={stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("valid JSON report");
    assert_eq!(report["ok"], Value::Bool(true));

    // Release set runs typecheck, lint, test, build
    let (code, stdout, stderr) =
        run(harness(root).args(["verify", "--set", "release", "--format", "json", "--strict"]));
    assert_eq!(
        code,
        Some(0),
        "strict verify on node release set must pass: stdout={stdout} stderr={stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("valid JSON report");
    assert_eq!(report["ok"], Value::Bool(true));

    let sensor_names: Vec<&str> = report["sensors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    for expected in ["typecheck", "lint", "test", "build"] {
        assert!(
            sensor_names.contains(&expected),
            "missing sensor {expected} in {sensor_names:?}"
        );
    }
}

#[test]
fn web_pack_unconfigured_skips_and_fails_strict() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let (code, stdout, stderr) = run(harness(root).args(["init", "--language", "web"]));
    assert_eq!(
        code,
        Some(0),
        "web init must succeed: stdout={stdout} stderr={stderr}"
    );

    assert!(root.join("scripts/viewport-audit.mjs").is_file());
    assert!(root.join("scripts/a11y-audit.mjs").is_file());
    // A current failure needs established Git provenance; a non-repository
    // can only establish unknown/stale inputs under the canonical contract.
    git(root, &["init", "-q"]);

    // Without WEB_AUDIT_ROUTES, web sensors emit SKIP: which is recorded as warn.
    let (code, stdout, stderr) = run(harness(root).args([
        "verify",
        "--set",
        "verification",
        "--format",
        "json",
        "--strict",
    ]));
    assert_eq!(
        code,
        Some(1),
        "strict verify on unconfigured web pack must fail closed: stdout={stdout} stderr={stderr}"
    );

    let (status_code, stdout, stderr) =
        run(harness(root).args(["status", "--set", "verification", "--format", "json"]));
    assert_eq!(
        status_code,
        Some(1),
        "status must report failure for weak evidence: stdout={stdout} stderr={stderr}"
    );
    let status_doc: Value = serde_json::from_str(&stdout).expect("valid status JSON");
    assert_eq!(status_doc["state"], "red");
}

#[test]
fn generic_pack_vacuous_pass_is_distinguishable_from_strict_gate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let (code, stdout, stderr) = run(harness(root).args(["init", "--language", "generic"]));
    assert_eq!(
        code,
        Some(0),
        "generic init must succeed: stdout={stdout} stderr={stderr}"
    );

    let evidence_file = root.join("evidence.json");
    let (code, stdout, stderr) = run(harness(root).args([
        "verify",
        "--format",
        "json",
        "--strict",
        "--evidence",
        evidence_file.to_str().unwrap(),
    ]));
    assert_eq!(
        code,
        Some(0),
        "generic verify succeeds vacuously: stdout={stdout} stderr={stderr}"
    );

    let report: Value = serde_json::from_str(&stdout).expect("valid JSON report");
    assert_eq!(report["ok"], Value::Bool(true));
    assert_eq!(report["ok"], Value::Bool(code == Some(0)));
    assert!(
        report["sensors"].as_array().unwrap().is_empty(),
        "generic pack must report zero sensors"
    );

    let evidence_text = std::fs::read_to_string(&evidence_file).unwrap();
    let evidence_doc: Value = serde_json::from_str(&evidence_text).expect("valid evidence JSON");
    assert_eq!(evidence_doc["sensor_pack"], "generic");
    assert_eq!(evidence_doc["sensors"].as_array().unwrap().len(), 0);
}

#[test]
fn missing_declared_artifact_fails_strict() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let config = r#"
[signal-sets]
verification = ["builder"]

[[sensors]]
name = "builder"
argv = ["echo", "no-artifact-created"]
artifacts = ["dist/*.js"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "initial"]);

    let (code, stdout, stderr) = run(harness(root).args([
        "verify",
        "--set",
        "verification",
        "--strict",
        "--format",
        "json",
    ]));
    assert_eq!(
        code,
        Some(1),
        "missing declared artifact glob must fail strict verify: stdout={stdout} stderr={stderr}"
    );

    let (status_code, stdout, stderr) =
        run(harness(root).args(["status", "--set", "verification", "--format", "json"]));
    assert_eq!(
        status_code,
        Some(1),
        "status must report failure for weak evidence: stdout={stdout} stderr={stderr}"
    );
    let status_doc: Value = serde_json::from_str(&stdout).expect("valid status JSON");
    assert_eq!(status_doc["state"], "red");
}

#[test]
fn dangling_sensor_reference_and_invalid_config_fail_actionably() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. Dangling sensor in signal set
    let dangling_config = r#"
[signal-sets]
verification = ["nonexistent_sensor"]

[[sensors]]
name = "real_sensor"
argv = ["true"]
"#;
    std::fs::write(root.join("do-harness.toml"), dangling_config).unwrap();

    let (code, _stdout, stderr) = run(harness(root).args(["verify", "--set", "verification"]));
    assert_eq!(
        code,
        Some(2),
        "dangling sensor in set must produce exit code 2"
    );
    assert!(
        stderr.contains("nonexistent_sensor"),
        "stderr must name missing sensor: {stderr}"
    );

    // 2. Invalid syntax in do-harness.toml
    std::fs::write(root.join("do-harness.toml"), "invalid = toml [[ syntax").unwrap();
    let (code, _stdout, stderr) = run(harness(root).args(["verify"]));
    assert_eq!(code, Some(2), "invalid TOML must produce exit code 2");
    assert!(
        !stderr.is_empty(),
        "stderr must give actionable failure reason: {stderr}"
    );
}

#[test]
fn missing_tool_policy_under_require_tools() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let config = r#"
[signal-sets]
verification = ["missing_tool_sensor"]

[[sensors]]
name = "missing_tool_sensor"
argv = ["definitely_nonexistent_tool_12345"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "initial"]);

    // Local advisory run (execfail is caught as an error signature)
    let (code, stdout, stderr) =
        run(harness(root).args(["verify", "--set", "verification", "--format", "json"]));
    assert_eq!(
        code,
        Some(1),
        "non-existent tool must fail verify: stdout={stdout} stderr={stderr}"
    );

    // Under DO_HARNESS_REQUIRE_TOOLS=1
    let mut cmd = harness(root);
    cmd.args(["verify", "--set", "verification", "--format", "json"]);
    cmd.env("DO_HARNESS_REQUIRE_TOOLS", "1");
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(
        code,
        Some(1),
        "missing tool under DO_HARNESS_REQUIRE_TOOLS=1 must fail: stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn discovery_fails_closed_outside_git_selecting_all() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let config = r#"
[signal-sets]
verification = ["s1", "s2"]

[[sensors]]
name = "s1"
argv = ["true"]
when-changed = ["src/s1.rs"]

[[sensors]]
name = "s2"
argv = ["true"]
when-changed = ["src/s2.rs"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();

    // Outside git, change discovery fails, so verify --changed must select ALL sensors (fail closed)
    let (code, stdout, stderr) = run(harness(root).args([
        "verify",
        "--set",
        "verification",
        "--changed",
        "--format",
        "json",
    ]));
    assert_eq!(
        code,
        Some(0),
        "verify --changed outside git must succeed selecting all: stdout={stdout} stderr={stderr}"
    );

    let report: Value = serde_json::from_str(&stdout).expect("valid JSON report");
    let sensors: Vec<&str> = report["sensors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        sensors,
        vec!["s1", "s2"],
        "all sensors must be selected when change discovery fails"
    );
}

#[test]
fn deleted_file_matches_applicability_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let config = r#"
[signal-sets]
verification = ["checker"]

[[sensors]]
name = "checker"
argv = ["true"]
when-changed = ["src/**/*.rs"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn x() {}\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "initial"]);

    // Delete src/lib.rs
    std::fs::remove_file(root.join("src/lib.rs")).unwrap();

    let (code, stdout, stderr) = run(harness(root).args([
        "explain",
        "--set",
        "verification",
        "--changed",
        "--format",
        "json",
    ]));
    assert_eq!(
        code,
        Some(0),
        "explain must succeed: stdout={stdout} stderr={stderr}"
    );

    let doc: Value = serde_json::from_str(&stdout).expect("valid JSON explain doc");
    let selected: Vec<&str> = doc["selected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        selected,
        vec!["checker"],
        "deleted src/lib.rs must select checker sensor"
    );
}

#[test]
fn stale_and_malformed_evidence_rejected_by_status() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let config = r#"
[signal-sets]
verification = ["s1"]

[[sensors]]
name = "s1"
argv = ["true"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "initial"]);

    // 1. Missing evidence file
    let (code, _stdout, stderr) = run(harness(root).args(["status", "--set", "verification"]));
    assert_eq!(code, Some(1), "status with missing evidence must fail");
    assert!(
        stderr.contains("missing"),
        "stderr must report missing: {stderr}"
    );

    // 2. Malformed evidence file
    std::fs::create_dir_all(root.join(".do-harness")).unwrap();
    std::fs::write(
        root.join(".do-harness/evidence.verification.json"),
        "{ invalid json",
    )
    .unwrap();
    let (code, _stdout, stderr) = run(harness(root).args(["status", "--set", "verification"]));
    assert_eq!(code, Some(1), "status with malformed evidence must fail");
    assert!(
        stderr.contains("unreadable") || stderr.contains("missing") || stderr.contains("error"),
        "stderr must report unreadable/error: {stderr}"
    );
}
