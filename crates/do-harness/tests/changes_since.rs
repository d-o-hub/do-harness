//! Acceptance fixtures for committed-range signal selection (`--since`).
//!
//! These drive the real `do-harness` binary inside real git repositories, so a
//! clean checkout of an already-committed branch is proven to select the
//! sensors its commits touch.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use serde_json::Value;

mod support;

use support::{git, harness, run};

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

/// Resolves `spec` to a full SHA inside `root`.
fn rev(root: &Path, spec: &str) -> String {
    let output = support::git_command(root)
        .args(["rev-parse", spec])
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git rev-parse {spec} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Commits a Rust change on top of the fixture's base commit and returns the
/// base SHA, leaving a clean working tree.
fn committed_rust_change(root: &Path) -> String {
    let base = rev(root, "HEAD");
    std::fs::write(root.join("main.rs"), "fn main() { let x = 1; }\n").unwrap();
    git(root, &["commit", "-qam", "test: rust change"]);
    base
}

/// A clean committed branch selects its sensors from the committed range,
/// where `--changed` alone sees nothing.
#[test]
fn committed_range_selects_rust_sensor_on_clean_checkout() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "changed run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert!(
        sensor_names(&report).is_empty(),
        "a clean checkout has no working-tree change: {report}"
    );

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "since run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(sensor_names(&report), vec!["rs-check".to_owned()]);
    assert_eq!(report["selection"]["mode"], serde_json::json!("range"));
    assert_eq!(report["selection"]["since"], serde_json::json!(base));
    assert_eq!(
        report["selection"]["head_sha"],
        serde_json::json!(rev(&root, "HEAD"))
    );
    assert_eq!(
        report["selection"]["merge_base_sha"],
        serde_json::json!(base)
    );
    assert_eq!(
        report["selection"]["skipped"][0]["name"],
        serde_json::json!("docs")
    );
    // Selected sensors carry their selection reasons in the verify report.
    assert_eq!(
        report["selection"]["selected"][0]["name"],
        serde_json::json!("rs-check")
    );
    let reason = report["selection"]["selected"][0]["reason"]
        .as_str()
        .expect("selected reason");
    assert!(!reason.is_empty(), "selected reason must be recorded");
}

/// `--since` and `--changed` union committed and uncommitted paths.
#[test]
fn since_and_changed_union_paths() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    std::fs::write(root.join("README.md"), "# docs\n\nmore\n").unwrap();

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "union run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(
        sensor_names(&report),
        vec!["rs-check".to_owned(), "docs".to_owned()]
    );
    assert_eq!(
        report["selection"]["mode"],
        serde_json::json!("working-tree+range")
    );
}

/// An unresolvable `--since` ref fails closed: every sensor runs and the
/// report says why.
#[test]
fn since_unknown_ref_fails_closed() {
    let (_dir, root) = fixture_repo();

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg("origin/does-not-exist")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "fail-closed run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(
        sensor_names(&report),
        vec!["rs-check".to_owned(), "docs".to_owned()]
    );
    let error = report["selection"]["discovery_error"]
        .as_str()
        .expect("discovery error");
    assert!(
        error.contains("origin/does-not-exist"),
        "diagnostic must name the ref: {error}"
    );
}

/// `explain --since` reports the resolved revisions without running sensors.
#[test]
fn explain_since_reports_revisions() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);

    let (code, stdout, _) = run(harness(&root)
        .arg("explain")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "explain must exit 0:\n{stdout}");
    let doc: Value = serde_json::from_str(&stdout).expect("json explain");
    assert_eq!(doc["selection_mode"], serde_json::json!("range"));
    assert_eq!(doc["since"], serde_json::json!(base));
    assert_eq!(doc["base_sha"], serde_json::json!(base));
    assert_eq!(doc["head_sha"], serde_json::json!(rev(&root, "HEAD")));
    assert_eq!(doc["merge_base_sha"], serde_json::json!(base));
    let selected: Vec<&str> = doc["selected"]
        .as_array()
        .expect("selected array")
        .iter()
        .map(|s| s["name"].as_str().expect("name"))
        .collect();
    assert_eq!(selected, vec!["rs-check"]);
}

/// `status` judges range evidence against the same range, and goes stale when
/// the head moves.
#[test]
fn status_tracks_range_selection() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);

    let (code, _, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "range verify must exit 0:\n{stderr}");

    let (code, stdout, _) = run(harness(&root)
        .arg("status")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "range evidence must be green:\n{stdout}");
    let doc: Value = serde_json::from_str(&stdout).expect("json status");
    assert_eq!(doc["state"], serde_json::json!("green"));

    std::fs::write(root.join("main.rs"), "fn main() { let y = 2; }\n").unwrap();
    git(&root, &["commit", "-qam", "test: another change"]);

    let (code, stdout, _) = run(harness(&root)
        .arg("status")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("json"));
    assert_ne!(code, Some(0), "a moved head must not stay green:\n{stdout}");
    let doc: Value = serde_json::from_str(&stdout).expect("json status");
    assert_eq!(doc["state"], serde_json::json!("stale"));
    assert_eq!(doc["reason"], serde_json::json!("selection_changed"));
}
