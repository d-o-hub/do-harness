//! Acceptance tests for multi-workspace verification and isolation.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::Value;

mod support;

use support::range_selection::fixture_repo;
use support::{git, harness, run};

const WORKSPACE_LOCK_ENV: &str = "DO_HARNESS_TEST_585_WORKSPACE_LOCK";
const WORKSPACE_FAIL_ENV: &str = "DO_HARNESS_TEST_585_WORKSPACE_FAIL";

fn quote_toml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn workspace_config(name: &str, argv: &[String], changed: &[&str]) -> String {
    let argv = argv
        .iter()
        .map(|value| quote_toml(value))
        .collect::<Vec<_>>()
        .join(", ");
    let changed = changed
        .iter()
        .map(|value| quote_toml(value))
        .collect::<Vec<_>>()
        .join(", ");
    let changed = if changed.is_empty() {
        String::new()
    } else {
        format!("when-changed = [{changed}]\n")
    };
    format!(
        "[signal-sets]\nall = [{}]\n\n[[sensors]]\nname = {}\nargv = [{}]\n{}",
        quote_toml(name),
        quote_toml(name),
        argv,
        changed
    )
}

fn add_workspace(
    root: &Path,
    relative: &str,
    name: &str,
    argv: &[String],
    changed: &[&str],
) -> PathBuf {
    let workspace = root.join(relative);
    fs::create_dir_all(&workspace).expect("create workspace");
    fs::write(
        workspace.join("do-harness.toml"),
        workspace_config(name, argv, changed),
    )
    .expect("write workspace config");
    workspace
}

fn version_argv() -> Vec<String> {
    vec![
        env!("CARGO_BIN_EXE_do-harness").to_owned(),
        "version".to_owned(),
    ]
}

fn workspace_probe_argv() -> Vec<String> {
    vec![
        std::env::current_exe()
            .expect("current test executable")
            .to_string_lossy()
            .into_owned(),
        "--exact".to_owned(),
        "workspace_sensor_probe".to_owned(),
        "--nocapture".to_owned(),
    ]
}

fn run_workspaces(root: &Path, pattern: &str, args: &[&str]) -> (Option<i32>, Value, String) {
    let mut command = harness(root);
    command
        .args(["verify", "--workspaces", pattern, "--format", "json"])
        .args(args);
    let (status, stdout, stderr) = run(&mut command);
    let report = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!("invalid workspace report: {error}; stdout={stdout:?}; stderr={stderr:?}")
    });
    (status, report, stderr)
}

fn run_text_workspaces(root: &Path, pattern: &str, args: &[&str]) -> (Option<i32>, String, String) {
    let mut command = harness(root);
    command
        .args(["verify", "--workspaces", pattern, "--format", "text"])
        .args(args);
    run(&mut command)
}

fn workspace_result<'a>(report: &'a Value, relative: &str) -> &'a Value {
    report["workspaces"]
        .as_array()
        .expect("workspace result array")
        .iter()
        .find(|result| {
            result["root"]
                .as_str()
                .is_some_and(|path| Path::new(path).ends_with(relative))
        })
        .unwrap_or_else(|| panic!("missing workspace {relative}: {report}"))
}

#[test]
fn aggregates_reports_and_records_each_workspace_locally() {
    let (_directory, root) = fixture_repo();
    let argv = version_argv();
    add_workspace(&root, "packages/api", "api", &argv, &[]);
    add_workspace(&root, "packages/web", "web", &argv, &[]);

    let (status, report, stderr) = run_workspaces(&root, "packages/*", &["--record"]);
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["ok"], true);
    assert_eq!(report["jobs"], 1);
    assert_eq!(
        report["workspaces"].as_array().expect("workspaces").len(),
        2
    );
    assert_eq!(
        workspace_result(&report, "packages/api")["status"],
        "passed"
    );
    assert_eq!(
        workspace_result(&report, "packages/web")["status"],
        "passed"
    );
    assert!(
        root.join("packages/api/.do-harness/agent_state.db")
            .is_file()
    );
    assert!(
        root.join("packages/web/.do-harness/agent_state.db")
            .is_file()
    );
}

#[test]
fn global_changed_input_runs_only_affected_workspaces_with_local_paths() {
    let (_directory, root) = fixture_repo();
    let argv = version_argv();
    let api = add_workspace(&root, "packages/api", "api", &argv, &["src/**"]);
    let web = add_workspace(&root, "packages/web", "web", &argv, &["src/**"]);
    fs::create_dir_all(api.join("src")).expect("create api source dir");
    fs::create_dir_all(web.join("src")).expect("create web source dir");
    fs::write(api.join("src/api.rs"), "pub fn api() {}\n").expect("write api source");
    fs::write(web.join("src/web.rs"), "pub fn web() {}\n").expect("write web source");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: add workspaces"]);
    fs::write(api.join("src/api.rs"), "pub fn api() { let _ = 1; }\n").expect("modify api source");

    let (status, report, stderr) = run_workspaces(&root, "packages/*", &["--changed"]);
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(
        report["selection"]["changed_files"][0],
        "packages/api/src/api.rs"
    );
    let api_report = workspace_result(&report, "packages/api");
    let web_report = workspace_result(&report, "packages/web");
    assert_eq!(api_report["status"], "passed");
    assert_eq!(
        api_report["report"]["selection"]["changed_files"][0],
        "src/api.rs"
    );
    assert_eq!(web_report["status"], "skipped");
    assert_eq!(web_report["exit_code"], 0);
}

