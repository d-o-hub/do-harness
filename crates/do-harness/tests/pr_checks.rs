//! Acceptance fixtures for `do-harness pr checks` (#465).
//!
//! The typed classifier is the single merge-authorizing source of CI truth.
//! These fixtures drive the real binary against a fake `gh` and prove the
//! fail-closed contract: unreadable, truncated, empty, and missing-required
//! states never read as a pass.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

mod support;

#[cfg(unix)]
use support::pr_readiness::{fake_gh_router, run_pr};

#[cfg(unix)]
fn write_view(root: &std::path::Path, pr: u64) {
    std::fs::write(
        root.join("view.json"),
        format!(
            r#"{{
                "number": {pr},
                "baseRefName": "main",
                "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
                "mergeStateStatus": "CLEAN",
                "mergeable": "MERGEABLE",
                "autoMergeRequest": null
            }}"#
        ),
    )
    .unwrap();
}

#[cfg(unix)]
fn write_policy(root: &std::path::Path, body: &str) {
    let dir = root.join(".git/pr-triage");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("checks-policy.toml"), body).unwrap();
}

#[cfg(unix)]
fn write_page(root: &std::path::Path, page: &Value) {
    std::fs::write(root.join("check_runs.json"), page.to_string()).unwrap();
}

#[cfg(unix)]
#[test]
fn an_unreadable_api_is_unknown_and_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 2001);
    std::fs::write(root.join("fail_api"), "").unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "checks", "2001", "--format", "json"]);
    assert_eq!(code, Some(3), "unreadable API must exit 3: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["verdict"], "UNKNOWN");
    assert_eq!(report["blockers"][0]["code"], "api_unreadable");
}

#[cfg(unix)]
#[test]
fn zero_checks_and_statuses_do_not_pass() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 2002);
    std::fs::write(root.join("empty_checks"), "").unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "checks", "2002", "--format", "json"]);
    assert_eq!(code, Some(1), "an empty suite must not pass: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["verdict"], "UNKNOWN");
    assert!(
        report["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["code"] == "no_checks_reported")
    );
}

#[cfg(unix)]
#[test]
fn the_101st_failing_check_fails_the_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 2003);

    let mut runs: Vec<Value> = (0..100)
        .map(|i| {
            json!({
                "id": i,
                "name": format!("check-{i}"),
                "status": "completed",
                "conclusion": "success",
                "app": {"slug": "github-actions"}
            })
        })
        .collect();
    runs.push(json!({
        "id": 101,
        "name": "check-101",
        "status": "completed",
        "conclusion": "failure",
        "app": {"slug": "github-actions"}
    }));
    write_page(root, &json!({"total_count": 101, "check_runs": runs}));

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "checks", "2003", "--format", "json"]);
    assert_eq!(code, Some(1), "the 101st failure must fail: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["verdict"], "FAIL");
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "check-101" && c["verdict"] == "FAIL")
    );
}

#[cfg(unix)]
#[test]
fn a_missing_required_check_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 2004);
    write_policy(root, "required = [\"verify\"]\n");
    write_page(
        root,
        &json!({
            "total_count": 1,
            "check_runs": [{
                "id": 1,
                "name": "docs",
                "status": "completed",
                "conclusion": "success",
                "app": {"slug": "github-actions"}
            }]
        }),
    );

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "checks", "2004", "--format", "json"]);
    assert_eq!(
        code,
        Some(1),
        "a missing required check must not pass: {stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["verdict"], "UNKNOWN");
    assert!(
        report["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["code"] == "missing_required_check")
    );
}

#[cfg(unix)]
#[test]
fn bot_and_external_failures_are_included() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 2005);
    write_page(
        root,
        &json!({
            "total_count": 2,
            "check_runs": [
                {"id": 1, "name": "dependabot", "status": "completed", "conclusion": "failure", "app": {"slug": "dependabot"}},
                {"id": 2, "name": "Codacy", "status": "completed", "conclusion": "failure", "app": {"slug": "codacy"}}
            ]
        }),
    );

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "checks", "2005", "--format", "json"]);
    assert_eq!(code, Some(1), "bot failures must fail: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["verdict"], "FAIL");
    let sources: Vec<&str> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["source"].as_str().unwrap())
        .collect();
    assert!(sources.contains(&"dependabot"));
    assert!(sources.contains(&"codacy"));
}

#[cfg(unix)]
#[test]
fn an_allowlisted_skip_passes_and_the_report_carries_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 2006);
    write_policy(root, "allow_skips = [\"docs\"]\n");
    write_page(
        root,
        &json!({
            "total_count": 2,
            "check_runs": [
                {"id": 1, "name": "verify", "status": "completed", "conclusion": "success", "app": {"slug": "github-actions"}},
                {"id": 2, "name": "docs", "status": "completed", "conclusion": "skipped", "app": {"slug": "github-actions"}}
            ]
        }),
    );

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "checks", "2006", "--format", "json"]);
    assert_eq!(code, Some(0), "an allowlisted skip passes: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["verdict"], "PASS");
    assert_eq!(
        report["head_sha"],
        "abcd1234abcd1234abcd1234abcd1234abcd1234"
    );
    let docs = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "docs")
        .unwrap();
    assert_eq!(docs["source"], "github-actions");
    assert_eq!(docs["state"], "completed");
    assert_eq!(docs["verdict"], "SKIP");
    assert_eq!(docs["reason"], "allowlisted skip");
}

#[cfg(unix)]
#[test]
fn a_truncated_response_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 2007);
    // The API claims two runs but the page carries one.
    write_page(
        root,
        &json!({
            "total_count": 2,
            "check_runs": [{
                "id": 1,
                "name": "verify",
                "status": "completed",
                "conclusion": "success",
                "app": {"slug": "github-actions"}
            }]
        }),
    );

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "checks", "2007", "--format", "json"]);
    assert_eq!(
        code,
        Some(1),
        "a truncated response must not pass: {stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["verdict"], "UNKNOWN");
    assert_eq!(report["blockers"][0]["code"], "check_runs_truncated");
}

/// The 101st check lives on a second page. This proves the CLI asks `gh` for
/// every page (`--paginate --slurp`) and folds both pages into one verdict,
/// rather than reading a single 100-row response.
#[cfg(unix)]
#[test]
fn a_failure_on_the_second_page_is_collected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 2008);

    let first_page: Vec<Value> = (0..100)
        .map(|i| {
            json!({
                "id": i,
                "name": format!("check-{i}"),
                "status": "completed",
                "conclusion": "success",
                "app": {"slug": "github-actions"}
            })
        })
        .collect();
    let pages = json!([
        {"total_count": 101, "check_runs": first_page},
        {"total_count": 101, "check_runs": [{
            "id": 101,
            "name": "check-101",
            "status": "completed",
            "conclusion": "failure",
            "app": {"slug": "github-actions"}
        }]}
    ]);
    std::fs::write(root.join("check_runs_pages.json"), pages.to_string()).unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "checks", "2008", "--format", "json"]);
    assert_eq!(code, Some(1), "a second-page failure must fail: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["verdict"], "FAIL");
    assert_eq!(report["checks"].as_array().unwrap().len(), 101);
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "check-101" && c["verdict"] == "FAIL")
    );

    let flags = std::fs::read_to_string(root.join("check_runs_flags")).unwrap();
    assert!(
        flags.contains("--paginate"),
        "must request pagination: {flags}"
    );
    assert!(flags.contains("--slurp"), "must slurp every page: {flags}");
}
