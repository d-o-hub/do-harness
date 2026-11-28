//! CLI coverage for installed sensor manifests.

#![allow(clippy::unwrap_used)]

use std::path::Path;

mod support;

use support::{git, harness, run};

const PLUGIN_CONFIG: &str = r#"
language = "generic"
[signal-sets]
verification = ["lint"]
[[sensors]]
name = "lint"
plugin = "example.lint"
"#;

fn manifest(root: &Path, program: &str) {
    std::fs::write(
        root.join("sensor-plugin.toml"),
        format!("schema-version = 1\n[[plugins]]\nname = 'example.lint'\nargv = ['{program}']\n"),
    )
    .unwrap();
}

#[test]
fn plugin_runs_via_the_normal_sensor_gate() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    std::fs::write(root.join("do-harness.toml"), PLUGIN_CONFIG).unwrap();
    manifest(root, "true");
    git(root, &["init", "-q"]);
    git(root, &["add", "--all"]);
    git(root, &["commit", "-qm", "test: installed sensor"]);

    let (code, stdout, stderr) = run(harness(root).arg("list"));
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout.trim(), "lint");

    let (code, stdout, stderr) =
        run(harness(root).args(["verify", "--set", "verification", "--strict"]));
    assert_eq!(code, Some(0), "{stdout}\n{stderr}");

    manifest(root, "false");
    let (code, stdout, stderr) =
        run(harness(root).args(["status", "--set", "verification", "--format", "json"]));
    assert_eq!(code, Some(1), "{stdout}\n{stderr}");
    let status: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(status["state"], "stale");

    let (code, stdout, stderr) =
        run(harness(root).args(["verify", "--set", "verification", "--strict"]));
    assert_eq!(code, Some(1), "{stdout}\n{stderr}");
}

#[test]
fn an_uninstalled_or_conflicting_plugin_is_a_config_error() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let config = root.join("do-harness.toml");
    std::fs::write(&config, PLUGIN_CONFIG).unwrap();
    let (code, _, stderr) = run(harness(root).arg("list"));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("cannot find installed sensor manifest"),
        "{stderr}"
    );

    manifest(root, "true");
    std::fs::write(&config, format!("{PLUGIN_CONFIG}argv = ['true']\n")).unwrap();
    let (code, _, stderr) = run(harness(root).arg("list"));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("mutually exclusive"), "{stderr}");
}
