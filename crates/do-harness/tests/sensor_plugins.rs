//! Acceptance fixtures for the sensor plugin protocol (issue #575): a plugin
//! manifest under `plugins/` is discovered, `[[sensors]]` entries resolve to
//! it by name, and the resulting sensor behaves identically to an inline
//! sensor in `list`, `explain`, and `verify` — fail-closed on discovery and
//! resolution errors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use serde_json::Value;

mod support;

use support::{git, harness, run};

const RUFF_MANIFEST: &str = r#"
schema-version = 1
version = "1.2.0"
description = "Ruff lint gate"

[sensor]
name = "ruff"
argv = ["sh", "-c", "echo plugin-ran"]
when-changed = ["**/*.py"]
inputs = ["**/*.py", "pyproject.toml"]
severity = "error"
"#;

/// Creates a committed git repo with one plugin manifest and a config whose
/// `[[sensors]]` entry references it by name.
fn fixture_repo(manifest: &str, extra_sensors: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("plugins/ruff")).unwrap();
    std::fs::write(root.join("plugins/ruff/sensor-plugin.toml"), manifest).unwrap();
    let config = format!(
        "[signal-sets]\nverification = [\"ruff\"]\n\n[[sensors]]\nname = \"ruff\"\nplugin = \"ruff\"\n{extra_sensors}"
    );
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: plugin fixture"]);
    (dir, root)
}

/// Sensor results from a JSON verify report.
fn sensor_results(report: &Value) -> Vec<(&str, bool)> {
    report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .map(|s| {
            (
                s["name"].as_str().expect("sensor name"),
                s["ok"].as_bool().expect("sensor ok"),
            )
        })
        .collect()
}

#[test]
fn list_shows_plugin_sensors_with_provenance_in_text_only() {
    let (_dir, root) = fixture_repo(RUFF_MANIFEST, "");

    let (code, stdout, _) = run(harness(&root).arg("list"));
    assert_eq!(code, Some(0), "list must succeed");
    assert!(
        stdout.contains("ruff (plugin)"),
        "text list provenance: {stdout}"
    );

    let (code, stdout, _) = run(harness(&root).arg("list").arg("--format").arg("json"));
    assert_eq!(code, Some(0));
    let names: Vec<String> = serde_json::from_str(&stdout).expect("json list");
    assert_eq!(
        names,
        vec!["ruff".to_string()],
        "json list stays plain names"
    );
}

#[test]
fn explain_shows_plugin_sensors_with_provenance() {
    let (_dir, root) = fixture_repo(RUFF_MANIFEST, "");

    let (code, stdout, _) = run(harness(&root).arg("explain"));
    assert_eq!(code, Some(0), "explain must succeed");
    assert!(
        stdout.contains("plugin: ruff"),
        "explain text provenance: {stdout}"
    );

    let (code, stdout, _) = run(harness(&root).arg("explain").arg("--format").arg("json"));
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("json explain");
    let selected = report["selected"].as_array().expect("selected");
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0]["name"], "ruff");
    assert_eq!(selected[0]["plugin"], "ruff");
}

#[test]
fn plugin_sensor_runs_through_verify_and_passes() {
    let (_dir, root) = fixture_repo(RUFF_MANIFEST, "");

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--only")
        .arg("ruff")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "verify must pass: {stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json verify");
    let results = sensor_results(&report);
    assert_eq!(results, vec![("ruff", true)]);
}

#[test]
fn plugin_sensor_failure_fails_the_gate() {
    let failing = RUFF_MANIFEST.replace(
        "argv = [\"sh\", \"-c\", \"echo plugin-ran\"]",
        "argv = [\"sh\", \"-c\", \"echo plugin-failed; exit 1\"]",
    );
    let (_dir, root) = fixture_repo(&failing, "");

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--only")
        .arg("ruff")
        .arg("--format")
        .arg("json"));
    assert_eq!(
        code,
        Some(1),
        "failing plugin sensor must fail the gate: {stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("json verify");
    assert_eq!(sensor_results(&report), vec![("ruff", false)]);
}

#[test]
fn missing_plugin_reference_fails_closed_at_load() {
    let (_dir, root) = fixture_repo(RUFF_MANIFEST, "");
    std::fs::write(
        root.join("do-harness.toml"),
        "[[sensors]]\nname = \"lint\"\nplugin = \"nope\"\n",
    )
    .unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: missing plugin"]);

    for command in [&["list"][..], &["explain"][..], &["verify"][..]] {
        let (code, _, stderr) = run(harness(&root).args(command));
        assert_eq!(code, Some(2), "{command:?} must exit 2: {stderr}");
        assert!(
            stderr.contains("not installed"),
            "{command:?} must name the missing plugin: {stderr}"
        );
    }
}

#[test]
fn broken_manifest_fails_closed_at_load() {
    let bad = RUFF_MANIFEST.replace("severity = \"error\"", "bogus = true\nseverity = \"error\"");
    let (_dir, root) = fixture_repo(&bad, "");

    let (code, _, stderr) = run(harness(&root).arg("list"));
    assert_eq!(code, Some(2), "broken manifest must fail closed: {stderr}");
    assert!(
        stderr.contains("invalid plugin manifest"),
        "must name the broken manifest: {stderr}"
    );
}

#[test]
fn duplicate_plugin_names_fail_closed_at_load() {
    let (_dir, root) = fixture_repo(RUFF_MANIFEST, "");
    std::fs::create_dir_all(root.join("plugins/other")).unwrap();
    std::fs::write(root.join("plugins/other/sensor-plugin.toml"), RUFF_MANIFEST).unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: duplicate plugin"]);

    let (code, _, stderr) = run(harness(&root).arg("list"));
    assert_eq!(
        code,
        Some(2),
        "duplicate plugins must fail closed: {stderr}"
    );
    assert!(
        stderr.contains("two plugin manifests"),
        "must name the duplicate: {stderr}"
    );
}

#[test]
fn unsupported_manifest_schema_version_fails_closed() {
    let future = RUFF_MANIFEST.replace("schema-version = 1", "schema-version = 2");
    let (_dir, root) = fixture_repo(&future, "");

    let (code, _, stderr) = run(harness(&root).arg("list"));
    assert_eq!(code, Some(2), "future schema must fail closed: {stderr}");
    assert!(
        stderr.contains("schema version"),
        "must name the schema version: {stderr}"
    );
}
