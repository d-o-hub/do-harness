//! Unit tests for the `dora` module.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn percentile_is_nearest_rank() {
    let samples: Vec<i64> = (0..165).collect();
    assert_eq!(percentile(&samples, 50), Some(82));
    assert_eq!(percentile(&samples, 90), Some(148));
    assert_eq!(percentile(&[], 50), None);
    assert_eq!(percentile(&[7], 50), Some(7));
    assert_eq!(percentile(&[7], 90), Some(7));
}

#[test]
fn percentile_rank_uses_ceiling_division() {
    // n=3: p50 -> rank 2, p90 -> rank 3 (verified against the plan's
    // fixture expectations).
    assert_eq!(percentile(&[0, 100, 200], 50), Some(100));
    assert_eq!(percentile(&[0, 100, 200], 90), Some(200));
}

#[test]
fn resolve_now_prefers_the_explicit_clock() {
    assert_eq!(resolve_now(Some(42)), 42);
    assert!(resolve_now(None) > 1_700_000_000);
}

#[test]
fn incident_without_a_following_tag_is_unrestored() {
    let incident = incident_from(None, "f".to_string(), 200);
    assert_eq!(incident.restored_by, None);
    assert_eq!(incident.restore_ts, None);
}

#[test]
fn incident_is_restored_by_the_next_deploy_even_on_the_same_commit() {
    // A tag cut directly on the revert commit has deploy_ts == revert_ts;
    // it still restores the incident, because it is the deploy whose range
    // boundary closed the failure range.
    let next = git::Tag {
        name: "v0.1.2".to_string(),
        rev: "b".to_string(),
        deploy_ts: 200,
    };
    let incident = incident_from(Some(&next), "f".to_string(), 200);
    assert_eq!(incident.restored_by.as_deref(), Some("v0.1.2"));
    assert_eq!(incident.restore_ts, Some(200));
}
