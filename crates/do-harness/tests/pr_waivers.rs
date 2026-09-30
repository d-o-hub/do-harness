//! Integration coverage for `do-harness pr waivers`.
//!
//! The fixtures under `tests/fixtures/waivers/` are extracts of measured
//! residue from two public PRs (see their README): the classifier must
//! reproduce the waived classes on them without a human reading the report.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Fixture directory for one extracted case.
fn fixture(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/waivers")
        .join(case)
}

/// Runs `pr waivers` against a fixture, returning (exit code, stdout, stderr).
fn run(case: &str, extra: &[&str]) -> (Option<i32>, String, String) {
    let root = fixture(case);
    let mut command = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    command
        .arg("--root")
        .arg(&root)
        .args(["pr", "waivers", "--patch"])
        .arg(root.join("patch.diff"))
        .args(["--lcov"])
        .arg(root.join("lcov.info"))
        .args(extra);
    let output = command.output().expect("run do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// JSON report of a fixture run.
fn report(case: &str, extra: &[&str]) -> Value {
    let mut args = vec!["--format", "json"];
    args.extend_from_slice(extra);
    let (code, stdout, stderr) = run(case, &args);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    serde_json::from_str(&stdout).expect("report JSON")
}

/// Class of one line in the JSON report.
fn class(report: &Value, path: &str, line: u64) -> String {
    let file = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == path)
        .unwrap_or_else(|| panic!("{path} missing from {report}"));
    file["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|verdict| verdict["line"] == line)
        .unwrap_or_else(|| panic!("line {line} missing from {file}"))["class"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn case_a_reproduces_the_waived_classes() {
    // PR #1041: the `let-else` alignment fallback and the `tracing` field
    // expressions, both waived by hand in the pull request discussion.
    let report = report("case-a", &[]);
    for line in [89, 90, 91, 92] {
        assert_eq!(
            class(&report, "src/retrieval/judgment.rs", line),
            "guarded-arm"
        );
    }
    for line in [115, 116, 127, 128] {
        assert_eq!(
            class(&report, "src/retrieval/judgment.rs", line),
            "macro-field"
        );
    }
    assert_eq!(report["counts"]["guarded-arm"], 4);
    assert_eq!(report["counts"]["macro-field"], 4);
    assert!(report["counts"].get("missing").is_none(), "{report}");
    assert!(report["unresolved_files"].as_array().unwrap().is_empty());
}

#[test]
fn case_b_reproduces_all_three_classes() {
    // PR #1042: the same macro-field class, the defensive count-mismatch arm
    // after the equality-guarded arm, and the `csm`-gated module the measured
    // pipeline never compiled (absent from the report entirely).
    let report = report("case-b", &[]);
    for line in 19..=38 {
        assert_eq!(
            class(&report, "src/retrieval/cascade/mod.rs", line),
            "feature-gated"
        );
    }
    for line in [86, 91] {
        assert_eq!(
            class(&report, "src/retrieval/rerank.rs", line),
            "macro-field"
        );
    }
    for line in (97..=105).chain(std::iter::once(107)) {
        assert_eq!(
            class(&report, "src/retrieval/rerank.rs", line),
            "guarded-arm"
        );
    }
    assert_eq!(report["counts"]["feature-gated"], 20);
    assert_eq!(report["counts"]["macro-field"], 2);
    assert_eq!(report["counts"]["guarded-arm"], 10);
    assert!(report["counts"].get("missing").is_none(), "{report}");
}

#[test]
fn the_text_output_is_the_paste_ready_comment() {
    let (code, stdout, stderr) = run("case-a", &[]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert!(
        stdout.starts_with("### Patch-coverage residue\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("8 uncovered changed line(s) across 1 file(s)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("| `src/retrieval/judgment.rs` | 89-92 | guarded-arm |"),
        "{stdout}"
    );
    assert!(
        stdout.contains("| `src/retrieval/judgment.rs` | 115-116 | macro-field |"),
        "{stdout}"
    );
    assert!(stdout.contains("**Per file**"), "{stdout}");
    assert!(
        stdout.contains("| `src/retrieval/judgment.rs` | 8 | 0 |"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("| `src/retrieval/judgment.rs` | 8 | 0 |\n"),
        "{stdout}"
    );
}

#[test]
fn the_previous_report_reports_lines_covered_since() {
    // The head report covers line 113; the previous one missed it.
    let current = std::fs::read_to_string(fixture("case-a").join("lcov.info")).unwrap();
    let previous = current.replace("DA:113,2", "DA:113,0");
    assert_ne!(current, previous);
    let dir = tempfile::tempdir().unwrap();
    let since = dir.path().join("since.info");
    std::fs::write(&since, previous).unwrap();

    let report = report("case-a", &["--since", since.to_str().unwrap()]);
    let covered = report["covered_since"].as_array().unwrap();
    assert_eq!(covered.len(), 1, "{report}");
    assert_eq!(covered[0]["path"], "src/retrieval/judgment.rs");
    assert_eq!(covered[0]["line"], 113);
}

#[test]
fn an_unreadable_report_is_a_usage_error() {
    let root = fixture("case-a");
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(&root)
        .args(["pr", "waivers", "--patch"])
        .arg(root.join("patch.diff"))
        .args(["--lcov", "/nonexistent/lcov.info"])
        .output()
        .expect("run do-harness");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot read lcov report"), "{stderr}");
}

#[test]
fn a_patch_file_is_exclusive_with_a_revision_range() {
    let root = fixture("case-a");
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(&root)
        .args(["pr", "waivers", "--patch"])
        .arg(root.join("patch.diff"))
        .args(["--lcov"])
        .arg(root.join("lcov.info"))
        .args(["--base", "main", "--head", "HEAD"])
        .output()
        .expect("run do-harness");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be used with"), "{stderr}");
}
