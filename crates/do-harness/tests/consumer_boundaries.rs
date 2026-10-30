//! Consumer-visible boundary, error-code, cache, and suppression tests (WF-10).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

fn policy(root: &Path, script: &str, settings: &str) {
    std::fs::write(root.join("sensor.py"), script).unwrap();
    std::fs::write(
        root.join("do-harness.toml"),
        format!(
            "language='generic'\n[[sensors]]\nname='probe'\nargv=['python3','sensor.py']\n{settings}\n"
        ),
    )
    .unwrap();
}

#[test]
fn corrupted_pr_review_cache_evicts_cleanly_and_recomputes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(root)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(root)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .status()
        .unwrap();

    std::fs::write(root.join("f.txt"), "v1\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .status()
        .unwrap();
    Command::new("git")
        .args(["commit", "-q", "-m", "c1"])
        .current_dir(root)
        .status()
        .unwrap();

    Command::new("git")
        .args(["switch", "-q", "-c", "feature"])
        .current_dir(root)
        .status()
        .unwrap();
    std::fs::write(root.join("f.txt"), "v2\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .status()
        .unwrap();
    Command::new("git")
        .args(["commit", "-q", "-m", "c2"])
        .current_dir(root)
        .status()
        .unwrap();

    // Plant corrupt cache files in .git/do-harness/pr-review
    let cache_dir = root.join(".git/do-harness/pr-review");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(cache_dir.join("corrupt.json"), "{ invalid JSON content }}}").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .args([
            "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
        ])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "review should succeed despite corrupt cache: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("output must be valid JSON");
    assert_eq!(parsed["cached"], false);
    assert_eq!(parsed["residual"].as_array().unwrap().len(), 1);

    // Warm run should now find the newly written valid cache
    let out2 = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .args([
            "pr", "review", "--base", "main", "--head", "feature", "--format", "json",
        ])
        .output()
        .unwrap();
    assert!(out2.status.success());
    let parsed2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(parsed2["cached"], true);
}

#[test]
fn invalid_argument_values_fail_with_usage_code_2() {
    let dir = tempfile::tempdir().unwrap();
    policy(dir.path(), "print('ok')", "");

    let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(dir.path())
        .args([
            "pr",
            "review",
            "--base",
            "main",
            "--head",
            "feature",
            "--view",
            "unsupported_view",
        ])
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "invalid argument must exit code 2"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported_view") || stderr.contains("possible values"));
}

#[tokio::test(flavor = "current_thread")]
async fn feedback_suppression_boundary_excludes_transient_and_passing_events() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("do-harness.toml"), "language = 'rust'\n").unwrap();

    let conn = do_harness_db::connect_and_migrate(root).await.unwrap();

    // 1 transient non-severe fire: must be suppressed
    do_harness_db::bump_error_signature(
        &conn,
        "sensor:test",
        "branch:main",
        Some("transient error"),
    )
    .await
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .args([
            "feedback",
            "draft",
            "--repo",
            "d-o-hub/do-harness",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["candidates"].as_array().unwrap().len(),
        0,
        "transient error must be suppressed"
    );

    // Severe crash signature: immediately qualifies without recurrence
    do_harness_db::bump_error_signature(
        &conn,
        "panic:assertion_failure",
        "branch:main",
        Some("panic"),
    )
    .await
    .unwrap();

    let out2 = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .args([
            "feedback",
            "draft",
            "--repo",
            "d-o-hub/do-harness",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(out2.status.success());
    let report2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(
        report2["candidates"].as_array().unwrap().len(),
        1,
        "severe crash must qualify"
    );
}
