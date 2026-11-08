//! Acceptance tests for bounded unattended jobs (issue #481).
//!
//! Every scenario drives the real `do-harness` binary against a fixture
//! repository and a fake shell executor. The executor is a deterministic
//! script, so success, failure, timeout, interruption, malformed output,
//! missing credentials, and progress detection are all exercised without a
//! model or network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

mod support;

use support::{git, harness, run};

const SECRET_ENV: &str = "PARENT_ONLY_SECRET";
/// Attempts a normal test job may use.
const ATTEMPTS: u32 = 3;
/// Attempts an extended-budget test job may use.
const MANY_ATTEMPTS: u32 = 5;
/// Per-attempt and whole-job budget in the fixture spec.
const TIMEOUT_SECONDS: u64 = 30;
/// Per-attempt budget that deliberately expires.
const HUNG_TIMEOUT_SECONDS: u64 = 1;
/// How long a test waits for a job to reach an observable state.
const WAIT_SECONDS: u64 = 20;
/// Poll interval while a job runs.
const POLL_MILLIS: u64 = 50;

/// Creates a committed fixture repository with `sensors` and `repairs` bodies.
fn fixture(sensors: &str, repairs: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let config = format!(
        "[signal-sets]\nverification = [{names}]\n\n{sensors}\n{repairs}",
        names = sensor_names(sensors)
    );
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::write(root.join(".gitignore"), ".do-harness/\n").unwrap();
    std::fs::write(root.join("lib.rs"), "v1\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);
    (dir, root)
}

/// Comma-separated quoted names of the sensors declared in `sensors`.
fn sensor_names(sensors: &str) -> String {
    sensors
        .lines()
        .filter_map(|line| line.strip_prefix("name = "))
        .map(|name| format!("\"{}\"", name.trim_matches('"')))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Writes a fake executor script and returns `(absolute path, argv TOML)`.
fn executor(root: &Path, body: &str) -> (PathBuf, String) {
    let path = root.join("executor.sh");
    std::fs::write(&path, format!("#!/bin/sh\nset -u\n{body}\n")).unwrap();
    let quoted = path.display().to_string().replace('\\', "\\\\");
    (path, format!("[\"sh\", \"{quoted}\"]"))
}

/// Writes the job spec next to the fixture repository.
fn write_spec(
    root: &Path,
    argv: &str,
    attempts: u32,
    executor_timeout: u64,
    allowed_env: &str,
) -> PathBuf {
    let spec = format!(
        "schema-version = 1\n\
         task = \"make the gate pass\"\n\
         revision = \"HEAD\"\n\
         signal-set = \"verification\"\n\
         attempts = {attempts}\n\
         deadline-seconds = 120\n\
         executor-timeout-seconds = {executor_timeout}\n\
         \n\
         [executor]\n\
         argv = {argv}\n\
         allowed-env = [{allowed_env}]\n\
         result-required = true\n\
         \n\
         [isolation]\n\
         mechanism = \"test-harness\"\n"
    );
    let path = root.join("job.toml");
    std::fs::write(&path, spec).unwrap();
    path
}

/// Runs `job run` for `spec`.
fn run_job(root: &Path, spec: &Path, extra: &[&str]) -> (Option<i32>, String, String) {
    let mut command = harness(root);
    command
        .arg("job")
        .arg("run")
        .arg("--spec")
        .arg(spec)
        .arg("--format")
        .arg("json")
        .args(extra)
        .env(SECRET_ENV, "s3cret");
    run(&mut command)
}

/// Reads the single job receipt under the fixture root.
fn receipt(root: &Path) -> Value {
    let jobs = root.join(".do-harness/jobs");
    let entry = std::fs::read_dir(&jobs)
        .unwrap_or_else(|error| panic!("no jobs directory at {}: {error}", jobs.display()))
        .filter_map(std::result::Result::ok)
        .find(|entry| entry.path().join("job.json").is_file())
        .expect("a job receipt");
    let text = std::fs::read_to_string(entry.path().join("job.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn state(receipt: &Value) -> &str {
    receipt["state"].as_str().unwrap()
}

fn blocked_code(receipt: &Value) -> &str {
    receipt["blocked"]["code"].as_str().unwrap()
}

fn attempts(receipt: &Value) -> u64 {
    receipt["attempts_used"].as_u64().unwrap()
}

const PASS_WHEN_FIXED: &str =
    "[[sensors]]\nname = \"gate\"\nargv = [\"sh\", \"-c\", \"test -f fixed\"]\n";
const DONE: &str = "printf '{\"status\":\"done\"}' > \"$DO_HARNESS_JOB_RESULT\"";

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

/// Best-effort receipt lookup used while a job is still running.
fn receipt_of(root: &Path) -> Option<Value> {
    std::fs::read_dir(root.join(".do-harness/jobs"))
        .ok()?
        .filter_map(std::result::Result::ok)
        .find_map(|entry| std::fs::read_to_string(entry.path().join("job.json")).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
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
