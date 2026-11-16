//! Receipt lifetime, identity, and freshness checks.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::pr::readiness::reason::{
    MAX_RECEIPT_LIFETIME_SECONDS, RECEIPT_LIFETIME_SECONDS, RECEIPT_SCHEMA_VERSION,
};

use super::{MergeReceipt, ReceiptVerdict, receipt_lifetime_seconds, validate_receipt};

fn receipt() -> MergeReceipt {
    MergeReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        pr: 7,
        issued_at_unix: 100,
        expires_at_unix: 160,
        generation_seconds: 0,
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
    }
}

#[test]
fn receipt_lifetime_is_sixty_seconds() {
    assert_eq!(RECEIPT_LIFETIME_SECONDS, 60);
}

#[test]
fn generation_widens_the_lifetime_up_to_the_cap() {
    assert_eq!(receipt_lifetime_seconds(0), 60);
    assert_eq!(receipt_lifetime_seconds(-5), 60);
    assert_eq!(receipt_lifetime_seconds(29), 60);
    assert_eq!(receipt_lifetime_seconds(31), 62);
    assert_eq!(receipt_lifetime_seconds(120), 240);
    assert_eq!(receipt_lifetime_seconds(500), MAX_RECEIPT_LIFETIME_SECONDS);
}

#[test]
fn a_wrong_pr_number_is_an_invalid_receipt() {
    let refreshed = receipt();
    let mut supplied = refreshed.clone();
    supplied.pr = 8;
    assert!(matches!(
        validate_receipt(&supplied, &refreshed, 120),
        ReceiptVerdict::Invalid(_)
    ));
}

#[test]
fn an_expired_receipt_is_expired_not_changed() {
    let refreshed = receipt();
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

#[test]
fn a_generation_bounded_window_is_accepted_until_it_expires() {
    let refreshed = receipt();
    let mut supplied = refreshed.clone();
    supplied.expires_at_unix = 340;
    supplied.generation_seconds = 120;

    assert_eq!(
        validate_receipt(&supplied, &refreshed, 300),
        ReceiptVerdict::Fresh
    );
    assert_eq!(
        validate_receipt(&supplied, &refreshed, 341),
        ReceiptVerdict::Expired
    );
}

#[test]
fn a_lifetime_outside_the_bounded_range_is_invalid() {
    let refreshed = receipt();

    let mut short = refreshed.clone();
    short.expires_at_unix = 159;
    assert!(matches!(
        validate_receipt(&short, &refreshed, 120),
        ReceiptVerdict::Invalid(message) if message.contains("59s")
    ));

    let mut long = refreshed.clone();
    long.expires_at_unix = 701;
    assert!(matches!(
        validate_receipt(&long, &refreshed, 120),
        ReceiptVerdict::Invalid(message) if message.contains("601s")
    ));
}

#[test]
fn a_receipt_from_the_previous_schema_is_invalid() {
    let refreshed = receipt();
    let mut supplied = refreshed.clone();
    supplied.schema_version = 1;
    assert!(matches!(
        validate_receipt(&supplied, &refreshed, 120),
        ReceiptVerdict::Invalid(message)
            if message.contains("schema_version is 1 (expected 2)")
    ));
}
