//! Integration tests for impact-gated feedback and automated submission (WF-09).

#![allow(clippy::unwrap_used, clippy::expect_used)]
// The fake `gh` is a bash script, so these fixtures are Unix-only by
// construction (the Windows runner spawns `gh` directly and cannot exec a shebang).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const EXEC_PERMS: u32 = 0o755;
const MIN_INDEPENDENT_FIRES: usize = 3;
const SAMPLE_TIMESTAMP: i64 = 1_700_000_000;
const MOCK_ISSUE_NUMBER: u64 = 99;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    bin_dir: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let fake_gh = bin_dir.join("gh");
    let script = format!(
        r#"#!/bin/sh
ROOT="{}"
case "$1" in
    issue)
        case "$2" in
            list)
                if [ -f "$ROOT/fail_gh" ]; then
                    echo "gh: authentication failed or rate limited" >&2
                    exit 1
                fi
                if [ -f "$ROOT/mock_issues.json" ]; then
                    cat "$ROOT/mock_issues.json"
                else
                    echo "[]"
                fi
                exit 0
                ;;
            create)
                if [ -f "$ROOT/fail_gh" ]; then
                    echo "gh: authentication failed or rate limited" >&2
                    exit 1
                fi
                cat - > "$ROOT/last_created_body.txt"
                echo "$@" > "$ROOT/last_created_argv.txt"
                echo "https://github.com/d-o-hub/do-harness/issues/42"
                exit 0
                ;;
        esac
        ;;
esac
echo "unknown fake gh invocation: $@" >&2
exit 1
"#,
        root.display()
    );
    fs::write(&fake_gh, script).unwrap();
    let mut perms = fs::metadata(&fake_gh).unwrap().permissions();
    perms.set_mode(EXEC_PERMS);
    fs::set_permissions(&fake_gh, perms).unwrap();

    fs::write(
        root.join("do-harness.toml"),
        "language = \"rust\"\n[[sensors]]\nname = \"clippy\"\nargv = [\"cargo\", \"clippy\"]\n",
    )
    .unwrap();

    Fixture {
        _temp: temp,
        root,
        bin_dir,
    }
}

fn harness(fx: &Fixture) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(&fx.root);
    let old_path = std::env::var("PATH").unwrap_or_default();
    cmd.env("PATH", format!("{}:{}", fx.bin_dir.display(), old_path));
    cmd
}

