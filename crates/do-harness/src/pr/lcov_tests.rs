//! Unit tests for lcov parsing and path resolution.

use std::path::PathBuf;

use super::lcov;

/// Fixture root holding the extracted patch-set residue.
fn fixture(case: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/waivers")
        .join(case)
}

#[test]
fn parses_line_records_and_ignores_other_records() {
    let report = lcov::parse(
        "SF:src/lib.rs\nFN:1,main\nFNDA:2,main\nDA:1,0\nDA:2,3\nBRDA:2,0,0,-\nLF:2\nLH:1\nend_of_record\n",
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let file = &report.files["src/lib.rs"];
    assert_eq!(file.uncovered.iter().copied().collect::<Vec<_>>(), vec![1]);
    assert!(file.records(2));
    assert!(!file.misses(2));
    assert!(!file.records(3));
}

#[test]
fn da_outside_a_file_section_warns_and_is_not_applied() {
    let report = lcov::parse("DA:7,0\nSF:src/lib.rs\nDA:7,0\nend_of_record\n");
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].contains("outside a file section"));
    assert!(report.files["src/lib.rs"].misses(7));
}

#[test]
fn malformed_records_warn_without_losing_the_rest() {
    let report = lcov::parse("SF:src/lib.rs\nDA:oops\nDA:5,1\nend_of_record\n");
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.files["src/lib.rs"].records(5));
}

#[test]
fn multiple_sections_for_one_file_merge() {
    let report =
        lcov::parse("SF:src/lib.rs\nDA:1,0\nend_of_record\nSF:src/lib.rs\nDA:2,0\nend_of_record\n");
    let file = &report.files["src/lib.rs"];
    assert!(file.misses(1) && file.misses(2));
}

#[test]
fn resolves_a_relative_path_under_the_root() {
    let root = fixture("case-a");
    assert_eq!(
        lcov::resolve_path("src/retrieval/judgment.rs", None, &root).as_deref(),
        Some("src/retrieval/judgment.rs")
    );
}

#[test]
fn resolves_a_ci_checkout_prefix_to_the_longest_existing_suffix() {
    let root = fixture("case-a");
    // The measured report carries the runner path (fixtures/waivers/README.md).
    assert_eq!(
        lcov::resolve_path(
            "/home/runner/work/rust-self-learning-memory/rust-self-learning-memory/src/retrieval/judgment.rs",
            None,
            &root
        )
        .as_deref(),
        Some("src/retrieval/judgment.rs")
    );
}

#[test]
fn an_explicit_strip_prefix_is_applied_first() {
    let root = fixture("case-a");
    assert_eq!(
        lcov::resolve_path(
            "/build/checkout/src/retrieval/judgment.rs",
            Some("/build/checkout"),
            &root
        )
        .as_deref(),
        Some("src/retrieval/judgment.rs")
    );
}

#[test]
fn an_unknown_path_resolves_to_nothing() {
    let root = fixture("case-a");
    assert_eq!(lcov::resolve_path("/nowhere/src/lib.rs", None, &root), None);
    assert_eq!(lcov::resolve_path("", None, &root), None);
}
