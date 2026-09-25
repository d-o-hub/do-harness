//! Acceptance fixtures for `do-harness ci-explain <run-id>` (#243).
//!
//! Validates:
//! 1. A cancelled job is classified `cancelled` with `gh run rerun` advice, distinct from `failure`.
//! 2. A compile-error log maps to the configured sensor and its exact local command.
//! 3. A `FAIL <sensor>` marker printed by the harness wins over job-name heuristics.
//! 4. A full workflow-run URL is accepted in place of a bare run ID.

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

/// A repository root with the two sensors the fixtures map onto.
fn fixture_root(config_extra: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(
        root.join("do-harness.toml"),
        format!(
            r#"[[sensors]]
name = "check"
argv = ["cargo", "check", "--workspace"]

[[sensors]]
name = "clippy"
argv = ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]
{config_extra}"#
        ),
    )
    .unwrap();
    (dir, root)
}

/// Fake `gh` dispatching the run/job/annotation/log queries `ci-explain` makes.
fn fake_gh(root: &Path) -> PathBuf {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    std::fs::write(
        &script,
        format!(
            r#"#!/bin/sh
MOCK_DIR="{}"
case "$1" in
    api)
        case "$2" in
            */jobs*)
                cat "$MOCK_DIR/jobs.json"
                exit 0
                ;;
            */annotations*)
                if [ -f "$MOCK_DIR/annotations.json" ]; then
                    cat "$MOCK_DIR/annotations.json"
                else
                    printf '%s' '[]'
                fi
                exit 0
                ;;
            */actions/runs/*)
                cat "$MOCK_DIR/run.json"
                exit 0
                ;;
        esac
        ;;
    run)
        shift
        # gh run view --job <id> --log-failed
        while [ "$#" -gt 0 ]; do
            if [ "$1" = "--job" ]; then
                job="$2"
                shift 2
                continue
            fi
            shift
        done
        if [ -f "$MOCK_DIR/log-$job.txt" ]; then
            cat "$MOCK_DIR/log-$job.txt"
            exit 0
        fi
        printf '%s' ''
        exit 0
        ;;
esac
echo "unknown fake gh call: $@" >&2
exit 1
"#,
            root.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

fn run_ci(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
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

fn report(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("ci-explain --format json must emit JSON")
}

#[cfg(unix)]
#[test]
fn cancelled_matrix_leg_is_cancelled_with_rerun_advice_not_failure() {
    let (dir, root) = fixture_root("");
    let bin = fake_gh(&root);

    std::fs::write(
        root.join("run.json"),
        r#"{
            "id": 35885635041,
            "name": "Storage Matrix Tests",
            "status": "completed",
            "conclusion": "cancelled",
            "html_url": "https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635041"
        }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("jobs.json"),
        r#"{
            "jobs": [
                {
                    "id": 4242,
                    "name": "Storage Matrix (ubuntu-latest)",
                    "status": "completed",
                    "conclusion": "cancelled",
                    "html_url": "https://example.test/job/4242",
                    "steps": [
                        {"name": "Run tests", "conclusion": "cancelled"}
                    ]
                },
                {
                    "id": 4243,
                    "name": "Quick PR Check",
                    "status": "completed",
                    "conclusion": "success",
                    "html_url": "https://example.test/job/4243",
                    "steps": [{"name": "Checkout", "conclusion": "success"}]
                }
            ]
        }"#,
    )
    .unwrap();

    let (code, stdout, stderr) = run_ci(
        &root,
        &bin,
        &["ci-explain", "35885635041", "--format", "json"],
    );
    assert_eq!(code, Some(0), "stderr:\n{stderr}");
    let json = report(&stdout);

    let cancelled = &json["jobs"][0];
    assert_eq!(cancelled["classification"], "cancelled");
    assert_eq!(cancelled["failed_step"], Value::Null);
    assert_eq!(cancelled["local_sensor"], Value::Null);
    assert_eq!(
        cancelled["rerun_command"],
        "gh run rerun 35885635041 --job 4242"
    );
    assert_eq!(json["summary"]["cancelled"], 1);
    assert_eq!(json["summary"]["failed"], 0);
    assert_eq!(
        json["summary"]["recommended_action"],
        "gh run rerun 35885635041"
    );

    // Human surface: the word CANCELLED, the rerun command, and rerun-first advice.
    let (text_code, text, _) = run_ci(&root, &bin, &["ci-explain", "35885635041"]);
    assert_eq!(text_code, Some(0));
    assert!(
        text.contains("CANCELLED: Storage Matrix (ubuntu-latest)"),
        "{text}"
    );
    assert!(
        text.contains("gh run rerun 35885635041 --job 4242"),
        "{text}"
    );
    assert!(
        !text.contains("FAILED:"),
        "no job may read as failed:\n{text}"
    );
    let _ = dir;
}

#[cfg(unix)]
#[test]
fn compile_error_log_maps_to_the_sensor_and_local_command() {
    let (dir, root) = fixture_root("");
    let bin = fake_gh(&root);

    std::fs::write(
        root.join("run.json"),
        r#"{
            "id": 77,
            "name": "verify",
            "status": "completed",
            "conclusion": "failure",
            "html_url": "https://example.test/runs/77"
        }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("jobs.json"),
        r#"{
            "jobs": [
                {
                    "id": 900,
                    "name": "verify",
                    "status": "completed",
                    "conclusion": "failure",
                    "html_url": "https://example.test/job/900",
                    "steps": [
                        {"name": "Build harness CLI", "conclusion": "success"},
                        {"name": "Run sensors", "conclusion": "failure"}
                    ]
                }
            ]
        }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("annotations.json"),
        r#"[{"annotation_level": "failure", "message": "Process completed with exit code 1.", "path": ".github"}]"#,
    )
    .unwrap();
    std::fs::write(
        root.join("log-900.txt"),
        "verify\tRun sensors\t2026-09-25T17:40:00.0000000Z error[E0432]: unresolved import `crate::missing`\n\
         verify\tRun sensors\t2026-09-25T17:40:00.1000000Z   --> src/lib.rs:3:5\n\
         verify\tRun sensors\t2026-09-25T17:40:01.0000000Z error: could not compile `do-harness` (bin \"do-harness\") due to 1 previous error\n",
    )
    .unwrap();

    let (code, stdout, stderr) = run_ci(&root, &bin, &["ci-explain", "77", "--format", "json"]);
    assert_eq!(code, Some(0), "stderr:\n{stderr}");
    let json = report(&stdout);

    let failed = &json["jobs"][0];
    assert_eq!(failed["classification"], "failed");
    assert_eq!(failed["failed_step"], "Run sensors");
    assert_eq!(failed["local_sensor"], "check");
    assert_eq!(failed["local_repro_command"], "cargo check --workspace");
    assert!(
        failed["failure_reason"]
            .as_str()
            .unwrap()
            .contains("could not compile"),
        "the compile error must be reported, got {:?}",
        failed["failure_reason"]
    );
    assert!(
        json["summary"]["recommended_action"]
            .as_str()
            .unwrap()
            .contains("cargo check --workspace"),
        "got {:?}",
        json["summary"]["recommended_action"]
    );
    let _ = dir;
}

#[cfg(unix)]
#[test]
fn harness_fail_marker_wins_over_job_name_heuristics() {
    let (dir, root) = fixture_root("");
    let bin = fake_gh(&root);

    std::fs::write(
        root.join("run.json"),
        r#"{"id": 88, "name": "verify", "status": "completed", "conclusion": "failure", "html_url": ""}"#,
    )
    .unwrap();
    // Job and step text say "test"; the harness log says the clippy sensor failed.
    std::fs::write(
        root.join("jobs.json"),
        r#"{
            "jobs": [
                {
                    "id": 901,
                    "name": "Test adjacent guardian-proxy crate",
                    "status": "completed",
                    "conclusion": "failure",
                    "html_url": "",
                    "steps": [{"name": "Run cargo test", "conclusion": "failure"}]
                }
            ]
        }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("log-901.txt"),
        "verify\tRun sensors\t2026-09-25T17:40:50.7221986Z PASS  fmt\n\
         verify\tRun sensors\t2026-09-25T17:40:50.7221986Z FAIL  clippy\n",
    )
    .unwrap();

    let (code, stdout, stderr) = run_ci(&root, &bin, &["ci-explain", "88", "--format", "json"]);
    assert_eq!(code, Some(0), "stderr:\n{stderr}");
    let json = report(&stdout);
    let failed = &json["jobs"][0];
    assert_eq!(failed["local_sensor"], "clippy");
    assert_eq!(
        failed["local_repro_command"],
        "cargo clippy --workspace --all-targets -- -D warnings"
    );
    let _ = dir;
}

#[cfg(unix)]
#[test]
fn workflow_url_is_accepted_and_unparseable_ids_are_a_usage_error() {
    let (dir, root) = fixture_root("");
    let bin = fake_gh(&root);

    std::fs::write(
        root.join("run.json"),
        r#"{"id": 99, "name": "CI", "status": "completed", "conclusion": "success", "html_url": ""}"#,
    )
    .unwrap();
    std::fs::write(root.join("jobs.json"), r#"{"jobs": []}"#).unwrap();

    let (code, stdout, stderr) = run_ci(
        &root,
        &bin,
        &[
            "ci-explain",
            "https://github.com/d-o-hub/do-harness/actions/runs/99",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "stderr:\n{stderr}");
    assert_eq!(report(&stdout)["run_id"], 99);

    let (bad_code, _, bad_err) = run_ci(&root, &bin, &["ci-explain", "not-a-run"]);
    assert_eq!(bad_code, Some(2), "usage errors exit 2:\n{bad_err}");
    assert!(bad_err.contains("invalid run ID"), "{bad_err}");
    let _ = dir;
}
