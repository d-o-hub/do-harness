//! Acceptance tests for bounded unattended jobs (issue #481).
//!
//! Every scenario drives the real `do-harness` binary against a fixture
//! repository and a fake shell executor. The executor is a deterministic
//! script, so success, failure, timeout, interruption, malformed output,
//! missing credentials, and progress detection are all exercised without a
//! model or network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use serde_json::Value;

mod support;

use support::job::{
    ATTEMPTS, DONE, HUNG_TIMEOUT_SECONDS, MANY_ATTEMPTS, OVERSIZED_RESULT_BYTES, PASS_WHEN_FIXED,
    POLL_MILLIS, SECRET_ENV, TIMEOUT_SECONDS, WAIT_SECONDS, attempts, blocked_code, executor,
    fixture, receipt, receipt_of, run_job, state, write_spec,
};
use support::{harness, run};

#[test]
fn a_semantic_fix_ends_verified_through_the_gate() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, &format!("touch fixed\n{DONE}"));
    let spec = write_spec(&root, &argv, ATTEMPTS, TIMEOUT_SECONDS, "");
    let (code, stdout, stderr) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(0), "stdout:\n{stdout}\nstderr:\n{stderr}");
    let receipt = receipt(&root);
    assert_eq!(state(&receipt), "verified");
    assert_eq!(attempts(&receipt), 1);
    assert_eq!(receipt["attempts"][0]["verification"]["state"], "green");
}

#[test]
fn claiming_done_with_failing_checks_blocks() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, DONE);
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let (code, _, stderr) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(1), "{stderr}");
    let receipt = receipt(&root);
    assert_eq!(state(&receipt), "blocked");
    assert_eq!(blocked_code(&receipt), "verification-failed");
    assert!(stderr.contains("verification-failed"), "{stderr}");
}

#[test]
fn a_declared_mechanical_repair_completes_without_the_executor() {
    let repairs = "\n[[repairs]]\nsensor = \"gate\"\nargv = [\"sh\", \"-c\", \"touch fixed\"]\n";
    let (_dir, root) = fixture(PASS_WHEN_FIXED, repairs);
    let (_, argv) = executor(&root, DONE);
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let (code, stdout, stderr) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(0), "stdout:\n{stdout}\nstderr:\n{stderr}");
    let receipt = receipt(&root);
    assert_eq!(state(&receipt), "verified");
    assert_eq!(
        receipt["attempts"][0]["repairs_applied"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(receipt["applied_repairs"].as_array().unwrap().len(), 1);
}

#[test]
fn repeated_identical_failures_stop_as_blocked() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, DONE);
    let spec = write_spec(&root, &argv, MANY_ATTEMPTS, TIMEOUT_SECONDS, "");
    let (code, _, _) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(1));
    let receipt = receipt(&root);
    assert_eq!(blocked_code(&receipt), "no-progress");
    assert_eq!(
        attempts(&receipt),
        2,
        "budget remains but no progress was made"
    );
}

#[test]
fn alternating_failures_stop_as_blocked() {
    let sensors = "[[sensors]]\nname = \"a\"\nargv = [\"sh\", \"-c\", \"test -f a_ok\"]\n\n\
                   [[sensors]]\nname = \"b\"\nargv = [\"sh\", \"-c\", \"test -f b_ok\"]\n";
    let (_dir, root) = fixture(sensors, "");
    let body = "if [ \"$DO_HARNESS_JOB_ATTEMPT\" = \"1\" ]; then rm -f b_ok; touch a_ok; \
                else rm -f a_ok; touch b_ok; fi\nprintf '{\"status\":\"done\"}' > \"$DO_HARNESS_JOB_RESULT\"";
    let (_, argv) = executor(&root, body);
    let spec = write_spec(&root, &argv, MANY_ATTEMPTS, TIMEOUT_SECONDS, "");
    let (code, _, _) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(1));
    let receipt = receipt(&root);
    assert_eq!(blocked_code(&receipt), "no-progress");
    assert_eq!(attempts(&receipt), 2);
}

