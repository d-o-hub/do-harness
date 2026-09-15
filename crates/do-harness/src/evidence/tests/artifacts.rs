//! Declared artifact digests and `COVERAGE:` marker recording.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::*;

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
