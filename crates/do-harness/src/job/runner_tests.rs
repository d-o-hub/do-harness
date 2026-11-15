#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::finish_green_attempt;
use crate::job::receipt::{
    AttemptRecord, BlockedCode, ExecutorObservation, JobReceipt, JobState, RECEIPT_SCHEMA_VERSION,
    VerificationRecord,
};

#[test]
fn green_gate_at_deadline_persists_a_blocked_attempt() {
    let root = tempfile::tempdir().unwrap();
    let mut receipt = JobReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        job_id: "deadline-test".to_owned(),
        spec_digest: "spec-digest".to_owned(),
        task: "test deadline".to_owned(),
        revision: "HEAD".to_owned(),
        resolved_revision: "resolved".to_owned(),
        signal_set: "verification".to_owned(),
        isolation: "test-harness".to_owned(),
        executor: "sh".to_owned(),
        accounting: "unavailable".to_owned(),
        attempts_limit: 1,
        attempts_used: 1,
        started_at: 0,
        deadline_at: 10,
        state: JobState::Running,
        blocked: None,
        attempts: Vec::new(),
        applied_repairs: Vec::new(),
        workspace: "workspace".to_owned(),
        workspace_removed: false,
    };
    let attempt = AttemptRecord {
        attempt: 1,
        started_at: 1,
        finished_at: 10,
        executor: ExecutorObservation {
            exit_code: Some(0),
            timed_out: false,
            spawn_error: None,
            output_bytes: 0,
            output_truncated: false,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            report: None,
            report_error: None,
        },
        repairs_applied: Vec::new(),
        verification: Some(VerificationRecord {
            state: "green".to_owned(),
            reason: "current".to_owned(),
            failed: Vec::new(),
            unrunnable: Vec::new(),
        }),
        failure_signature: None,
        failure_set: Vec::new(),
        blocked: None,
    };

    finish_green_attempt(
        &mut receipt,
        root.path(),
        &root.path().join("workspace"),
        attempt,
        10,
    )
    .unwrap();

    let persisted = JobReceipt::load(root.path(), "deadline-test").unwrap();
    assert_eq!(persisted.state, JobState::Blocked);
    assert_eq!(
        persisted.blocked.as_ref().unwrap().code,
        BlockedCode::DeadlineExhausted
    );
    assert_eq!(persisted.attempts.len(), 1);
    assert_eq!(
        persisted.attempts[0].verification.as_ref().unwrap().state,
        "green"
    );
    assert_eq!(
        persisted.attempts[0].blocked.as_ref().unwrap().code,
        BlockedCode::DeadlineExhausted
    );
    assert!(!persisted.workspace_removed);
}
