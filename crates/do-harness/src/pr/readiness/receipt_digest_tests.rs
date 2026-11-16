//! Digest-stability checks for the classified check set.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::pr::checks::{CheckEntry, Verdict};
use crate::pr::readiness::model::CheckSummary;

use super::checks_digest;

fn check_summary(verdict: &str) -> CheckSummary {
    CheckSummary {
        total: 1,
        passed: 1,
        failed: Vec::new(),
        cancelled: Vec::new(),
        pending: Vec::new(),
        skipped: 0,
        incomplete: false,
        verdict: verdict.to_owned(),
        head_sha: "h".to_owned(),
        unknown: Vec::new(),
        entries: vec![CheckEntry {
            name: "verify".to_owned(),
            source: "github-actions".to_owned(),
            app_id: None,
            kind: "check".to_owned(),
            state: "completed".to_owned(),
            conclusion: Some("success".to_owned()),
            verdict: Verdict::Pass,
            reason: None,
            rerun_command: None,
            url: None,
        }],
        blockers: Vec::new(),
    }
}

#[test]
fn checks_digest_changes_when_a_check_verdict_changes() {
    let mut summary = check_summary("PASS");
    let before = checks_digest(&summary);
    summary.verdict = "FAIL".to_owned();
    assert_ne!(before, checks_digest(&summary));
}
