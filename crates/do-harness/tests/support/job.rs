//! Fixtures for the bounded unattended-job acceptance tests (#481).
//!
//! A fixture repository carries one trivial signal set and, when a scenario
//! needs it, project-policy repairs. Executors are deterministic shell scripts,
//! so success, failure, timeout, interruption, and malformed output are
//! exercised without a model or network access.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{git, harness, run};

/// Parent-only variable the executor must never inherit.
pub const SECRET_ENV: &str = "PARENT_ONLY_SECRET";
/// Attempts a normal test job may use.
pub const ATTEMPTS: u32 = 3;
/// Attempts an extended-budget test job may use.
pub const MANY_ATTEMPTS: u32 = 5;
/// Per-attempt and whole-job budget in the fixture spec.
pub const TIMEOUT_SECONDS: u64 = 30;
/// Per-attempt budget that deliberately expires.
pub const HUNG_TIMEOUT_SECONDS: u64 = 1;
/// Poll interval while a job runs.
pub const POLL_MILLIS: u64 = 50;
/// Declared result size that must be rejected before it is read.
pub const OVERSIZED_RESULT_BYTES: u32 = 70_000;
/// A sensor that passes once the workspace contains `fixed`.
pub const PASS_WHEN_FIXED: &str =
    "[[sensors]]\nname = \"gate\"\nargv = [\"sh\", \"-c\", \"test -f fixed\"]\n";
/// A synthetic executor result.
pub const DONE: &str = "printf '{\"status\":\"done\"}' > \"$DO_HARNESS_JOB_RESULT\"";

/// Creates a committed fixture repository with `sensors` and `repairs` bodies.
pub fn fixture(sensors: &str, repairs: &str) -> (tempfile::TempDir, PathBuf) {
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
pub fn executor(root: &Path, body: &str) -> (PathBuf, String) {
    let path = root.join("executor.sh");
    std::fs::write(&path, format!("#!/bin/sh\nset -u\n{body}\n")).unwrap();
    let quoted = path.display().to_string().replace('\\', "\\\\");
    (path, format!("[\"sh\", \"{quoted}\"]"))
}

/// Writes the job spec next to the fixture repository.
pub fn write_spec(
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
pub fn run_job(root: &Path, spec: &Path, extra: &[&str]) -> (Option<i32>, String, String) {
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
pub fn receipt(root: &Path) -> Value {
    let jobs = root.join(".do-harness/jobs");
    let entry = std::fs::read_dir(&jobs)
        .unwrap_or_else(|error| panic!("no jobs directory at {}: {error}", jobs.display()))
        .filter_map(std::result::Result::ok)
        .find(|entry| entry.path().join("job.json").is_file())
        .expect("a job receipt");
    let text = std::fs::read_to_string(entry.path().join("job.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Best-effort receipt lookup used while a job is still running.
pub fn receipt_of(root: &Path) -> Option<Value> {
    std::fs::read_dir(root.join(".do-harness/jobs"))
        .ok()?
        .filter_map(std::result::Result::ok)
        .find_map(|entry| std::fs::read_to_string(entry.path().join("job.json")).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
}

/// Receipt state.
pub fn state(receipt: &Value) -> &str {
    receipt["state"].as_str().unwrap()
}

/// Typed blocked code.
pub fn blocked_code(receipt: &Value) -> &str {
    receipt["blocked"]["code"].as_str().unwrap()
}

/// Attempts consumed.
pub fn attempts(receipt: &Value) -> u64 {
    receipt["attempts_used"].as_u64().unwrap()
}
