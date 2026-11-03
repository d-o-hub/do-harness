//! `verify --dry-run` must not spawn sensor processes or write harness state.
//!
//! The probe appends to `runs.log`, so "planned" versus "ran" is proven by a
//! process side effect rather than by the report's self-description.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use support::reuse::{
    PROBE, fixture, harness, log, probe_config, run, status, verify_code, verify_raw,
};

/// A dry run reports the plan, exits non-zero, and leaves no side effects.
#[test]
fn dry_run_plans_without_running_or_writing() {
    let (_dir, root) = fixture(&probe_config(PROBE));

    let (value, stderr) = verify_code(&root, &["--dry-run"], 1);

    assert_eq!(value["ok"], false, "a plan is not a pass: {value}");
    assert_eq!(value["sensors"][0]["execution"], "planned", "{value}");
    assert_eq!(value["sensors"][0]["ok"], false, "{value}");
    assert!(value["sensors"][0]["exit_code"].is_null(), "{value}");
    assert_eq!(log(&root), "", "the sensor process must not be spawned");
    assert!(
        stderr.contains("Dry run: 1 sensor(s) planned, 0 executed."),
        "expected an explicit plan footer: {stderr}"
    );
    assert!(
        !stderr.contains("All sensors passed."),
        "a plan must never read as a pass: {stderr}"
    );
    assert!(
        !root.join(".do-harness/evidence.verification.json").exists(),
        "a dry run must not write evidence"
    );
}

/// `--record` is suppressed too: the state database stays untouched.
#[test]
fn dry_run_record_writes_no_beats() {
    let (_dir, root) = fixture(&probe_config(PROBE));

    verify_code(&root, &["--dry-run", "--record"], 1);

    assert_eq!(log(&root), "");
    let (_, doc) = status(&root);
    assert_eq!(doc["state"], "missing", "no beat may be recorded: {doc}");
}

/// Text output names the plan and never claims a pass.
#[test]
fn dry_run_text_output_is_a_plan() {
    let (_dir, root) = fixture(&probe_config(PROBE));

    let mut cmd = harness(&root);
    cmd.args(["verify", "--set", "verification", "--dry-run"]);
    let (code, stdout, stderr) = run(&mut cmd);

    assert_eq!(code, Some(1), "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("PLAN  probe"), "stdout: {stdout}");
    assert!(
        stdout.contains("Dry run: 1 sensor(s) planned, 0 executed."),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains("All sensors passed."),
        "a plan must never read as a pass: {stdout}"
    );
    assert_eq!(log(&root), "");
}

/// A bless cannot be previewed, so the combination is a usage error.
#[test]
fn dry_run_rejects_bless() {
    let (_dir, root) = fixture(&probe_config(PROBE));

    let (code, _stdout, stderr) = verify_raw(&root, &["--dry-run", "--record", "--bless"]);

    assert_eq!(code, Some(2), "expected a usage error: {stderr}");
    assert!(
        stderr.contains("--bless cannot be combined with --dry-run"),
        "unexpected stderr: {stderr}"
    );
    assert_eq!(log(&root), "");
}
