//! Integration tests for plans/regression-matrix.json.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct MatrixEntry {
    id: String,
    symptom: String,
    signature: String,
    fix_hint: String,
    sensor: String,
}

#[test]
fn test_regression_matrix_entries_and_fixtures() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("failed to resolve repo root");

    let matrix_path = repo_root.join("plans/regression-matrix.json");
    assert!(
        matrix_path.exists(),
        "plans/regression-matrix.json should exist"
    );

    let content = fs::read_to_string(&matrix_path).expect("failed to read regression-matrix.json");
    let entries: Vec<MatrixEntry> =
        serde_json::from_str(&content).expect("regression-matrix.json should be valid JSON array");

    assert!(
        entries.len() >= 5,
        "regression matrix should have at least 5 seeded entries, found {}",
        entries.len()
    );

    let fixtures_dir = repo_root.join("tests/fixtures/regression-matrix");
    let clean_path = fixtures_dir.join("clean.log");
    assert!(
        clean_path.exists(),
        "clean.log fixture should exist at {:?}",
        clean_path
    );
    let clean_text = fs::read_to_string(&clean_path).expect("failed to read clean.log");

    for entry in &entries {
        assert!(!entry.id.is_empty(), "id should not be empty");
        assert!(!entry.symptom.is_empty(), "symptom should not be empty");
        assert!(!entry.signature.is_empty(), "signature should not be empty");
        assert!(!entry.fix_hint.is_empty(), "fix_hint should not be empty");
        assert!(!entry.sensor.is_empty(), "sensor should not be empty");

        let re = Regex::new(&entry.signature)
            .unwrap_or_else(|e| panic!("invalid regex signature '{}' for entry '{}': {}", entry.signature, entry.id, e));

        // Positive test: must match fixture
        let fixture_path = fixtures_dir.join(format!("{}.log", entry.id));
        assert!(
            fixture_path.exists(),
            "positive fixture log {:?} should exist for entry '{}'",
            fixture_path,
            entry.id
        );
        let fixture_text = fs::read_to_string(&fixture_path)
            .unwrap_or_else(|e| panic!("failed to read fixture {:?}: {}", fixture_path, e));

        assert!(
            re.is_match(&fixture_text),
            "entry '{}' signature '{}' should match its positive fixture {:?}",
            entry.id,
            entry.signature,
            fixture_path
        );

        // Negative test: must NOT match clean log
        assert!(
            !re.is_match(&clean_text),
            "entry '{}' signature '{}' should NOT match clean output",
            entry.id,
            entry.signature
        );
    }
}
