//! Acceptance fixtures for `do-harness pr <n>` merge readiness (#240).
//!
//! Validates:
//! 1. A cancelled CI check run is flagged with its exact rerun command and fails readiness.
//! 2. Direct invocation `do-harness pr <n>` routes cleanly to the readiness check.
//! 3. An unreadable `gh` fails closed instead of reporting readiness.
//!
//! Conversation fixtures (unresolved threads, Codecov) live in
//! `tests/pr_readiness_conversations.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;

mod support;

use support::pr_readiness::{fake_gh_router, run_pr};

#[cfg(unix)]
#[test]
fn cancelled_check_fails_readiness_and_provides_rerun_command() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1041,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    // `gh api --paginate --jq '.check_runs[]'` streams one object per line.
    std::fs::write(
        root.join("check_runs.json"),
        r#"{"id": 1, "name": "Storage Matrix (redis)", "status": "completed", "conclusion": "cancelled", "html_url": "https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635041/job/987654321"}
{"id": 2, "name": "test", "status": "completed", "conclusion": "success", "html_url": "https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635041/job/111111111"}"#,
    )
    .unwrap();

    // 1. Text format via `do-harness pr ready 1041`:
    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "1041"]);
    assert_eq!(code, Some(1), "cancelled leg must fail readiness");
    assert!(stdout.contains("pr 1041: merge readiness: NOT READY"));
    assert!(
        stdout.contains(
            "CANCELLED: Storage Matrix (redis) -> gh run rerun 35885635041 --job 987654321"
        )
    );
    assert!(stdout.contains("check 'Storage Matrix (redis)' was CANCELLED; rerun with: gh run rerun 35885635041 --job 987654321"));
    assert!(stderr.contains("not ready to merge"));

    // 2. Direct invocation `do-harness pr 1041 --format json`:
    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1041", "--format", "json"]);
    assert_eq!(code, Some(1));
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    assert_eq!(report["checks"]["cancelled"].as_array().unwrap().len(), 1);
    let cancelled = &report["checks"]["cancelled"][0];
    assert_eq!(cancelled["name"], "Storage Matrix (redis)");
    assert_eq!(
        cancelled["rerun_command"],
        "gh run rerun 35885635041 --job 987654321"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_check_state_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1044,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();
    std::fs::write(root.join("fail_api"), "").unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "1044", "--format", "json"]);
    assert_eq!(
        code,
        Some(2),
        "a gh failure is an environment error, never a pass:\n{stderr}"
    );
    assert!(
        stderr.contains("could not read check runs"),
        "stderr must name the unreadable input: {stderr}"
    );
    assert!(
        !stdout.contains("ready"),
        "no readiness report may be emitted when the check state is unknown: {stdout}"
    );
}
