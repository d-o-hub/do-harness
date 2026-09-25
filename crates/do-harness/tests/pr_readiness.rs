//! Acceptance fixtures for `do-harness pr <n>` merge readiness (#240).
//!
//! Validates:
//! 1. A cancelled CI check run is flagged with its exact rerun command and fails readiness.
//! 2. An unanswered Codecov comment reporting missing patch coverage is flagged as an actionable blocker.
//! 3. Once answered by a subsequent comment, the Codecov gap is no longer actionable.
//! 4. Direct invocation `do-harness pr <n>` routes cleanly to the readiness check.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

mod support;

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Creates a fake `gh` script that dispatches API and view queries based on
/// mock JSON data written in `mock_dir`.
fn fake_gh_router(mock_dir: &Path) -> PathBuf {
    let bin = mock_dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    let script_content = format!(
        r#"#!/bin/sh
MOCK_DIR="{}"
# gh pr view ...
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
    cat "$MOCK_DIR/view.json"
    exit 0
fi
# gh api ...
if [ "$1" = "api" ]; then
    case "$2" in
        *check-runs*)
            if [ -f "$MOCK_DIR/check_runs.json" ]; then
                cat "$MOCK_DIR/check_runs.json"
            else
                printf '%s' '{{"check_runs":[]}}'
            fi
            exit 0
            ;;
        *status*)
            if [ -f "$MOCK_DIR/status.json" ]; then
                cat "$MOCK_DIR/status.json"
            else
                printf '%s' '{{"statuses":[]}}'
            fi
            exit 0
            ;;
        *comments*)
            if [ -f "$MOCK_DIR/comments.json" ]; then
                cat "$MOCK_DIR/comments.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        graphql)
            if [ -f "$MOCK_DIR/graphql.json" ]; then
                cat "$MOCK_DIR/graphql.json"
            else
                printf '%s' '{{"data":{{"repository":{{"pullRequest":{{"reviewThreads":{{"nodes":[]}}}}}}}}}}'
            fi
            exit 0
            ;;
    esac
fi
echo "unknown fake gh call: $@" >&2
exit 1
"#,
        mock_dir.display()
    );
    std::fs::write(&script, script_content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

fn run_pr(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = harness(root)
        .args(args)
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

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

    std::fs::write(
        root.join("check_runs.json"),
        r#"{
            "check_runs": [
                {
                    "id": 1,
                    "name": "Storage Matrix (redis)",
                    "status": "completed",
                    "conclusion": "cancelled",
                    "html_url": "https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635041/job/987654321"
                },
                {
                    "id": 2,
                    "name": "test",
                    "status": "completed",
                    "conclusion": "success",
                    "html_url": "https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635041/job/111111111"
                }
            ]
        }"#,
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
fn unanswered_codecov_comment_is_actionable_blocker() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1042,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            }
        ]"###,
    )
    .unwrap();

    // With no subsequent comment, Codecov gap is actionable and blocks merge:
    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(code, Some(1));
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(true));
    assert_eq!(report["codecov"]["patch_coverage"], serde_json::json!(0.0));
    assert_eq!(report["codecov"]["missing_lines"], serde_json::json!(2));
    assert_eq!(
        report["conversations"]["actionable_comments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(stderr.contains("not ready to merge"));

    // Now developer replies with a waiver comment:
    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            },
            {
                "id": 102,
                "user": { "login": "developer" },
                "body": "Waiver: CLI entry point not covered by unit suite.",
                "created_at": "2026-09-25T10:05:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-2"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(
        code,
        Some(0),
        "answered Codecov comment must not block merge:\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(true));
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(false));
    assert_eq!(
        report["conversations"]["actionable_comments"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(report["blockers"].as_array().unwrap().len(), 0);
}
