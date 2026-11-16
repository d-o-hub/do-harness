//! Receipt lifetime, identity, and freshness checks.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::pr::readiness::reason::{RECEIPT_LIFETIME_SECONDS, RECEIPT_SCHEMA_VERSION};

use super::{MergeReceipt, ReceiptVerdict, validate_receipt};

#[test]
fn receipt_lifetime_is_sixty_seconds() {
    assert_eq!(RECEIPT_LIFETIME_SECONDS, 60);
}

#[test]
fn a_wrong_pr_number_is_an_invalid_receipt() {
    let refreshed = MergeReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        pr: 7,
        issued_at_unix: 100,
        expires_at_unix: 160,
        snapshot_sha256: "abc".to_owned(),
        base_ref_name: "main".to_owned(),
        base_ref_oid: "b".to_owned(),
        head_ref_name: "topic".to_owned(),
        head_ref_oid: "h".to_owned(),
        head_repository: Some("acme/widget".to_owned()),
        state: "OPEN".to_owned(),
        is_draft: false,
        labels: Vec::new(),
        mergeable: "MERGEABLE".to_owned(),
        merge_state_status: "CLEAN".to_owned(),
        auto_merge: false,
        review_decision: None,
        checks_sha256: String::new(),
        conversations_sha256: String::new(),
        findings_sha256: String::new(),
        policy_revision: "absent".to_owned(),
        policy_source: "overlay".to_owned(),
        ancestry: "ahead".to_owned(),
        ancestry_base_commit: "b".to_owned(),
        ancestry_merge_base: "b".to_owned(),
    };
    let mut supplied = refreshed.clone();
    supplied.pr = 8;
    assert!(matches!(
        validate_receipt(&supplied, &refreshed, 120),
        ReceiptVerdict::Invalid(_)
    ));
}

#[test]
fn an_expired_receipt_is_expired_not_changed() {
    let refreshed = MergeReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        pr: 7,
        issued_at_unix: 100,
        expires_at_unix: 160,
        snapshot_sha256: "abc".to_owned(),
        base_ref_name: "main".to_owned(),
        base_ref_oid: "b".to_owned(),
        head_ref_name: "topic".to_owned(),
        head_ref_oid: "h".to_owned(),
        head_repository: Some("acme/widget".to_owned()),
        state: "OPEN".to_owned(),
        is_draft: false,
        labels: Vec::new(),
        mergeable: "MERGEABLE".to_owned(),
        merge_state_status: "CLEAN".to_owned(),
        auto_merge: false,
        review_decision: None,
        checks_sha256: String::new(),
        conversations_sha256: String::new(),
        findings_sha256: String::new(),
        policy_revision: "absent".to_owned(),
        policy_source: "overlay".to_owned(),
        ancestry: "ahead".to_owned(),
        ancestry_base_commit: "b".to_owned(),
        ancestry_merge_base: "b".to_owned(),
    };
    assert_eq!(
        validate_receipt(&refreshed, &refreshed, 161),
        ReceiptVerdict::Expired
    );
    let mut moved = refreshed.clone();
    moved.snapshot_sha256 = "def".to_owned();
    assert_eq!(
        validate_receipt(&refreshed, &moved, 120),
        ReceiptVerdict::Changed
    );
}
