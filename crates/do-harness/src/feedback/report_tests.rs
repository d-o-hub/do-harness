//! Tests for `crates/do-harness/src/feedback/report.rs`, split out to keep that file under the 500-line ceiling.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn mode_parsing_defaults_to_internal() {
    assert_eq!(parse_mode(None), AutoReport::Internal);
    assert_eq!(parse_mode(Some("")), AutoReport::Internal);
    assert_eq!(parse_mode(Some("off")), AutoReport::Off);
    assert_eq!(parse_mode(Some("0")), AutoReport::Off);
    assert_eq!(parse_mode(Some("false")), AutoReport::Off);
    assert_eq!(parse_mode(Some("1")), AutoReport::Internal);
    assert_eq!(parse_mode(Some(" TRUE ")), AutoReport::Internal);
    assert_eq!(parse_mode(Some("internal")), AutoReport::Internal);
    assert_eq!(parse_mode(Some("all")), AutoReport::All);
}

#[test]
fn internal_mode_skips_usage_and_sensor_failures() {
    assert!(!should_report(AutoReport::Off, FailureClass::Internal));
    assert!(should_report(AutoReport::Internal, FailureClass::Internal));
    assert!(!should_report(AutoReport::Internal, FailureClass::Usage));
    assert!(!should_report(AutoReport::Internal, FailureClass::Verify));
    assert!(should_report(AutoReport::All, FailureClass::Usage));
    assert!(should_report(AutoReport::All, FailureClass::Verify));
    // An in-flight verify is an expected state, never a defect.
    assert!(!should_report(AutoReport::Internal, FailureClass::Running));
    assert!(!should_report(AutoReport::All, FailureClass::Running));
}

#[test]
fn draft_carries_details_and_redacts_secrets() {
    let redactor = Redactor::synthetic(&["s3cr3t-value"]);
    let argv = vec![
        "do-harness".to_owned(),
        "verify".to_owned(),
        "--only".to_owned(),
        "clippy".to_owned(),
    ];
    let draft = build_draft(
        "acme/widget",
        "error",
        "boom: token s3cr3t-value rejected",
        &argv,
        Some(2),
        "log line\ns3cr3t-value\n",
        &redactor,
        1_700_000_000,
    );
    assert_eq!(draft.target_repo, "acme/widget");
    assert!(draft.title.starts_with("[harness:cli] error: boom"));
    assert!(draft.body.contains("do-harness verify --only clippy"));
    assert!(draft.body.contains("Exit code: `2`"));
    assert!(draft.body.contains("log line"));
    assert!(!draft.body.contains("s3cr3t-value"));
    assert!(draft.body.contains(&draft.fingerprint));
    assert_eq!(draft.created_at, 1_700_000_000);
}

#[test]
fn fingerprint_ignores_volatile_tokens() {
    let a = fingerprint(
        "do-harness status",
        "verify is in flight (pid 9539, since 1793874401)",
    );
    let b = fingerprint(
        "do-harness status",
        "verify is in flight (pid 9412, since 1793874399)",
    );
    assert_eq!(a, b);
    assert_ne!(a, fingerprint("do-harness status", "a different failure"));
    assert_eq!(a.len(), FINGERPRINT_LEN);
}

#[test]
fn normalization_strips_volatile_tokens() {
    assert_eq!(normalize("pid 9539, since 1793874401"), "pid #, since #");
    assert_eq!(normalize("commit 3c5f20d"), "commit #");
    assert_eq!(normalize("clippy failed"), "clippy failed");
}

#[test]
fn draft_is_implementation_ready() {
    let redactor = Redactor::synthetic(&[]);
    let argv = vec!["do-harness".to_owned(), "verify".to_owned()];
    let draft = build_draft(
        "acme/widget",
        "error",
        "boom",
        &argv,
        Some(1),
        "",
        &redactor,
        0,
    );
    for section in [
        "## Summary",
        "## Impact",
        "## Steps to reproduce",
        "## Expected behavior",
        "## Actual behavior",
        "## Diagnostics",
        "## Acceptance criteria",
    ] {
        assert!(draft.body.contains(section), "missing {section}");
    }
}

#[test]
fn long_text_is_clipped_on_a_char_boundary() {
    let text = "é".repeat(10);
    let clipped = clip(&text, 5);
    assert!(clipped.len() <= 5);
    assert!(text.starts_with(&clipped));
    let excerpted = excerpt(&text, 19);
    assert!(excerpted.len() <= 19);
    assert!(excerpted.ends_with("... [truncated]"));
}

#[test]
fn log_tail_reads_only_the_bounded_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.log");
    let mut content = "x".repeat(MAX_LOG_BYTES);
    content.push_str("TAIL");
    std::fs::write(&path, content).unwrap();

    let tail = read_log_tail(&path).unwrap();
    assert_eq!(tail.len(), MAX_LOG_BYTES);
    assert!(tail.ends_with("TAIL"));
}
