//! Acceptance fixtures for the `ci-explain` log and repository discovery paths.
//!
//! Validates:
//! 1. nextest's own `FAIL [ ... ]` line is never read as a sensor name.
//! 2. A plain git repository without harness initialization is supported.
//! 3. The raw job-log endpoint backs up `gh run view` when it returns nothing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

mod support;

use support::ci_explain::{fake_gh, fixture_root, report, run_ci};
use support::{git_command, isolate_command};

#[cfg(unix)]
#[test]
fn nextest_fail_line_is_not_read_as_a_sensor_marker() {
    let (dir, root) = fixture_root("");
    let bin = fake_gh(&root);

    std::fs::write(
        root.join("run.json"),
        r#"{"id": 88, "name": "verify", "status": "completed", "conclusion": "failure", "html_url": ""}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("jobs.json"),
        r#"{
            "jobs": [
                {
                    "id": 902,
                    "name": "Windows",
                    "status": "completed",
                    "conclusion": "failure",
                    "html_url": "",
                    "steps": [{"name": "Run cargo nextest run --workspace --release --exclude do-harness-db", "conclusion": "failure"}]
                }
            ]
        }"#,
    )
    .unwrap();
    // nextest's own failure line, uncoloured (NO_COLOR / CARGO_TERM_COLOR=never):
    // only the harness verdict form `FAIL  <sensor>` may name a sensor.
    std::fs::write(
        root.join("log-902.txt"),
        "Windows\tRun cargo nextest run\t2026-09-28T17:01:06.0064200Z FAIL [   8.953s] ( 52/687) do-harness::dogfood rust_init_ignores_a_hook_inherited_git_dir\n",
    )
    .unwrap();

    let (code, stdout, stderr) = run_ci(&root, &bin, &["ci-explain", "88", "--format", "json"]);
    assert_eq!(code, Some(0), "stderr:\n{stderr}");
    let json = report(&stdout);
    let failed = &json["jobs"][0];
    assert_eq!(
        failed["local_sensor"], "test",
        "the step names the tool, not the `FAIL [ ... ]` token"
    );
    assert_eq!(
        failed["local_repro_command"],
        "cargo nextest run --workspace --no-tests=pass"
    );
    let _ = dir;
}

#[cfg(unix)]
#[test]
fn plain_git_repository_without_harness_init_is_supported() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // `git_command` drops the view variables a hook exports; left in place,
    // `git init` would reinitialize the *calling* repository instead.
    let git_init = git_command(root)
        .args(["init", "-q", "-b", "main"])
        .output()
        .expect("git init");
    assert!(git_init.status.success());
    let bin = fake_gh(root);

    std::fs::write(
        root.join("run.json"),
        r#"{"id": 88, "name": "verify", "status": "completed", "conclusion": "success", "html_url": ""}"#,
    )
    .unwrap();
    std::fs::write(root.join("jobs.json"), r#"{"jobs": []}"#).unwrap();

    let path = std::env::var("PATH").unwrap_or_default();
    let mut command = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    command
        .current_dir(root)
        .args(["ci-explain", "88", "--format", "json"])
        .env("PATH", format!("{}:{path}", bin.display()));
    let output = isolate_command(&mut command)
        .output()
        .expect("spawn do-harness");
    assert_eq!(
        output.status.code(),
        Some(0),
        "ci-explain must resolve a plain git tree; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = report(&String::from_utf8_lossy(&output.stdout));
    assert_eq!(json["run_id"], serde_json::json!(88));
    let _ = dir;
}

#[cfg(unix)]
#[test]
fn job_log_api_is_used_when_run_view_returns_nothing() {
    let (dir, root) = fixture_root("");
    let bin = fake_gh(&root);

    std::fs::write(
        root.join("run.json"),
        r#"{"id": 88, "name": "verify", "status": "completed", "conclusion": "failure", "html_url": ""}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("jobs.json"),
        r#"{
            "jobs": [
                {
                    "id": 903,
                    "name": "verify",
                    "status": "completed",
                    "conclusion": "failure",
                    "html_url": "",
                    "steps": [{"name": "Run sensors", "conclusion": "failure"}]
                }
            ]
        }"#,
    )
    .unwrap();
    // No `log-903.txt`, so `gh run view --log-failed` yields nothing and the
    // raw job-log endpoint carries the harness verdict instead.
    std::fs::write(
        root.join("joblog.txt"),
        "2026-09-28T17:01:06.0064200Z FAIL  shell\n2026-09-28T17:01:06.0064200Z shellcheck: SC2086 in scripts/demo.sh\n",
    )
    .unwrap();

    let (code, stdout, stderr) = run_ci(&root, &bin, &["ci-explain", "88", "--format", "json"]);
    assert_eq!(code, Some(0), "stderr:\n{stderr}");
    let json = report(&stdout);
    let failed = &json["jobs"][0];
    assert_eq!(failed["local_sensor"], "shell");
    assert_eq!(failed["local_repro_command"], "shellcheck scripts/*.sh");
    let _ = dir;
}
