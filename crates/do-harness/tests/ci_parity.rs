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

/// The sensor must fail open (`SKIP`) unless `CI=true` or
/// `DO_HARNESS_REQUIRE_TOOLS=1`, independent of host tool availability.
/// A private `bin` holding only `dirname` (needed for the script's root
/// resolution) makes every lint runner unreachable, including `npx`.
#[cfg(unix)]
#[test]
fn markdownlint_sensor_fails_closed_only_when_required() {
    use std::fs;
    use std::path::Path;

    let temp = tempfile::tempdir().expect("create temp dir");
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).expect("create private bin");
    let dirname = ["/usr/bin/dirname", "/bin/dirname"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
        .unwrap_or_else(|| panic!("neither /usr/bin/dirname nor /bin/dirname is present"));
    fs::copy(dirname, bin.join("dirname")).expect("copy dirname into private bin");

    let run_scenario = |ci: bool, require_tools: bool| -> Output {
        let mut cmd = Command::new("/bin/bash");
        cmd.arg(script("check-markdownlint.sh"));
        cmd.env("PATH", &bin);
        cmd.env_remove("BASH_ENV");
        cmd.env_remove("ENV");
        cmd.env_remove("CI");
        cmd.env_remove("DO_HARNESS_REQUIRE_TOOLS");
        if ci {
            cmd.env("CI", "true");
        }
        if require_tools {
            cmd.env("DO_HARNESS_REQUIRE_TOOLS", "1");
        }
        cmd.output().expect("spawn script")
    };

    let local = run_scenario(false, false);
    let stdout = String::from_utf8_lossy(&local.stdout);
    assert_eq!(
        local.status.code(),
        Some(0),
        "local run must SKIP without a lint runner: stdout={stdout} stderr={}",
        String::from_utf8_lossy(&local.stderr)
    );
    assert!(
        stdout.contains("SKIP:"),
        "local run must emit the SKIP: marker: {stdout}"
    );

    for (label, ci, require_tools) in [
        ("CI=true", true, false),
        ("DO_HARNESS_REQUIRE_TOOLS=1", false, true),
    ] {
        let output = run_scenario(ci, require_tools);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{label} run must fail closed without a lint runner: stdout={stdout} stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("FAIL:"),
            "{label} run must emit the FAIL: marker: {stdout}"
        );
    }
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
