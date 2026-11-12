//! Acceptance fixtures for merge authorization and the receipt boundary (#466).
//!
//! Each case drives the real binary against a fake `gh`, so the authorizing
//! object, the exit codes, and the receipt revalidation are all exercised
//! without network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};
#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::pr_readiness::{fake_gh_router, run_pr};
#[cfg(unix)]
use support::pr_readiness_fixtures::{BASE, HEAD, green_checks, reason_codes, write_view};

#[cfg(unix)]
#[cfg(unix)]
#[test]
fn a_clean_green_pull_request_is_merge_allowed_with_a_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4001, &json!({}));
    green_checks(root);

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "4001", "--format", "json"]);
    assert_eq!(
        code,
        Some(0),
        "a clean PR must authorize: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["ready"], true);
    assert_eq!(report["authorization"]["decision"], "MERGE_ALLOWED");
    let receipt = &report["authorization"]["receipt"];
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["pr"], 4001);
    assert_eq!(receipt["head_ref_oid"], HEAD);
    assert_eq!(receipt["ancestry"], "ahead");
    assert_eq!(
        receipt["expires_at_unix"].as_i64().unwrap() - receipt["issued_at_unix"].as_i64().unwrap(),
        60,
        "the receipt lifetime is exactly 60 seconds"
    );
    assert_eq!(receipt["snapshot_sha256"].as_str().unwrap().len(), 64);
}

#[cfg(unix)]
#[test]
fn draft_and_held_and_closed_are_blocked_not_unknown() {
    let cases = [
        (4002, json!({"isDraft": true}), "draft"),
        (4003, json!({"labels": [{"name": "HOLD"}]}), "held"),
        (4004, json!({"state": "CLOSED"}), "state_not_open"),
        (
            4005,
            json!({"mergeStateStatus": "BEHIND"}),
            "merge_state_dirty",
        ),
        (
            4006,
            json!({"autoMergeRequest": {"enabledAt": "now"}}),
            "auto_merge_armed",
        ),
    ];

    for (pr, overrides, expected) in cases {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let bin = fake_gh_router(root);
        write_view(root, pr, &overrides);
        green_checks(root);

        let (code, stdout, stderr) = run_pr(
            root,
            &bin,
            &["pr", "ready", &pr.to_string(), "--format", "json"],
        );
        assert_eq!(code, Some(1), "PR {pr} must be blocked: {stderr}\n{stdout}");
        let report: Value = serde_json::from_str(&stdout).expect("JSON report");
        assert_eq!(report["authorization"]["decision"], "BLOCKED", "PR {pr}");
        assert!(
            report["authorization"]["receipt"].is_null(),
            "no receipt for PR {pr}"
        );
        let codes = reason_codes(&stdout);
        assert!(
            codes.iter().any(|code| code == expected),
            "PR {pr} must report '{expected}', got {codes:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_missing_base_or_head_sha_is_unknown_and_never_a_pass() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4007, &json!({"baseRefOid": ""}));
    green_checks(root);

    let (code, stdout, _stderr) = run_pr(root, &bin, &["pr", "ready", "4007", "--format", "json"]);
    assert_eq!(code, Some(2), "an incomplete payload cannot be judged");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["ready"], false);
    assert_eq!(report["authorization"]["decision"], "UNKNOWN");
    assert!(
        reason_codes(&stdout)
            .iter()
            .any(|code| code == "api_incomplete")
    );
}

#[cfg(unix)]
#[test]
fn diverged_ancestry_blocks_even_when_github_says_mergeable() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4008, &json!({}));
    green_checks(root);
    // The compare API reports a head that does not contain the captured base.
    std::fs::write(
        root.join("compare_override.json"),
        json!({
            "status": "diverged",
            "base_commit": {"sha": BASE},
            "merge_base_commit": {"sha": "3333333333333333333333333333333333333333"}
        })
        .to_string(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "4008", "--format", "json"]);
    assert_eq!(code, Some(1), "diverged ancestry must block: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["ready"], false);
    assert_eq!(report["merge_state"]["mergeable"], "MERGEABLE");
    assert!(
        reason_codes(&stdout)
            .iter()
            .any(|c| c == "ancestry_diverged")
    );
}

#[cfg(unix)]
fn authorize_and_capture(root: &Path, bin: &Path, pr: u64) -> String {
    let (code, stdout, stderr) = run_pr(
        root,
        bin,
        &["pr", "ready", &pr.to_string(), "--format", "json"],
    );
    assert_eq!(
        code,
        Some(0),
        "authorization must succeed: {stderr}\n{stdout}"
    );
    stdout
}

