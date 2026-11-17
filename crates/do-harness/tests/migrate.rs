//! Acceptance fixtures for `do-harness migrate` and the doctor schema check.
//!
//! Migration edits the policy file, so every refusal is proven to leave the
//! bytes untouched, and the applied result is reloaded by the real config
//! parser rather than asserted as text alone.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::{harness, run};

const V0_CONFIG: &str = r#"
language = "generic"

[[sensors]]
name = "check"
argv = ["true"]
allow_failure = true
"#;

fn fixture(config: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    (dir, root)
}

fn read_config(root: &Path) -> String {
    std::fs::read_to_string(root.join("do-harness.toml")).unwrap()
}

#[test]
fn diff_previews_without_writing() {
    let (_dir, root) = fixture(V0_CONFIG);
    let before = read_config(&root);
    let (code, stdout, stderr) =
        run(harness(&root).args(["migrate", "--diff", "--format", "json"]));
    assert_eq!(code, Some(0), "{stderr}");
    let plan: Value = serde_json::from_str(&stdout).expect("plan json");
    assert_eq!(plan["from"], serde_json::json!(0));
    assert_eq!(plan["to"], serde_json::json!(1));
    assert_eq!(plan["applicable"], serde_json::json!(true));
    let reasons: Vec<&str> = plan["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .map(|change| change["reason"].as_str().expect("reason"))
        .collect();
    assert!(reasons.contains(&"schema-version"), "{reasons:?}");
    assert!(reasons.contains(&"allow-failure-alias"), "{reasons:?}");
    assert_eq!(read_config(&root), before, "diff must not write");
}

#[test]
fn write_applies_a_config_the_parser_accepts() {
    let (_dir, root) = fixture(V0_CONFIG);
    let (code, stdout, stderr) = run(harness(&root).args(["migrate", "--write"]));
    assert_eq!(code, Some(0), "{stderr}\n{stdout}");
    let config = read_config(&root);
    assert!(config.contains("schema-version = 1"), "{config}");
    assert!(config.contains("severity = \"warn\""), "{config}");
    assert!(!config.contains("allow_failure"), "{config}");

    // Reloaded by the real parser, not just string-matched.
    let (code, stdout, stderr) = run(harness(&root).args(["list", "--format", "json"]));
    assert_eq!(code, Some(0), "{stderr}");
    let names: Value = serde_json::from_str(&stdout).expect("list json");
    assert_eq!(names, serde_json::json!(["check"]));
}

#[test]
fn a_conflict_refuses_to_write() {
    let config = r#"
language = "generic"

[[sensors]]
name = "check"
argv = ["true"]
severity = "error"
allow_failure = true
"#;
    let (_dir, root) = fixture(config);
    let before = read_config(&root);
    let (code, _stdout, stderr) = run(harness(&root).args(["migrate", "--write"]));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("both `allow_failure` and `severity`"),
        "{stderr}"
    );
    assert_eq!(read_config(&root), before);
}

#[test]
fn a_newer_schema_is_refused() {
    let (_dir, root) = fixture("schema-version = 99\nlanguage = \"generic\"\n");
    let before = read_config(&root);
    let (code, _stdout, stderr) = run(harness(&root).args(["migrate", "--write"]));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("upgrade do-harness"), "{stderr}");
    assert_eq!(read_config(&root), before);
}

#[test]
fn a_current_file_is_a_noop() {
    let (_dir, root) = fixture("schema-version = 1\nlanguage = \"generic\"\n");
    let (code, stdout, _stderr) = run(harness(&root).args(["migrate", "--write"]));
    assert_eq!(code, Some(0));
    assert!(stdout.contains("already at schema version 1"), "{stdout}");
}

#[test]
fn a_missing_config_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let (code, _stdout, stderr) = run(harness(dir.path()).args(["migrate", "--diff"]));
    assert_eq!(code, Some(2));
    assert!(stderr.contains("no "), "{stderr}");
    assert!(stderr.contains("do-harness.toml"), "{stderr}");
}

#[test]
fn doctor_recommends_migrate_and_accepts_the_result() {
    let (_dir, root) = fixture(V0_CONFIG);
    support::git(&root, &["init", "-q"]);
    let (code, _stdout, stderr) = run(harness(&root).arg("doctor"));
    assert_eq!(code, Some(1), "an outdated schema is a doctor failure");
    assert!(
        stderr.contains("schema version 0 is outdated; run: do-harness migrate --write"),
        "{stderr}"
    );

    let (code, _stdout, stderr) = run(harness(&root).args(["migrate", "--write"]));
    assert_eq!(code, Some(0), "{stderr}");
    let (_code, stdout, stderr) = run(harness(&root).arg("doctor"));
    // The migrated workspace may still have unrelated failures (no hooks
    // installed in the fixture), but schema skew must be gone.
    assert!(
        !stdout.contains("schema version 0 is outdated")
            && !stderr.contains("schema version 0 is outdated"),
        "doctor still reports the old schema:\n{stdout}\n{stderr}"
    );
}
