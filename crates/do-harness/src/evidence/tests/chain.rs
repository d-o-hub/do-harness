//! Hash-chain sealing and tamper detection.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::*;

#[test]
fn seal_and_verify_chain() {
    let mut doc = EvidenceDocument {
        schema_version: 2,
        tool: "do-harness".to_owned(),
        harness_version: "0.1.0".to_owned(),
        git_sha: Some("abc".into()),
        started_at: 1,
        finished_at: 2,
        root: "/tmp".into(),
        task_id: None,
        sensor_pack: "rust".into(),
        signal_set: None,
        workspace_fingerprint: "sha256:w".into(),
        policy_fingerprint: "sha256:p".into(),
        config_fingerprint: "sha256:c".into(),
        changed: false,
        sensors: vec![],
        skipped: Vec::new(),
        coverage: std::collections::BTreeMap::new(),
        summary: EvidenceSummary {
            pass: 0,
            fail: 0,
            skip: 0,
            verdict: "pass".into(),
        },
        prev_hash: None,
        chain_hash: String::new(),
    };
    doc.seal(None).unwrap();
    assert!(doc.verify_chain(None));
    let genesis = doc.chain_hash.clone();

    let mut next = doc.clone();
    next.seal(Some(genesis.clone())).unwrap();
    assert_eq!(next.prev_hash.as_deref(), Some(genesis.as_str()));
    assert!(next.verify_chain(Some(&genesis)));

    // Tampering with a hashed field invalidates the chain.
    next.git_sha = Some("tampered".into());
    assert!(!next.verify_chain(Some(&genesis)));
}