#[test]
fn jobs_bounds_concurrent_workspace_processes() {
    let (_directory, root) = fixture_repo();
    let argv = workspace_probe_argv();
    add_workspace(&root, "packages/api", "api", &argv, &[]);
    add_workspace(&root, "packages/web", "web", &argv, &[]);
    let lock = root.join("workspace-probe.lock");
    let mut command = harness(&root);
    command
        .args([
            "verify",
            "--workspaces",
            "packages/*",
            "--jobs",
            "1",
            "--format",
            "json",
        ])
        .env(WORKSPACE_LOCK_ENV, lock);
    let (status, stdout, stderr) = run(&mut command);
    let report: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid workspace report: {error}; stderr={stderr:?}"));
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(report["jobs"], 1);
    assert_eq!(report["ok"], true);
}

#[test]
fn aggregates_unrunnable_workspace_as_exit_two() {
    let (_directory, root) = fixture_repo();
    let good = version_argv();
    add_workspace(&root, "packages/good", "good", &good, &[]);
    let missing = vec![root.join("missing-sensor").to_string_lossy().into_owned()];
    add_workspace(&root, "packages/bad", "bad", &missing, &[]);

    let (status, report, stderr) = run_workspaces(&root, "packages/*", &[]);
    assert_eq!(status, Some(2), "{stderr}");
    assert_eq!(report["ok"], false);
    assert_eq!(report["exit_code"], 2);
    assert_eq!(
        workspace_result(&report, "packages/good")["status"],
        "passed"
    );
    assert_eq!(workspace_result(&report, "packages/bad")["exit_code"], 2);
    assert_ne!(
        workspace_result(&report, "packages/bad")["report"]["unrunnable"]
            .as_array()
            .expect("unrunnable sensors")
            .as_slice(),
        &[] as &[Value]
    );
}

#[test]
fn text_output_reports_workspace_and_overall_verdicts() {
    let (_directory, root) = fixture_repo();
    let argv = version_argv();
    add_workspace(&root, "packages/api", "api", &argv, &[]);

    let (status, stdout, stderr) = run_text_workspaces(&root, "packages/*", &[]);
    assert_eq!(status, Some(0), "{stderr}");
    assert!(stdout.contains("api: passed (exit 0)"), "{stdout}");
    assert!(stdout.contains("api: PASS"), "{stdout}");
    assert!(stdout.contains("overall: PASS (exit 0)"), "{stdout}");
}

#[test]
fn text_output_marks_reused_sensor_as_skipped_even_when_quiet() {
    let (_directory, root) = fixture_repo();
    let argv = version_argv();
    let workspace = add_workspace(&root, "packages/api", "api", &argv, &[]);
    let input = workspace.join("src/steady.rs");
    fs::create_dir_all(input.parent().expect("input parent")).expect("create source directory");
    fs::write(&input, "pub fn steady() {}\n").expect("write reusable sensor input");
    let config_path = workspace.join("do-harness.toml");
    let config = fs::read_to_string(&config_path).expect("read workspace config");
    fs::write(
        config_path,
        format!("{config}inputs = [\"src/steady.rs\"]\n"),
    )
    .expect("declare reusable sensor input");

    let (status, first_report, stderr) = run_workspaces(&root, "packages/*", &["--record"]);
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(
        workspace_result(&first_report, "packages/api")["report"]["sensors"][0]["execution"],
        "ran"
    );

    let (status, stdout, stderr) = run_text_workspaces(
        &root,
        "packages/*",
        &["--record", "--unchanged", "skip", "--quiet"],
    );
    assert_eq!(status, Some(0), "{stderr}");
    assert!(stdout.contains("api: SKIP"), "{stdout}");
    assert!(!stdout.contains("api: PASS"), "{stdout}");
}

#[test]
fn text_output_marks_advisory_sensor_failure_as_warn() {
    let (_directory, root) = fixture_repo();
    let workspace = add_workspace(&root, "packages/api", "api", &workspace_probe_argv(), &[]);
    let config_path = workspace.join("do-harness.toml");
    let config = fs::read_to_string(&config_path).expect("read workspace config");
    fs::write(config_path, format!("{config}severity = \"warn\"\n"))
        .expect("enable advisory severity");

    let mut command = harness(&root);
    command
        .args(["verify", "--workspaces", "packages/*", "--format", "text"])
        .env(WORKSPACE_FAIL_ENV, "1");
    let (status, stdout, stderr) = run(&mut command);
    assert_eq!(status, Some(0), "{stderr}");
    assert!(stdout.contains("api: WARN"), "{stdout}");
    assert!(!stdout.contains("api: FAIL"), "{stdout}");
    assert!(stdout.contains("overall: PASS (exit 0)"), "{stdout}");
}

#[test]
fn simulate_text_output_marks_the_plan_without_claiming_a_pass() {
    let (_directory, root) = fixture_repo();
    let argv = version_argv();
    add_workspace(&root, "packages/api", "api", &argv, &[]);

    let (status, stdout, stderr) = run_text_workspaces(&root, "packages/*", &["--simulate"]);
    assert_eq!(status, Some(0), "{stderr}");
    assert!(stdout.contains("api: planned (exit 0)"), "{stdout}");
    assert!(stdout.contains("api: PLAN"), "{stdout}");
    assert!(stdout.contains("overall: PLAN (exit 0)"), "{stdout}");
    assert!(!stdout.contains("api: PASS"), "{stdout}");
}

#[test]
fn workspace_sensor_probe() {
    assert!(
        std::env::var_os(WORKSPACE_FAIL_ENV).is_none(),
        "intentional advisory sensor failure"
    );
    let Ok(lock_path) = std::env::var(WORKSPACE_LOCK_ENV) else {
        return;
    };
    let lock_path = PathBuf::from(lock_path);
    let lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .expect("workspace verifiers overlapped despite --jobs 1");
    drop(lock);
    std::thread::sleep(Duration::from_millis(250));
    fs::remove_file(lock_path).expect("remove workspace probe lock");
}
