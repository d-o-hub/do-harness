//! Strict-cleanliness verdict tests: pass/warn accounting and the strict gate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::*;

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
