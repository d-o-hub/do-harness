//! Tests for `policy.rs`, extracted to keep that file under the 450-line decomposition threshold.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn policy() -> DoraPolicy {
    serde_json::from_str(
        r#"{
          "window_days": 30,
          "tag_glob": "refs/tags/v*",
          "merge_strategy": "squash",
          "percentile_method": "nearest-rank",
          "revert_pattern": "^revert(\\(|:)",
          "bot_allowlist": [],
          "min_deploys": 1,
          "max_lead_p90_days": 30,
          "max_change_failure_rate": 0.15,
          "max_mttr_hours": 24,
          "max_unrestored": 0
        }"#,
    )
    .unwrap()
}

fn snapshot() -> DoraSnapshot {
    serde_json::from_str(
        r#"{
          "window_days": 30, "window_start": 0, "window_end": 0,
          "source": "git", "source_rev": "deadbeef",
          "deploy_count": 2, "deploys_failed": 1, "deploy_tags": [],
          "lead_samples": 165, "lead_p50_seconds": 865179,
          "lead_p90_seconds": 2335088, "mttr_seconds": null,
          "mttr_restored": 0, "mttr_unrestored": 1, "breaches": [],
          "policy_fingerprint": "sha256:x",
          "derivation": {
            "tag_glob": "refs/tags/v*", "merge_strategy": "squash",
            "percentile_method": "nearest-rank",
            "revert_pattern": "^revert(\\(|:)", "window_days": 30,
            "policy_digest": "abc", "clock_skew_commits": 0,
            "ranges": [], "incidents": []
          }
        }"#,
    )
    .unwrap()
}

#[test]
fn rate_ceiling_rounds_to_basis_points() {
    assert_eq!(policy().max_change_failure_rate_bp(), 1500);
}

#[test]
fn rate_breach_is_exact_integer_cross_multiplication() {
    // 1/7 = 0.142857… is below a 0.15 ceiling; 1/6 is above it.
    let mut snap = snapshot();
    snap.deploy_count = 7;
    snap.deploys_failed = 1;
    snap.lead_p90_seconds = Some(0);
    snap.mttr_unrestored = 0;
    assert!(breaches(&snap, &policy()).is_empty());
    snap.deploy_count = 6;
    assert_eq!(
        breaches(&snap, &policy())
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>(),
        vec!["change_failure_rate"]
    );
}

#[test]
fn unrestored_incidents_report_once_and_not_as_mttr() {
    let names: Vec<String> = breaches(&snapshot(), &policy())
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert_eq!(names, vec!["change_failure_rate", "unrestored_deploys"]);
}

#[test]
fn zero_incidents_report_no_restore_failure() {
    let mut snap = snapshot();
    snap.deploy_count = 1;
    snap.deploys_failed = 0;
    snap.lead_p90_seconds = Some(0);
    snap.mttr_unrestored = 0;
    assert!(breaches(&snap, &policy()).is_empty());
}

#[test]
fn restored_incidents_report_mttr() {
    let mut snap = snapshot();
    snap.deploy_count = 1;
    snap.deploys_failed = 0;
    snap.lead_p90_seconds = Some(0);
    snap.mttr_unrestored = 0;
    snap.mttr_restored = 1;
    snap.mttr_seconds = Some(2 * SECONDS_PER_DAY);
    let names: Vec<String> = breaches(&snap, &policy())
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert_eq!(names, vec!["mttr"]);
}

#[test]
fn missing_lead_p90_is_a_breach() {
    let mut snap = snapshot();
    snap.deploy_count = 1;
    snap.deploys_failed = 0;
    snap.lead_p90_seconds = None;
    snap.mttr_unrestored = 0;
    assert_eq!(
        breaches(&snap, &policy())
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>(),
        vec!["lead_p90"]
    );
}

#[test]
fn unsupported_policy_values_are_rejected() {
    let mut p = policy();
    p.revert_pattern = "^revert".to_string();
    assert!(p.validate().is_err());
    let mut p = policy();
    p.percentile_method = "linear".to_string();
    assert!(p.validate().is_err());
    assert!(policy().validate().is_ok());
}

#[test]
fn undocumented_unknown_field_is_rejected() {
    assert!(serde_json::from_str::<DoraPolicy>(r#"{"window_days": 1, "bogus": 2}"#).is_err());
}
