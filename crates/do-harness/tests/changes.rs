//! Acceptance fixtures for change-aware signal selection (`--changed`) and
//! `explain`. These drive the real `do-harness` binary inside real git
//! repositories so applicability is proven computationally.

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

/// Creates a committed git repo with a two-sensor config: `rs-check` reacts
/// to Rust sources, `docs` reacts to markdown.
fn fixture_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let config = r#"
[signal-sets]
all = ["rs-check", "docs"]

[[sensors]]
name = "rs-check"
argv = ["true"]
when-changed = ["**/*.rs", "Cargo.toml", "Cargo.lock"]

[[sensors]]
name = "docs"
argv = ["true"]
when-changed = ["**/*.md", "docs/**"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "# docs\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);
    (dir, root)
}

/// Names of sensors in a JSON verify report.
fn sensor_names(report: &Value) -> Vec<String> {
    report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .map(|s| s["name"].as_str().expect("sensor name").to_owned())
        .collect()
}

/// A Rust edit selects only the Rust sensor under `--changed`.
#[test]
fn rust_edit_selects_only_rust_sensor() {
    let (_dir, root) = fixture_repo();
    std::fs::write(root.join("main.rs"), "fn main() { let x = 1; }\n").unwrap();

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "changed run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(sensor_names(&report), vec!["rs-check".to_owned()]);
}

/// A docs-only edit skips the unrelated implementation sensor.
#[test]
fn docs_only_edit_skips_rust_sensor() {
    let (_dir, root) = fixture_repo();
    std::fs::write(root.join("README.md"), "# docs\n\nmore\n").unwrap();

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "changed run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(sensor_names(&report), vec!["docs".to_owned()]);
}

/// Untracked files drive selection: a new markdown file selects `docs`.
#[test]
fn untracked_file_drives_selection() {
    let (_dir, root) = fixture_repo();
    std::fs::write(root.join("notes.md"), "untracked\n").unwrap();

    let (code, stdout, _) = run(harness(&root)
        .arg("explain")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "explain must exit 0:\n{stdout}");
    let doc: Value = serde_json::from_str(&stdout).expect("json explain");
    let selected: Vec<&str> = doc["selected"]
        .as_array()
        .expect("selected array")
        .iter()
        .map(|s| s["name"].as_str().expect("name"))
        .collect();
    assert!(selected.contains(&"docs"), "untracked notes.md: {doc}");
    let changed: Vec<&str> = doc["changed_files"]
        .as_array()
        .expect("changed_files array")
        .iter()
        .map(|f| f.as_str().expect("file"))
        .collect();
    assert!(changed.contains(&"notes.md"), "changed files: {changed:?}");
}

/// `explain` describes every selected and skipped sensor with reasons.
#[test]
fn explain_reports_reasons_for_selected_and_skipped() {
    let (_dir, root) = fixture_repo();
    std::fs::write(root.join("main.rs"), "fn main() { let x = 1; }\n").unwrap();

    let (code, stdout, _) = run(harness(&root)
        .arg("explain")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "explain must exit 0:\n{stdout}");
    let doc: Value = serde_json::from_str(&stdout).expect("json explain");
    assert_eq!(doc["set"], serde_json::json!("all"));

    let selected = doc["selected"].as_array().expect("selected array");
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0]["name"], serde_json::json!("rs-check"));
    assert!(
        selected[0]["reason"]
            .as_str()
            .expect("reason")
            .contains("**/*.rs"),
        "selected reason must name the matching pattern: {selected:?}"
    );

    let skipped = doc["skipped"].as_array().expect("skipped array");
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["name"], serde_json::json!("docs"));
    assert!(
        !skipped[0]["reason"].as_str().expect("reason").is_empty(),
        "skipped sensors need a reason too"
    );
}

/// Outside a git repository `--changed` fails closed and runs everything.
#[test]
fn changed_outside_git_selects_everything() {
    let dir = tempfile::tempdir().unwrap();
    let config = r#"
[signal-sets]
all = ["a", "b"]

[[sensors]]
name = "a"
argv = ["true"]
when-changed = ["**/*.rs"]

[[sensors]]
name = "b"
argv = ["true"]
when-changed = ["**/*.md"]
"#;
    std::fs::write(dir.path().join("do-harness.toml"), config).unwrap();

    let (code, stdout, _) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "fail-closed run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(sensor_names(&report), vec!["a".to_owned(), "b".to_owned()]);

    let (code, stdout, _) = run(harness(dir.path())
        .arg("explain")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0));
    let doc: Value = serde_json::from_str(&stdout).expect("json explain");
    assert_eq!(doc["selected"].as_array().expect("array").len(), 2);
    assert!(
        doc["selected"][0]["reason"]
            .as_str()
            .expect("reason")
            .contains("fail-closed"),
        "fail-closed selection must say so: {}",
        doc["selected"]
    );
}

/// Runs `explain --set all --changed --format json` in `root`.
fn explain(root: &Path) -> (Option<i32>, String, String) {
    run(harness(root)
        .arg("explain")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"))
}

/// Explain is deterministic across repeated invocations.
#[test]
fn explain_is_deterministic() {
    let (_dir, root) = fixture_repo();
    std::fs::write(root.join("main.rs"), "fn main() { let x = 1; }\n").unwrap();

    let (first, s1, _) = explain(&root);
    let (second, s2, _) = explain(&root);
    assert_eq!((first, second), (Some(0), Some(0)));
    assert_eq!(s1, s2, "explain output must be deterministic");
}