#[test]
fn a_hung_executor_is_killed_and_blocks() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, "sleep 30");
    let spec = write_spec(&root, &argv, ATTEMPTS, HUNG_TIMEOUT_SECONDS, "");
    let started = Instant::now();
    let (code, _, _) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(1));
    assert!(
        started.elapsed() < Duration::from_secs(WAIT_SECONDS),
        "the hung executor outlived its budget"
    );
    let receipt = receipt(&root);
    assert_eq!(blocked_code(&receipt), "executor-timeout");
    assert_eq!(receipt["attempts"][0]["executor"]["timed_out"], true);
}

#[test]
fn malformed_output_never_proves_completion() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, "printf 'not json' > \"$DO_HARNESS_JOB_RESULT\"");
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let (code, _, _) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(1));
    let receipt = receipt(&root);
    assert_eq!(blocked_code(&receipt), "invalid-executor-output");
    assert!(
        receipt["attempts"][0]["executor"]["report_error"]
            .as_str()
            .unwrap()
            .contains("malformed")
    );
}

#[test]
fn an_oversized_result_is_rejected_without_being_read() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(
        &root,
        &format!("head -c {OVERSIZED_RESULT_BYTES} /dev/zero > \"$DO_HARNESS_JOB_RESULT\""),
    );
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let (code, _, _) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(1));
    let receipt = receipt(&root);
    assert_eq!(blocked_code(&receipt), "invalid-executor-output");
    assert!(
        receipt["attempts"][0]["executor"]["report_error"]
            .as_str()
            .unwrap()
            .contains("exceeds")
    );
}

#[test]
fn missing_credentials_block_without_requesting_broader_access() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let body = "env > \"$DO_HARNESS_JOB_WORKSPACE/env.txt\"\n\
                printf '{\"status\":\"blocked\",\"blocked\":{\"code\":\"authorization-unavailable\",\"detail\":\"JOB_TOKEN is unset\"}}' \
                > \"$DO_HARNESS_JOB_RESULT\"";
    let (_, argv) = executor(&root, body);
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "\"JOB_TOKEN\"");
    let (code, _, stderr) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(1), "{stderr}");
    let receipt = receipt(&root);
    assert_eq!(blocked_code(&receipt), "authorization-unavailable");
    assert!(
        receipt["blocked"]["resume"]
            .as_str()
            .unwrap()
            .contains("allowed-env")
    );
    let workspace = root.join(receipt["workspace"].as_str().unwrap());
    let env = std::fs::read_to_string(workspace.join("env.txt")).unwrap();
    assert!(
        !env.contains(SECRET_ENV),
        "the executor inherited a parent credential:\n{env}"
    );
}

#[test]
fn an_interrupted_job_resumes_without_renewing_budget() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, &format!("sleep 30\n{DONE}"));
    let spec = write_spec(&root, &argv, ATTEMPTS, TIMEOUT_SECONDS, "");
    let mut child = harness(&root)
        .arg("job")
        .arg("run")
        .arg("--spec")
        .arg(&spec)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(TIMEOUT_SECONDS);
    while !root.join(".do-harness/jobs").exists() {
        assert!(Instant::now() < deadline, "the job never started");
        std::thread::sleep(Duration::from_millis(POLL_MILLIS));
    }
    while receipt_of(&root).is_none_or(|value| attempts(&value) < 1) {
        assert!(Instant::now() < deadline, "the first attempt never started");
        std::thread::sleep(Duration::from_millis(POLL_MILLIS));
    }
    child.kill().unwrap();
    let _ = child.wait();

    let (_, fixed) = executor(&root, &format!("touch fixed\n{DONE}"));
    let resumed = write_spec(&root, &fixed, ATTEMPTS, TIMEOUT_SECONDS, "");
    let (code, stdout, stderr) = run_job(&root, &resumed, &["--resume"]);
    assert_eq!(code, Some(0), "stdout:\n{stdout}\nstderr:\n{stderr}");
    let receipt = receipt(&root);
    assert_eq!(state(&receipt), "verified");
    assert_eq!(attempts(&receipt), 2, "the budget was not renewed");
    assert_eq!(receipt["attempts_limit"], ATTEMPTS);
}

