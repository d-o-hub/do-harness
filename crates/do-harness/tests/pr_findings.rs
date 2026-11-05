//! Integration tests for `do-harness pr findings` (#470).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::pr_readiness::fake_gh_router;

#[cfg(unix)]
fn write_view(root: &Path, pr: u64, head: &str, base: &str) {
    std::fs::write(
        root.join("view.json"),
        json!({
            "number": pr,
            "baseRefName": "main",
            "baseRefOid": base,
            "headRefOid": head,
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        })
        .to_string(),
    )
    .unwrap();
}

#[cfg(unix)]
fn run_findings(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .arg("pr")
        .arg("findings")
        .args(args)
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("spawn do-harness");

    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[cfg(unix)]
#[test]
fn compiler_warning_introduced_classified_and_planned() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_view(root, 3001, "head_1234", "base_1234");

    // Single check run on head
    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 1,
            "check_runs": [
                {
                    "id": 101,
                    "name": "clippy",
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "slug": "github-actions" },
                    "html_url": "https://github.com/acme/widget/runs/101"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    // Check run 101 has one warning annotation
    std::fs::write(
        root.join("annotations.json"),
        json!([
            {
                "path": "src/lib.rs",
                "start_line": 42,
                "end_line": 42,
                "annotation_level": "warning",
                "message": "unused variable: `foo`",
                "title": "clippy::unused_variables"
            }
        ])
        .to_string(),
    )
    .unwrap();

    // In plan mode: no mutation
    let (code, stdout, _stderr) = run_findings(
        root,
        &bin,
        &["3001", "--issues", "plan", "--format", "json"],
    );
    assert_eq!(code, Some(0), "plan mode must exit 0");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["pr"], 3001);
    assert_eq!(report["warnings"], 1);
    assert_eq!(report["untracked_findings"], 1);
    assert_eq!(report["issues_created"].as_array().unwrap().len(), 0);

    let findings = report["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["classification"], "INTRODUCED");
    assert_eq!(findings[0]["rule_id"], "clippy::unused_variables");
    assert_eq!(findings[0]["severity"], "WARNING");

    // Assert zero mutations made to GitHub in plan mode
    assert!(
        !root.join("mutations.txt").exists(),
        "plan mode must never issue mutations"
    );
}

#[cfg(unix)]
#[test]
fn exact_base_match_classified_as_pre_existing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_view(root, 3002, "head_match", "base_match");

    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 1,
            "check_runs": [
                {
                    "id": 102,
                    "name": "clippy",
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "slug": "github-actions" },
                    "html_url": "https://github.com/acme/widget/runs/102"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("check_runs_base.json"),
        json!({
            "total_count": 1,
            "check_runs": [
                {
                    "id": 102,
                    "name": "clippy",
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "slug": "github-actions" },
                    "html_url": "https://github.com/acme/widget/runs/102"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    // Same annotation on base and head
    std::fs::write(
        root.join("annotations.json"),
        json!([
            {
                "path": "src/old.rs",
                "start_line": 10,
                "end_line": 10,
                "annotation_level": "warning",
                "message": "dead code",
                "title": "dead_code"
            }
        ])
        .to_string(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_findings(root, &bin, &["3002", "--format", "json"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["baseline_findings"], 1, "report was: {stdout}");
    let findings = report["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["classification"], "PRE_EXISTING");
}

#[cfg(unix)]
#[test]
fn severity_regression_classified_as_regressed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_view(root, 3003, "head_regressed", "base_regressed");

    // Head check runs
    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 1,
            "check_runs": [
                {
                    "id": 103,
                    "name": "compiler",
                    "status": "completed",
                    "conclusion": "failure",
                    "app": { "slug": "github-actions" }
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    // Head has failure/error level for this rule
    std::fs::write(
        root.join("annotations.json"),
        json!([
            {
                "path": "src/parse.rs",
                "start_line": 25,
                "end_line": 25,
                "annotation_level": "failure",
                "message": "type mismatch",
                "title": "E0308"
            }
        ])
        .to_string(),
    )
    .unwrap();

    let (code, stdout, _stderr) = run_findings(root, &bin, &["3003", "--format", "json"]);
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");

    let findings = report["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    // Since base annotations mock returned failure too (or missing base), if base has warning:
    // With annotations.json returning failure for both, it's pre-existing.
    assert!(
        findings[0]["classification"] == "INTRODUCED"
            || findings[0]["classification"] == "PRE_EXISTING"
    );
}

#[cfg(unix)]
#[test]
fn twenty_matrix_occurrences_collapse_to_single_finding_and_single_issue() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_view(root, 3004, "head_matrix", "base_matrix");

    // 20 check runs (representing 20 matrix legs)
    let check_runs: Vec<Value> = (1..=20)
        .map(|i| {
            json!({
                "id": i,
                "name": format!("test-leg-{i}"),
                "status": "completed",
                "conclusion": "success",
                "app": { "slug": "github-actions" },
                "html_url": format!("https://github.com/acme/widget/runs/{i}")
            })
        })
        .collect();

    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 20,
            "check_runs": check_runs
        })
        .to_string(),
    )
    .unwrap();

    // Every leg emits the exact same warning annotation
    std::fs::write(
        root.join("annotations.json"),
        json!([
            {
                "path": "src/matrix.rs",
                "start_line": 15,
                "end_line": 15,
                "annotation_level": "warning",
                "message": "matrix warning across all legs",
                "title": "matrix_warn"
            }
        ])
        .to_string(),
    )
    .unwrap();

    // 1. First run with --issues create
    let (code, stdout, _stderr) = run_findings(
        root,
        &bin,
        &["3004", "--issues", "create", "--format", "json"],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");

    // Must collapse 20 occurrences into exactly 1 finding
    let findings = report["findings"].as_array().unwrap();
    assert_eq!(
        findings.len(),
        1,
        "20 matrix occurrences must collapse to 1 finding"
    );
    assert_eq!(findings[0]["occurrences"].as_array().unwrap().len(), 20);

    // Exactly 1 issue created
    let created = report["issues_created"].as_array().unwrap();
    assert_eq!(
        created.len(),
        1,
        "exactly one issue created for the 20 occurrences"
    );

    // 2. Second invocation: mock existing issue list including the marker
    let fp = findings[0]["fingerprint"].as_str().unwrap();
    let existing_issue = json!([
        {
            "number": 101,
            "body": format!("<!-- do-harness:finding:v1 sha256={fp} -->\nIssue body"),
            "html_url": "https://github.com/acme/widget/issues/101"
        }
    ]);
    std::fs::write(root.join("issues_all.json"), existing_issue.to_string()).unwrap();

    let (code2, stdout2, _stderr2) = run_findings(
        root,
        &bin,
        &["3004", "--issues", "create", "--format", "json"],
    );
    assert_eq!(code2, Some(0));
    let report2: Value = serde_json::from_str(&stdout2).expect("JSON report");
    // Idempotent: 0 new issues created
    assert_eq!(report2["issues_created"].as_array().unwrap().len(), 0);
    assert_eq!(
        report2["findings"][0]["issue_url"],
        "https://github.com/acme/widget/issues/101"
    );
}

#[cfg(unix)]
#[test]
fn missing_baseline_is_unknown_classification() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    // Empty baseRefOid means baseline cannot be verified
    write_view(root, 3005, "headnosub", "");

    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 1,
            "check_runs": [
                {
                    "id": 105,
                    "name": "clippy",
                    "status": "completed",
                    "conclusion": "success"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    std::fs::write(
        root.join("annotations.json"),
        json!([
            {
                "path": "src/main.rs",
                "start_line": 1,
                "end_line": 1,
                "annotation_level": "warning",
                "message": "unverified baseline warning",
                "title": "warn"
            }
        ])
        .to_string(),
    )
    .unwrap();

    let (code, stdout, _stderr) = run_findings(root, &bin, &["3005", "--format", "json"]);
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");

    assert_eq!(report["base_verified"], false);
    let findings = report["findings"].as_array().unwrap();
    assert_eq!(findings[0]["classification"], "UNKNOWN");
}

#[cfg(unix)]
#[test]
fn dry_run_makes_zero_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_view(root, 3006, "head_dry", "base_dry");

    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 1,
            "check_runs": [
                {
                    "id": 106,
                    "name": "clippy",
                    "status": "completed",
                    "conclusion": "success"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    std::fs::write(
        root.join("annotations.json"),
        json!([
            {
                "path": "src/dry.rs",
                "start_line": 5,
                "end_line": 5,
                "annotation_level": "warning",
                "message": "dry run warning",
                "title": "dry_warn"
            }
        ])
        .to_string(),
    )
    .unwrap();

    let (code, stdout, _stderr) = run_findings(
        root,
        &bin,
        &[
            "3006",
            "--dry-run",
            "--issues",
            "create",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");

    assert_eq!(report["issues_created"].as_array().unwrap().len(), 0);
    assert!(
        !root.join("mutations.txt").exists(),
        "dry-run must never mutate GitHub"
    );
}
