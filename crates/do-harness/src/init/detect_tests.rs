#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// An empty directory resolves to greenfield Rust and reports it.
#[test]
fn empty_directory_is_greenfield_rust() {
    let dir = tempfile::tempdir().unwrap();
    let detection = inspect(dir.path(), None, None);
    assert_eq!(detection.language, Language::Rust);
    assert!(
        detection
            .findings
            .iter()
            .any(|f| f.contains("empty directory"))
    );
}

/// A `Cargo.toml` resolves to Rust and reports the crate/workspace kind.
#[test]
fn cargo_manifest_selects_rust() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    let detection = inspect(dir.path(), None, None);
    assert_eq!(detection.language, Language::Rust);
    assert!(detection.findings.iter().any(|f| f == "Cargo.toml"));
    assert!(detection.findings.iter().any(|f| f == "Rust crate"));
}

/// A workspace manifest reports "Rust workspace".
#[test]
fn workspace_manifest_reports_workspace() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    let detection = inspect(dir.path(), None, None);
    assert!(detection.findings.iter().any(|f| f == "Rust workspace"));
}

const SUPPORTED_PKG: &str = r#"{"scripts": {"test": "node --test"}}"#;

fn write_pkg(root: &Path, body: &str) {
    std::fs::write(root.join("package.json"), body).unwrap();
}

/// A package.json without supported commands stays Generic and emits a
/// diagnostic instead of claiming a meaningful green result.
#[test]
fn unsupported_package_json_selects_generic_with_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    write_pkg(dir.path(), "{}\n");
    let detection = inspect(dir.path(), None, None);
    assert_eq!(detection.language, Language::Generic);
    assert_eq!(detection.candidates.len(), 0);
    assert!(
        detection
            .diagnostics
            .iter()
            .any(|d| { d.code == crate::init::diagnostic::code::NO_SUPPORTED_NODE_COMMANDS })
    );
}

/// A package.json proving a supported Node command selects the Node pack.
#[test]
fn supported_package_json_selects_node() {
    let dir = tempfile::tempdir().unwrap();
    write_pkg(dir.path(), SUPPORTED_PKG);
    let detection = inspect(dir.path(), None, None);
    assert_eq!(detection.language, Language::Node);
    // Detection owns the language resolution and the manifest-derived
    // candidates: the test signal is proven by the manifest regardless of
    // tool availability. Inclusion is the probe's call (it depends on a
    // runnable package manager) and is asserted in init_node_auto.
    assert_eq!(detection.candidates.len(), 4);
    assert!(detection.candidates.iter().any(|c| c.name == "test"));
}

/// A malformed package.json stays Generic and emits a diagnostic.
#[test]
fn malformed_package_json_selects_generic_with_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    write_pkg(dir.path(), "{oops");
    let detection = inspect(dir.path(), None, None);
    assert_eq!(detection.language, Language::Generic);
    assert!(
        detection
            .diagnostics
            .iter()
            .any(|d| { d.code == crate::init::diagnostic::code::INVALID_PACKAGE_JSON })
    );
}

/// An explicit request wins over repository detection.
#[test]
fn explicit_language_wins() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    let detection = inspect(dir.path(), Some(Language::Generic), None);
    assert_eq!(detection.language, Language::Generic);
}

/// A declared existing-config pack outranks directory markers.
#[test]
fn existing_config_language_wins_over_markers() {
    let dir = tempfile::tempdir().unwrap();
    write_pkg(dir.path(), "{}\n");
    let detection = inspect(dir.path(), None, Some("rust"));
    assert_eq!(detection.language, Language::Rust);
    assert!(detection.candidates.iter().any(|c| c.name == "fmt"));
}

/// Included specs follow the probe decisions, not the full pack.
#[test]
fn included_specs_follow_probes() {
    let candidates = vec![
        candidate("fmt", true, true, ""),
        candidate("check", false, false, ""),
        candidate("clippy", true, true, "optional tool missing"),
    ];
    let dummy = std::path::Path::new(".");
    let specs = included_specs(dummy, Language::Rust, &candidates);
    let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["fmt", "clippy"]);
}
