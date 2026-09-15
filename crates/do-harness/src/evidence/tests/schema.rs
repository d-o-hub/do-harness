//! Schema serialization, unknown-field rejection, and legacy readability.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::*;

#[test]
fn serialization_matches_schema() {
    let doc = EvidenceDocument {
        schema_version: 1,
        tool: "do-harness".to_owned(),
        harness_version: "0.1.0".to_owned(),
        git_sha: Some("46463ef".into()),
        started_at: 1_755_852_762,
        finished_at: 1_755_852_810,
        root: "/abs/workspace".into(),
        task_id: None,
        sensor_pack: "rust".into(),
        signal_set: None,
        workspace_fingerprint: "sha256:w".into(),
        policy_fingerprint: "sha256:p".into(),
        config_fingerprint: "sha256:c".into(),
        changed: false,
        sensors: vec![EvidenceSensor {
            name: "check".into(),
            argv: vec!["cargo".into(), "check".into()],
            verdict: "pass".into(),
            exit_code: Some(0),
            duration_ms: Some(4200),
            output_sha256: "abc123".into(),
            artifacts: Vec::new(),
            recorded: true,
        }],
        skipped: Vec::new(),
        coverage: std::collections::BTreeMap::new(),
        summary: EvidenceSummary {
            pass: 1,
            fail: 0,
            skip: 0,
            verdict: "pass".into(),
        },
        prev_hash: None,
        chain_hash: "sealed-hash".into(),
    };

    let json = serde_json::to_string_pretty(&doc).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tool"], "do-harness");
    assert_eq!(value["sensors"][0]["verdict"], "pass");
}

/// Evidence types are a stability contract: stale payloads carrying
/// unknown fields are rejected at the deserialization boundary.
#[test]
fn unknown_fields_are_rejected() {
    let sensor = r#"{"name":"check","verdict":"pass","exit_code":0,
         "duration_ms":1,"recorded":false,"bogus":true}"#;
    assert!(serde_json::from_str::<EvidenceSensor>(sensor).is_err());

    let summary = r#"{"pass":1,"fail":0,"skip":0,"verdict":"pass","bogus":true}"#;
    assert!(serde_json::from_str::<EvidenceSummary>(summary).is_err());

    let document = r#"{"schema_version":3,"tool":"do-harness",
         "harness_version":"0.1.0","git_sha":null,"started_at":0,
         "finished_at":0,"root":"/","task_id":null,"sensor_pack":"rust",
         "workspace_fingerprint":"sha256:w","policy_fingerprint":"sha256:p",
         "config_fingerprint":"sha256:c","changed":false,
         "sensors":[],"skipped":[],"summary":{"pass":0,"fail":0,"skip":0,"verdict":"pass"},
         "bogus":true}"#;
    assert!(serde_json::from_str::<EvidenceDocument>(document).is_err());
}

/// Schema v2 payloads (no fingerprints) are rejected: status treats them as
/// legacy evidence, never as current.
#[test]
fn legacy_schema_without_fingerprints_is_rejected() {
    let document = r#"{"schema_version":2,"tool":"do-harness",
         "harness_version":"0.1.0","git_sha":null,"started_at":0,
         "finished_at":0,"root":"/","task_id":null,"sensor_pack":"rust",
         "sensors":[],"summary":{"pass":0,"fail":0,"skip":0,"verdict":"pass"},
         "chain_hash":"sealed-hash"}"#;
    assert!(serde_json::from_str::<EvidenceDocument>(document).is_err());
}

/// Schema v3 payloads parse into v4 with empty artifact and coverage fields.
#[test]
fn v3_payload_parses_with_v4_defaults() {
    let document = r#"{"schema_version":3,"tool":"do-harness",
         "harness_version":"0.1.0","git_sha":null,"started_at":0,
         "finished_at":0,"root":"/","task_id":null,"sensor_pack":"rust",
         "workspace_fingerprint":"sha256:w","policy_fingerprint":"sha256:p",
         "config_fingerprint":"sha256:c","changed":false,
         "sensors":[{"name":"check","argv":["true"],"verdict":"pass",
           "exit_code":0,"duration_ms":1,"output_sha256":"abc","recorded":true}],
         "skipped":[],"summary":{"pass":1,"fail":0,"skip":0,"verdict":"pass"},
         "chain_hash":"sealed-hash"}"#;
    let doc: EvidenceDocument = serde_json::from_str(document).expect("v3 must stay readable");
    assert_eq!(doc.schema_version, 3);
    assert!(doc.sensors[0].artifacts.is_empty());
    assert!(doc.coverage.is_empty());
    assert!(doc.is_strict_clean());
}