#[test]
fn dry_run_creates_nothing() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, &format!("touch fixed\n{DONE}"));
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let (code, stdout, stderr) = run_job(&root, &spec, &["--dry-run"]);
    assert_eq!(code, Some(0), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(!root.join(".do-harness/jobs").exists());
    assert!(!root.join("fixed").exists());
}

#[test]
fn ordinary_verification_never_launches_an_executor() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, &format!("touch executor_ran\n{DONE}"));
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    assert!(spec.is_file());
    let (code, stdout, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--strict"));
    assert_ne!(
        code,
        Some(0),
        "the fixture gate must be red:\n{stdout}\n{stderr}"
    );
    assert!(!root.join("executor_ran").exists());
    assert!(!root.join(".do-harness/jobs").exists());
}

#[test]
fn a_git_worktree_is_not_accepted_as_the_isolation_boundary() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, &format!("touch fixed\n{DONE}"));
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let text = std::fs::read_to_string(&spec)
        .unwrap()
        .replace("test-harness", "git-worktree");
    std::fs::write(&spec, text).unwrap();
    let (code, _, stderr) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("not a process/network/filesystem sandbox"),
        "{stderr}"
    );
}

#[test]
fn a_resumed_terminal_job_is_an_idempotent_no_op() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, &format!("touch fixed\n{DONE}"));
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let (code, _, _) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(0));
    let (code, stdout, stderr) = run_job(&root, &spec, &["--resume"]);
    assert_eq!(code, Some(0), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert_eq!(attempts(&receipt(&root)), 1);
}

#[test]
fn status_reports_blocked_receipts() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, DONE);
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let _ = run_job(&root, &spec, &[]);
    let (code, stdout, stderr) = run(harness(&root)
        .arg("job")
        .arg("status")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "{stderr}");
    let documents: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(documents[0]["state"], "blocked");
    assert_eq!(documents[0]["blocked"]["code"], "verification-failed");
}

#[test]
fn an_executor_that_cannot_start_blocks_as_unavailable() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let argv = "[\"definitely-not-a-real-executor-binary\"]".to_owned();
    let spec = write_spec(&root, &argv, 2, TIMEOUT_SECONDS, "");
    let (code, _, _) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(1));
    assert_eq!(blocked_code(&receipt(&root)), "executor-unavailable");
}

#[test]
fn a_job_without_a_typed_resume_plan_is_refused() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(&root, &format!("touch fixed\n{DONE}"));
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let _ = run_job(&root, &spec, &[]);
    let (code, _, stderr) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("already exists"), "{stderr}");
}

#[test]
fn the_fake_executor_observes_job_identity() {
    let (_dir, root) = fixture(PASS_WHEN_FIXED, "");
    let (_, argv) = executor(
        &root,
        "test -n \"$DO_HARNESS_JOB_ID\"\ntest -n \"$DO_HARNESS_JOB_ATTEMPT\"\ntouch fixed\nprintf '{\"status\":\"done\"}' > \"$DO_HARNESS_JOB_RESULT\"",
    );
    let spec = write_spec(&root, &argv, 1, TIMEOUT_SECONDS, "");
    let (code, stdout, stderr) = run_job(&root, &spec, &[]);
    assert_eq!(code, Some(0), "stdout:\n{stdout}\nstderr:\n{stderr}");
    let receipt = receipt(&root);
    assert!(receipt["job_id"].as_str().unwrap().starts_with("job-"));
}
