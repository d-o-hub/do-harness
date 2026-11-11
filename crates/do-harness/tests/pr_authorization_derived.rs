//! Derived-mode merge authorization: no local overlay, so the server decides (#515).
//!
//! These cases drive the real binary against a fake `gh` that deliberately has
//! no `plans/pr-merge-policy.toml`, which is the adopting-repository shape.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};
#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::pr_readiness::{fake_gh_router, run_pr_without_policy};
#[cfg(unix)]
use support::pr_readiness_fixtures::{reason_codes, ruleset_checks, write_view};

// Each case below asserts one acceptance criterion from #515.

#[cfg(unix)]
#[test]
fn a_repository_without_a_local_policy_derives_the_rules_and_authorizes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4020, &json!({}));
    ruleset_checks(root, 15368);

    let (code, stdout, stderr) =
        run_pr_without_policy(root, &bin, &["pr", "ready", "4020", "--format", "json"]);
    assert_eq!(
        code,
        Some(0),
        "an adopter with no overlay must still authorize: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["authorization"]["decision"], "MERGE_ALLOWED");
    let receipt = &report["authorization"]["receipt"];
    assert_eq!(receipt["policy_source"], "derived");
    // The receipt pins the derived rules digest, not a policy file hash.
    let revision = receipt["policy_revision"].as_str().expect("revision");
    assert!(
        revision.starts_with("sha256:"),
        "the derived revision is a rules digest, got {revision}"
    );
    assert_eq!(revision.len(), "sha256:".len() + 64);
}

#[cfg(unix)]
#[test]
fn the_overlay_still_does_not_govern_a_check_published_by_the_wrong_app() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4021, &json!({}));
    // Green and present, but from an app the ruleset did not name.
    ruleset_checks(root, 99999);

    let (code, stdout, _stderr) =
        run_pr_without_policy(root, &bin, &["pr", "ready", "4021", "--format", "json"]);
    assert_eq!(code, Some(1), "impersonation must block: {stdout}");
    let codes = reason_codes(&stdout);
    assert!(
        codes.iter().any(|c| c == "required_check_app_mismatch"),
        "expected the impersonation blocker, got {codes:?}"
    );
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert!(report["authorization"]["receipt"].is_null());
}

#[cfg(unix)]
#[test]
fn an_unreadable_ruleset_stays_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4022, &json!({}));
    ruleset_checks(root, 15368);
    // The repository endpoint fails, so the repository's merge settings (and
    // therefore the derived rules) cannot be established.
    std::fs::write(root.join("fail_repo_settings"), "").unwrap();

    let (code, stdout, _stderr) =
        run_pr_without_policy(root, &bin, &["pr", "ready", "4022", "--format", "json"]);
    assert_eq!(code, Some(2), "an unreadable ruleset is exit 2: {stdout}");
    let codes = reason_codes(&stdout);
    assert!(
        codes.iter().any(|c| c == "policy_unavailable"),
        "expected policy_unavailable, got {codes:?}"
    );
}
