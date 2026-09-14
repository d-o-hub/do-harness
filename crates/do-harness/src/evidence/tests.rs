#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn strict_clean_checks() {
    let doc = EvidenceDocument {
        schema_version: 1,
        tool: "do-harness".to_owned(),
        harness_version: "0.1.0".to_owned(),
        git_sha: Some("46463ef".into()),
        started_at: 100,
        finished_at: 200,
        root: "/tmp".into(),
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
            duration_ms: Some(10),
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
    assert!(doc.is_strict_clean());

    let mut doc_skip = doc.clone();
    doc_skip.summary.skip = 1;
    doc_skip.sensors.push(EvidenceSensor {
        name: "test".into(),
        argv: vec!["cargo".into(), "test".into()],
        verdict: "skip".into(),
        exit_code: None,
        duration_ms: None,
        output_sha256: String::new(),
        artifacts: Vec::new(),
        recorded: false,
    });
    assert!(!doc_skip.is_strict_clean());

    let mut doc_no_exit = doc.clone();
    doc_no_exit.sensors[0].exit_code = None;
    assert!(!doc_no_exit.is_strict_clean());
}

/// `allow_failure` softens the local gate only: evidence records the sensor
/// as `warn` and the summary stays non-pass, so `--strict` cannot bless a
/// weak run.
#[test]
fn soft_failure_is_recorded_as_warn_not_pass() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = crate::config::rust_default();
    cfg.sensors = vec![crate::config::SensorSpec {
        name: "links".into(),
        argv: vec!["true".into()],
        retry: None,
        timeout: None,
        severity: None,
        allow_failure: true,
        transient_exit_codes: vec![],
        artifacts: Vec::new(),
        coverage_inputs: Vec::new(),
        when_changed: vec![],
    }];
    let report = VerifyReport {
        ok: true,
        root: dir.path().display().to_string(),
        failed: vec![],
        signal_set: None,
        sensors: vec![crate::report::SensorResult {
            name: "links".into(),
            ok: false,
            exit_code: Some(1),
            duration_ms: 5,
            severity: crate::config::SensorSeverity::Warn,
            allow_failure: true,
            warned: false,
            findings: None,
            baseline: None,
            output: "boom".into(),
        }],
    };
    let meta = RunMeta {
        cfg: &cfg,
        root: dir.path(),
        set: None,
        selected: &["links".to_owned()],
        fingerprints: crate::fingerprint::Fingerprints {
            workspace: "sha256:w".into(),
            policy: "sha256:p".into(),
            config: "sha256:c".into(),
        },
        changed: false,
        skipped: Vec::new(),
        task: None,
        started_at: 0,
        finished_at: 1,
    };
    let doc = EvidenceDocument::from_run(&report, &meta);
    assert_eq!(doc.sensors[0].verdict, "warn");
    assert_eq!(doc.summary.fail, 1);
    assert_eq!(doc.summary.pass, 0);
    assert!(!doc.is_strict_clean());
}

/// A passing sensor that reported a `SKIP:` marker is recorded as `warn`:
/// the local gate stays green, but `--strict` and `status` reject it.
#[test]
fn warned_sensor_is_recorded_as_warn_and_fails_summary() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = crate::config::rust_default();
    cfg.sensors = vec![crate::config::SensorSpec {
        name: "tool".into(),
        argv: vec!["true".into()],
        retry: None,
        timeout: None,
        severity: None,
        allow_failure: false,
        transient_exit_codes: vec![],
        artifacts: Vec::new(),
        coverage_inputs: Vec::new(),
        when_changed: vec![],
    }];
    let report = VerifyReport {
        ok: true,
        root: dir.path().display().to_string(),
        failed: vec![],
        signal_set: None,
        sensors: vec![crate::report::SensorResult {
            name: "tool".into(),
            ok: true,
            exit_code: Some(0),
            duration_ms: 5,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: true,
            findings: None,
            baseline: None,
            output: "SKIP: tool missing".into(),
        }],
    };
    let meta = RunMeta {
        cfg: &cfg,
        root: dir.path(),
        set: None,
        selected: &["tool".to_owned()],
        fingerprints: crate::fingerprint::Fingerprints {
            workspace: "sha256:w".into(),
            policy: "sha256:p".into(),
            config: "sha256:c".into(),
        },
        changed: false,
        skipped: Vec::new(),
        task: None,
        started_at: 0,
        finished_at: 1,
    };
    let doc = EvidenceDocument::from_run(&report, &meta);
    assert_eq!(doc.sensors[0].verdict, "warn");
    assert_eq!(doc.summary.pass, 0);
    assert_eq!(doc.summary.fail, 1);
    assert_eq!(doc.summary.verdict, "fail");
    assert!(!doc.is_strict_clean());
}

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