#[tokio::test(flavor = "current_thread")]
async fn test_zero_candidates_from_routine_passes_or_under_threshold() {
    let fx = fixture();
    let conn = do_harness_db::connect_and_migrate(&fx.root).await.unwrap();

    // 2 fires (below minimum default of 3)
    do_harness_db::bump_error_signature(&conn, "sensor:clippy", "branch:main", Some("error 1"))
        .await
        .unwrap();
    do_harness_db::bump_error_signature(&conn, "sensor:clippy", "branch:main", Some("error 2"))
        .await
        .unwrap();

    let out = harness(&fx)
        .args([
            "feedback",
            "draft",
            "--repo",
            "d-o-hub/do-harness",
            "--format",
            "json",
        ])
        .output()
        .expect("run feedback draft");

    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let candidates = parsed["candidates"].as_array().unwrap();
    assert_eq!(
        candidates.len(),
        0,
        "under-threshold fires must produce 0 candidates"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn test_candidate_generated_from_three_independent_fires() {
    let fx = fixture();
    let conn = do_harness_db::connect_and_migrate(&fx.root).await.unwrap();

    for i in 1..=MIN_INDEPENDENT_FIRES {
        do_harness_db::bump_error_signature(
            &conn,
            "sensor:clippy",
            "branch:main",
            Some(&format!("compile fail {i}")),
        )
        .await
        .unwrap();
    }

    let out = harness(&fx)
        .args([
            "feedback",
            "draft",
            "--repo",
            "d-o-hub/do-harness",
            "--format",
            "json",
        ])
        .output()
        .expect("run feedback draft");

    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let candidates = parsed["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    let draft = &candidates[0];
    assert_eq!(draft["target_repo"], "d-o-hub/do-harness");
    assert_ne!(draft["fingerprint"].as_str().unwrap(), "");
    assert!(
        draft["body"]
            .as_str()
            .unwrap()
            .contains("<!-- do-harness-feedback-fingerprint:")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn test_severe_incident_qualifies_without_recurrence() {
    let fx = fixture();
    let conn = do_harness_db::connect_and_migrate(&fx.root).await.unwrap();

    // 1 single critical crash/panic signature
    do_harness_db::bump_error_signature(
        &conn,
        "panic:internal_assertion_failed",
        "branch:main",
        Some("thread panicked at 'unexpected state'"),
    )
    .await
    .unwrap();

    let out = harness(&fx)
        .args([
            "feedback",
            "draft",
            "--repo",
            "d-o-hub/do-harness",
            "--format",
            "json",
        ])
        .output()
        .expect("run feedback draft");

    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let candidates = parsed["candidates"].as_array().unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "severe incident qualifies without recurrence"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn test_duplicate_beat_and_event_counts_once() {
    let fx = fixture();
    let conn = do_harness_db::connect_and_migrate(&fx.root).await.unwrap();

    // 2 fires in DB
    do_harness_db::bump_error_signature(&conn, "sensor:clippy", "branch:main", Some("fire 1"))
        .await
        .unwrap();
    do_harness_db::bump_error_signature(&conn, "sensor:clippy", "branch:main", Some("fire 2"))
        .await
        .unwrap();

    // Write duplicate event with matching timestamp/signature
    let event_dir = fx.root.join(".agents/events/2026/10/29");
    fs::create_dir_all(&event_dir).unwrap();
    fs::write(
        event_dir.join("event-1.json"),
        r#"{"event": "sensor-fire", "payload": {"sensor": "clippy", "signature": "sensor:clippy"}}"#,
    )
    .unwrap();

    let out = harness(&fx)
        .args([
            "feedback",
            "draft",
            "--repo",
            "d-o-hub/do-harness",
            "--format",
            "json",
        ])
        .output()
        .expect("run feedback draft");

    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let candidates = parsed["candidates"].as_array().unwrap();
    // 2 unique fires does not meet the default minimum of 3
    assert_eq!(candidates.len(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn test_synthetic_secret_never_reaches_draft_or_payload() {
    let fx = fixture();
    let conn = do_harness_db::connect_and_migrate(&fx.root).await.unwrap();

    let secret = "ghp_super_secret_token_1234567890";
    for _ in 0..3 {
        do_harness_db::bump_error_signature(
            &conn,
            "sensor:clippy",
            "branch:main",
            Some(&format!("failed with token: {secret}")),
        )
        .await
        .unwrap();
    }

    let out = harness(&fx)
        .env("SECRET_VAR", secret)
        .args([
            "feedback",
            "draft",
            "--repo",
            "d-o-hub/do-harness",
            "--format",
            "json",
        ])
        .output()
        .expect("run feedback draft");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains(secret),
        "secret leaked into stdout:\n{stdout}"
    );
    assert!(stdout.contains("[REDACTED]"), "secret must be redacted");
}

#[tokio::test(flavor = "current_thread")]
async fn test_repeat_submission_links_existing_issue_without_creating_duplicate() {
    let fx = fixture();
    let draft_file = fx.root.join("draft.json");
    let fingerprint = "fp_abc123456789";
    let draft_json = serde_json::json!({
        "schema_version": 1,
        "fingerprint": fingerprint,
        "target_repo": "d-o-hub/do-harness",
        "title": "Bug: test issue",
        "body": format!("Description\n<!-- do-harness-feedback-fingerprint: {fingerprint} -->"),
        "impact": "high",
        "build_identity": "0.3.7",
        "source_ids": ["1", "2", "3"],
        "observed_behavior": "crash",
        "expected_behavior": "pass",
        "reproduction": "run verify",
        "diagnostic_excerpt": "error trace",
        "suggested_verification": "check fix",
        "created_at": SAMPLE_TIMESTAMP
    });
    fs::write(&draft_file, serde_json::to_string(&draft_json).unwrap()).unwrap();

    // Mock gh issue list returns an existing issue with matching fingerprint marker
    fs::write(
        fx.root.join("mock_issues.json"),
        serde_json::to_string(&serde_json::json!([
            {
                "number": MOCK_ISSUE_NUMBER,
                "url": "https://github.com/d-o-hub/do-harness/issues/99",
                "state": "OPEN",
                "title": "Bug: existing",
                "body": format!("Prior report\n<!-- do-harness-feedback-fingerprint: {fingerprint} -->")
            }
        ]))
        .unwrap(),
    )
    .unwrap();

    let out = harness(&fx)
        .args([
            "feedback",
            "submit",
            "--draft",
            draft_file.to_str().unwrap(),
            "--yes",
        ])
        .output()
        .expect("submit feedback");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("linked") || stdout.contains("99"),
        "must report linked existing issue:\n{stdout}"
    );

    // Verify gh issue create was NOT called
    assert!(
        !fx.root.join("last_created_body.txt").exists(),
        "gh issue create must not be called when issue already exists"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn test_auth_failure_preserves_draft_file_and_fails_closed() {
    let fx = fixture();
    let draft_file = fx.root.join("draft.json");
    fs::write(&draft_file, r#"{"schema_version":1,"fingerprint":"fp1"}"#).unwrap();

    // Trigger gh failure
    fs::write(fx.root.join("fail_gh"), "1").unwrap();

    let out = harness(&fx)
        .args([
            "feedback",
            "submit",
            "--draft",
            draft_file.to_str().unwrap(),
            "--yes",
        ])
        .output()
        .expect("submit feedback");

    assert!(!out.status.success(), "auth failure must exit non-zero");
    assert!(draft_file.exists(), "draft file must remain preserved");
}

#[tokio::test(flavor = "current_thread")]
async fn test_unattended_auto_mode_submits_end_to_end_without_prompts() {
    let fx = fixture();
    let conn = do_harness_db::connect_and_migrate(&fx.root).await.unwrap();

    // 3 fires to qualify
    for _ in 0..3 {
        do_harness_db::bump_error_signature(
            &conn,
            "sensor:clippy",
            "branch:main",
            Some("unattended test error"),
        )
        .await
        .unwrap();
    }

    let out = harness(&fx)
        .args([
            "feedback",
            "auto",
            "--repo",
            "d-o-hub/do-harness",
            "--format",
            "json",
        ])
        .output()
        .expect("run feedback auto");

    assert!(
        out.status.success(),
        "auto mode must exit 0: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("https://github.com/d-o-hub/do-harness/issues/42"));

    // Verify gh issue create was called with fixed argv
    assert!(fx.root.join("last_created_body.txt").exists());
    let argv = fs::read_to_string(fx.root.join("last_created_argv.txt")).unwrap();
    assert!(argv.contains("issue create --repo d-o-hub/do-harness"));
}
