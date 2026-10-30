//! Integration tests for feedback attribution gate and independent run detection (WF-09).

#![allow(clippy::unwrap_used, clippy::expect_used)]
// The fake `gh` is a bash script, so these fixtures are Unix-only by construction.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const EXEC_PERMS: u32 = 0o755;
const ATTEMPT_FAILURE_CODE: i32 = 1;
const RETRY_INTERVAL_SECS: i64 = 2;
const FIRST_RUN_OFFSET_SECS: i64 = 3600;
const SECOND_RUN_OFFSET_SECS: i64 = 1800;
const THIRD_RUN_OFFSET_SECS: i64 = 600;

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
    let script = r#"#!/bin/sh
echo "[]"
exit 0
"#;
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
async fn test_sensor_retries_in_single_run_do_not_count_as_independent_incidents() {
    let fx = fixture();
    let conn = do_harness_db::connect_and_migrate(&fx.root).await.unwrap();

    let now = do_harness_db::unix_now();
    let first_run_ts = now - FIRST_RUN_OFFSET_SECS;

    // 3 retries in the same run (within seconds of each other)
    for i in 0..3 {
        let ts = first_run_ts + (i * RETRY_INTERVAL_SECS);
        do_harness_db::record_sensor_outcome(
            &conn,
            &do_harness_db::NewBeat {
                task_id: None,
                scope: Some(do_harness_types::BeatScope::Branch("main".to_owned())),
                beat_type: do_harness_types::BeatType::Sensor,
                status: do_harness_types::BeatStatus::Failed,
                sensor_exit_code: Some(ATTEMPT_FAILURE_CODE),
                sensor_name: Some("clippy"),
                started_at: ts,
                completed_at: Some(ts),
            },
            false,
            Some("retry failure in single run"),
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
    assert_eq!(
        candidates.len(),
        0,
        "retries in a single run must count as 1 incident and remain under threshold"
    );

    // Now add 2 more independent runs at later times (>60s apart)
    for offset in [SECOND_RUN_OFFSET_SECS, THIRD_RUN_OFFSET_SECS] {
        let run_ts = now - offset;
        do_harness_db::record_sensor_outcome(
            &conn,
            &do_harness_db::NewBeat {
                task_id: None,
                scope: Some(do_harness_types::BeatScope::Branch("main".to_owned())),
                beat_type: do_harness_types::BeatType::Sensor,
                status: do_harness_types::BeatStatus::Failed,
                sensor_exit_code: Some(ATTEMPT_FAILURE_CODE),
                sensor_name: Some("clippy"),
                started_at: run_ts,
                completed_at: Some(run_ts),
            },
            false,
            Some("independent run failure"),
        )
        .await
        .unwrap();
    }

    let out2 = harness(&fx)
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

    assert!(out2.status.success());
    let parsed2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let candidates2 = parsed2["candidates"].as_array().unwrap();
    assert_eq!(
        candidates2.len(),
        1,
        "three independent runs meeting threshold must qualify"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn test_expected_atdd_red_checks_are_filtered_by_attribution_gate() {
    let fx = fixture();
    let conn = do_harness_db::connect_and_migrate(&fx.root).await.unwrap();

    // 3 fires from an expected ATDD red check
    for _ in 0..3 {
        do_harness_db::bump_error_signature(
            &conn,
            "sensor:test",
            "branch:main",
            Some("[ATDD-RED] failing test fixture before implementation"),
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
    assert_eq!(
        candidates.len(),
        0,
        "expected ATDD red check must be suppressed by attribution gate"
    );
}