/// Sealing links documents and `verify_chain` detects tampering.
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

/// Declared artifacts are digested and `COVERAGE:` markers are recorded.
#[test]
fn artifacts_and_coverage_are_recorded() {
    use sha2::Digest as _;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("out")).unwrap();
    std::fs::write(dir.path().join("out/report.txt"), b"proof").unwrap();

    let mut cfg = crate::config::rust_default();
    cfg.sensors = vec![crate::config::SensorSpec {
        name: "web".into(),
        argv: vec!["true".into()],
        retry: None,
        timeout: None,
        severity: None,
        allow_failure: false,
        transient_exit_codes: vec![],
        when_changed: vec![],
        artifacts: vec!["out/*.txt".into()],
        coverage_inputs: vec![],
    }];
    let report = VerifyReport {
        ok: true,
        root: dir.path().display().to_string(),
        failed: vec![],
        signal_set: None,
        sensors: vec![crate::report::SensorResult {
            name: "web".into(),
            ok: true,
            exit_code: Some(0),
            duration_ms: 3,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            output: "COVERAGE: {\"routes\":2,\"viewports\":3}".into(),
        }],
    };
    let meta = RunMeta {
        cfg: &cfg,
        root: dir.path(),
        set: None,
        selected: &["web".to_owned()],
        fingerprints: crate::fingerprint::Fingerprints {
            workspace: "sha256:w".into(),
            policy: "sha256:p".into(),
            config: "sha256:c".into(),
        },
        changed: false,
        skipped: Vec::new(),
        task: None,
        started_at: 0,
        finished_at: 1,
    };
    let doc = EvidenceDocument::from_run(&report, &meta);
    assert_eq!(doc.schema_version, 4);
    assert_eq!(doc.sensors[0].artifacts.len(), 1);
    assert_eq!(doc.sensors[0].artifacts[0].path, "out/report.txt");
    assert_eq!(
        doc.sensors[0].artifacts[0].sha256,
        hex::encode(sha2::Sha256::digest(b"proof"))
    );
    assert_eq!(
        doc.coverage.get("web"),
        Some(&serde_json::json!({"routes": 2, "viewports": 3}))
    );
    assert_eq!(doc.summary.verdict, "pass");
}

/// A declared artifact glob that matches nothing degrades the sensor to warn.
#[test]
fn missing_declared_artifact_records_warn() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = crate::config::rust_default();
    cfg.sensors = vec![crate::config::SensorSpec {
        name: "web".into(),
        argv: vec!["true".into()],
        retry: None,
        timeout: None,
        severity: None,
        allow_failure: false,
        transient_exit_codes: vec![],
        when_changed: vec![],
        artifacts: vec!["out/*.png".into()],
        coverage_inputs: vec![],
    }];
    let report = VerifyReport {
        ok: true,
        root: dir.path().display().to_string(),
        failed: vec![],
        signal_set: None,
        sensors: vec![crate::report::SensorResult {
            name: "web".into(),
            ok: true,
            exit_code: Some(0),
            duration_ms: 3,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            output: String::new(),
        }],
    };
    let meta = RunMeta {
        cfg: &cfg,
        root: dir.path(),
        set: None,
        selected: &["web".to_owned()],
        fingerprints: crate::fingerprint::Fingerprints {
            workspace: "sha256:w".into(),
            policy: "sha256:p".into(),
            config: "sha256:c".into(),
        },
        changed: false,
        skipped: Vec::new(),
        task: None,
        started_at: 0,
        finished_at: 1,
    };
    let doc = EvidenceDocument::from_run(&report, &meta);
    assert_eq!(doc.sensors[0].verdict, "warn");
    assert_eq!(doc.summary.verdict, "fail");
    assert!(!doc.is_strict_clean());
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
