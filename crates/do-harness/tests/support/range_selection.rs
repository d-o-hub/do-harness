//! Range-selection fixture helpers shared by `changes_since` acceptance
//! binaries. Each test binary compiles its own copy of `support`, so these
//! are safe to share across the two selection test files.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{git, git_command};

/// Creates a committed git repo with a two-sensor config: `rs-check` reacts
/// to Rust sources, `docs` reacts to markdown.
pub fn fixture_repo() -> (tempfile::TempDir, PathBuf) {
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
pub fn sensor_names(report: &Value) -> Vec<String> {
    report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .map(|s| s["name"].as_str().expect("sensor name").to_owned())
        .collect()
}

/// Resolves `spec` to a full SHA inside `root`.
pub fn rev(root: &Path, spec: &str) -> String {
    let output = git_command(root)
        .args(["rev-parse", spec])
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git rev-parse {spec} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Commits a Rust change on top of the fixture's base commit and returns the
/// base SHA, leaving a clean working tree.
pub fn committed_rust_change(root: &Path) -> String {
    let base = rev(root, "HEAD");
    std::fs::write(root.join("main.rs"), "fn main() { let x = 1; }\n").unwrap();
    git(root, &["commit", "-qam", "test: rust change"]);
    base
}
