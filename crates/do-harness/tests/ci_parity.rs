//! CI parity: tool-dependent sensors fail open locally and closed when required.
//!
//! The scripts resolve tools from `PATH`; the test strips Cargo's bin
//! directory so `cargo-audit`/`cargo-deny` are absent, then asserts the
//! documented policy: WARN skip locally, FAIL when `CI=true`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn script(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts")
        .join(name)
}

fn run(name: &str, required: bool) -> Output {
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(script(name));
    // Deliberately excludes ~/.cargo/bin so the tools are "missing".
    cmd.env("PATH", "/usr/bin:/bin");
    if required {
        cmd.env("CI", "true");
        cmd.env("DO_HARNESS_REQUIRE_TOOLS", "1");
    } else {
        cmd.env_remove("CI");
        cmd.env_remove("DO_HARNESS_REQUIRE_TOOLS");
    }
    cmd.output().expect("spawn script")
}

fn tool_hidden(name: &str) -> bool {
    Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .is_ok_and(|out| !out.status.success())
}

#[test]
fn audit_sensor_fails_closed_only_when_required() {
    if !tool_hidden("cargo-audit") {
        eprintln!("cargo-audit visible under /usr/bin:/bin; skipping parity check");
        return;
    }
    let local = run("check-audit.sh", false);
    assert!(
        local.status.success(),
        "local run must WARN-skip: {}",
        String::from_utf8_lossy(&local.stderr)
    );
    let ci = run("check-audit.sh", true);
    assert!(
        !ci.status.success(),
        "CI run must fail closed without cargo-audit"
    );
}

#[test]
fn deps_sensor_fails_closed_only_when_required() {
    if !tool_hidden("cargo-deny") {
        eprintln!("cargo-deny visible under /usr/bin:/bin; skipping parity check");
        return;
    }
    let local = run("check-deps.sh", false);
    assert!(
        local.status.success(),
        "local run must WARN-skip: {}",
        String::from_utf8_lossy(&local.stderr)
    );
    let ci = run("check-deps.sh", true);
    assert!(
        !ci.status.success(),
        "CI run must fail closed without cargo-deny/cargo tree"
    );
}

#[test]
fn markdownlint_sensor_fails_closed_only_when_required() {
    if !tool_hidden("markdownlint-cli2") || !tool_hidden("markdownlint") {
        eprintln!("markdownlint tool visible under /usr/bin:/bin; skipping parity check");
        return;
    }
    let local = run("check-markdownlint.sh", false);
    assert!(
        local.status.success(),
        "local run must WARN-skip: {}",
        String::from_utf8_lossy(&local.stderr)
    );
    let ci = run("check-markdownlint.sh", true);
    assert!(
        !ci.status.success(),
        "CI run must fail closed without markdownlint"
    );
}

#[test]
fn yamllint_sensor_fails_closed_only_when_required() {
    if !tool_hidden("yamllint") {
        eprintln!("yamllint visible under /usr/bin:/bin; skipping parity check");
        return;
    }
    let local = run("check-yamllint.sh", false);
    assert!(
        local.status.success(),
        "local run must WARN-skip: {}",
        String::from_utf8_lossy(&local.stderr)
    );
    let ci = run("check-yamllint.sh", true);
    assert!(
        !ci.status.success(),
        "CI run must fail closed without yamllint"
    );
}

#[test]
fn powerset_sensor_fails_closed_only_when_required() {
    if !tool_hidden("cargo-hack") {
        eprintln!("cargo-hack visible under /usr/bin:/bin; skipping parity check");
        return;
    }
    let local = run("check-powerset.sh", false);
    assert!(
        local.status.success(),
        "local run must WARN-skip: {}",
        String::from_utf8_lossy(&local.stderr)
    );
    let output = String::from_utf8_lossy(&local.stdout);
    assert!(
        output.contains("SKIP:"),
        "local run without cargo-hack must output SKIP: marker: {output}"
    );
    let ci = run("check-powerset.sh", true);
    assert!(
        !ci.status.success(),
        "CI run must fail closed without cargo-hack"
    );
}

#[test]
fn machete_sensor_fails_closed_only_when_required() {
    if !tool_hidden("cargo-machete") {
        eprintln!("cargo-machete visible under /usr/bin:/bin; skipping parity check");
        return;
    }
    let local = run("check-machete.sh", false);
    assert!(
        local.status.success(),
        "local run must WARN-skip: {}",
        String::from_utf8_lossy(&local.stderr)
    );
    let output = String::from_utf8_lossy(&local.stdout);
    assert!(
        output.contains("SKIP:"),
        "local run without cargo-machete must output SKIP: marker: {output}"
    );
    let ci = run("check-machete.sh", true);
    assert!(
        !ci.status.success(),
        "CI run must fail closed without cargo-machete"
    );
}