#[cfg(unix)]
#[test]
fn a_fresh_receipt_is_accepted_and_a_moved_snapshot_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4009, &json!({}));
    green_checks(root);

    let report = authorize_and_capture(root, &bin, 4009);
    let receipt_path = root.join("receipt.json");
    std::fs::write(&receipt_path, &report).unwrap();

    let (code, _stdout, stderr) = run_pr(
        root,
        &bin,
        &[
            "pr",
            "ready",
            "4009",
            "--expected-receipt",
            receipt_path.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(0),
        "an unchanged snapshot accepts its receipt: {stderr}"
    );

    // A new check changes the snapshot while the receipt stays put. The
    // required contexts keep their green state, so this is a moved snapshot
    // rather than a newly-unknown suite: the receipt must catch the difference
    // on the check set alone.
    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 4,
            "check_runs": [
                {"id": 1, "name": "verify", "status": "completed", "conclusion": "success", "app": {"id": 15368, "slug": "github-actions"}},
                {"id": 2, "name": "pr-required", "status": "completed", "conclusion": "success", "app": {"id": 15368, "slug": "github-actions"}},
                {"id": 3, "name": "semver", "status": "completed", "conclusion": "success", "app": {"id": 15368, "slug": "github-actions"}},
                {"id": 4, "name": "docs", "status": "completed", "conclusion": "success", "app": {"id": 15368, "slug": "github-actions"}}
            ]
        })
        .to_string(),
    )
    .unwrap();

    let (code, stdout, _stderr) = run_pr(
        root,
        &bin,
        &[
            "pr",
            "ready",
            "4009",
            "--expected-receipt",
            receipt_path.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(2),
        "a changed snapshot must not reuse the receipt"
    );
    assert!(reason_codes(&stdout).iter().any(|c| c == "receipt_changed"));
}

#[cfg(unix)]
#[test]
fn an_expired_or_forged_or_foreign_receipt_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4010, &json!({}));
    green_checks(root);

    let report = authorize_and_capture(root, &bin, 4010);
    let mut document: Value = serde_json::from_str(&report).unwrap();

    // 1. Expired: timestamps pushed into the past, snapshot left intact.
    let mut expired = document.clone();
    let issued = expired["authorization"]["receipt"]["issued_at_unix"]
        .as_i64()
        .unwrap();
    expired["authorization"]["receipt"]["issued_at_unix"] = json!(issued - 1000);
    expired["authorization"]["receipt"]["expires_at_unix"] = json!(issued - 940);
    let expired_path = root.join("expired.json");
    std::fs::write(&expired_path, expired.to_string()).unwrap();

    let (code, stdout, _stderr) = run_pr(
        root,
        &bin,
        &[
            "pr",
            "ready",
            "4010",
            "--expected-receipt",
            expired_path.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(2));
    assert!(
        reason_codes(&stdout).iter().any(|c| c == "receipt_expired"),
        "{stdout}"
    );

    // 2. A lifetime longer than the contract is invalid, not merely expired.
    let mut long_lived = document.clone();
    long_lived["authorization"]["receipt"]["expires_at_unix"] = json!(issued + 3600);
    let long_path = root.join("long.json");
    std::fs::write(&long_path, long_lived.to_string()).unwrap();
    let (code, stdout, _stderr) = run_pr(
        root,
        &bin,
        &[
            "pr",
            "ready",
            "4010",
            "--expected-receipt",
            long_path.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(2));
    assert!(
        reason_codes(&stdout).iter().any(|c| c == "receipt_invalid"),
        "{stdout}"
    );

    // 3. A standalone receipt document is not a readiness report.
    let standalone = document["authorization"]["receipt"].clone();
    let standalone_path = root.join("standalone.json");
    std::fs::write(&standalone_path, standalone.to_string()).unwrap();
    let (code, stdout, _stderr) = run_pr(
        root,
        &bin,
        &[
            "pr",
            "ready",
            "4010",
            "--expected-receipt",
            standalone_path.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(2));
    assert!(
        reason_codes(&stdout).iter().any(|c| c == "receipt_invalid"),
        "{stdout}"
    );

    // 4. A receipt for another PR is invalid.
    document["authorization"]["receipt"]["pr"] = json!(9999);
    let foreign_path = root.join("foreign.json");
    std::fs::write(&foreign_path, document.to_string()).unwrap();
    let (code, stdout, _stderr) = run_pr(
        root,
        &bin,
        &[
            "pr",
            "ready",
            "4010",
            "--expected-receipt",
            foreign_path.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(2));
    assert!(
        reason_codes(&stdout).iter().any(|c| c == "receipt_invalid"),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn changes_requested_and_unresolved_threads_prevent_a_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4011, &json!({"reviewDecision": "CHANGES_REQUESTED"}));
    green_checks(root);

    let (code, stdout, _stderr) = run_pr(root, &bin, &["pr", "ready", "4011", "--format", "json"]);
    assert_eq!(code, Some(1));
    assert!(
        reason_codes(&stdout)
            .iter()
            .any(|c| c == "review_changes_requested")
    );
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert!(report["authorization"]["receipt"].is_null());
}
