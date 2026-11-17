//! Acceptance fixtures for `do-harness sensor new`.
//!
//! The command edits the workspace's policy file, so every case asserts the
//! file's resulting bytes, not just the exit code: a refusal must leave the
//! configuration untouched, and a success must be one the selection resolver
//! accepts.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::{harness, run};

const CONFIG: &str = r#"
[signal-sets]
feedback = ["fmt"]

[[sensors]]
name = "fmt"
argv = ["true"]
"#;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("do-harness.toml"), CONFIG).unwrap();
    (dir, root)
}

fn read_config(root: &Path) -> String {
    std::fs::read_to_string(root.join("do-harness.toml")).unwrap()
}

#[test]
fn a_new_sensor_is_written_and_selected() {
    let (_dir, root) = fixture();
    let (code, stdout, stderr) = run(harness(&root).args([
        "sensor",
        "new",
        "lint",
        "--arg",
        "true",
        "--when-changed",
        "src/**",
        "--set",
        "feedback",
    ]));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("sensor 'lint' added"), "{stdout}");
    let config = read_config(&root);
    assert!(config.contains("[[sensors]]\nname = \"lint\""), "{config}");
    assert!(config.contains("when-changed = [\"src/**\"]"), "{config}");
    assert!(config.contains(r#"feedback = ["fmt", "lint"]"#), "{config}");

    // The proof is the real selection path, not a string check.
    let (code, stdout, stderr) =
        run(harness(&root).args(["verify", "--set", "feedback", "--format", "json"]));
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("verify json");
    let names: Vec<&str> = report["sensors"]
        .as_array()
        .expect("sensors")
        .iter()
        .map(|sensor| sensor["name"].as_str().expect("name"))
        .collect();
    assert!(names.contains(&"lint"), "{names:?}");
}

#[test]
fn a_dry_run_writes_nothing() {
    let (_dir, root) = fixture();
    let before = read_config(&root);
    let (code, stdout, _stderr) =
        run(harness(&root).args(["--dry-run", "sensor", "new", "lint", "--arg", "true"]));
    assert_eq!(code, Some(0));
    assert!(stdout.contains("would gain"), "{stdout}");
    assert_eq!(read_config(&root), before);
}

#[test]
fn an_unknown_signal_set_is_refused_without_writing() {
    let (_dir, root) = fixture();
    let before = read_config(&root);
    let (code, _stdout, stderr) = run(harness(&root).args([
        "sensor",
        "new",
        "lint",
        "--arg",
        "true",
        "--set",
        "verification",
    ]));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("signal set 'verification'"), "{stderr}");
    assert_eq!(read_config(&root), before);
}

#[test]
fn a_duplicate_or_invalid_name_is_refused() {
    let (_dir, root) = fixture();
    let before = read_config(&root);

    let (code, _stdout, stderr) =
        run(harness(&root).args(["sensor", "new", "fmt", "--arg", "true"]));
    assert_eq!(code, Some(2));
    assert!(stderr.contains("already exists"), "{stderr}");

    let (code, _stdout, stderr) =
        run(harness(&root).args(["sensor", "new", "Lint", "--arg", "true"]));
    assert_eq!(code, Some(2));
    assert!(stderr.contains("invalid sensor name"), "{stderr}");
    assert_eq!(read_config(&root), before);
}

#[test]
fn a_missing_argv_is_refused() {
    let (_dir, root) = fixture();
    let (code, _stdout, stderr) = run(harness(&root).args(["sensor", "new", "lint"]));
    assert_eq!(code, Some(2), "clap must require --arg:\n{stderr}");
}

#[test]
fn the_script_stub_is_executable_and_never_overwrites() {
    let (_dir, root) = fixture();
    let (code, stdout, stderr) = run(harness(&root).args([
        "sensor",
        "new",
        "lint",
        "--arg",
        "bash",
        "--arg",
        "scripts/lint.sh",
        "--script",
        "scripts/lint.sh",
    ]));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("scaffolded script"), "{stdout}");
    let script = root.join("scripts/lint.sh");
    assert!(script.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "stub must be executable: {mode:o}");
    }
    let config = read_config(&root);
    assert!(config.contains(r#"argv = ["bash", "scripts/lint.sh"]"#));

    // A second scaffold on the same path must fail closed and leave the file.
    let (code, _stdout, stderr) = run(harness(&root).args([
        "sensor",
        "new",
        "lint2",
        "--arg",
        "bash",
        "--arg",
        "scripts/lint.sh",
        "--script",
        "scripts/lint.sh",
    ]));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("refusing to overwrite"), "{stderr}");
    assert!(!read_config(&root).contains("lint2"), "config was written");
    assert!(script.exists(), "existing script must survive");
}
