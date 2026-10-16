//! CI parity: tool-dependent sensors fail open locally and closed when required.
//!
//! The scripts resolve tools from `PATH`. Each check runs its script with a
//! private `bin` as the whole `PATH`, so `cargo-audit`/`cargo-deny`/`cargo-hack`
//! /`cargo-machete` are missing no matter what the host installed under
//! `/usr/bin`, and asserts the documented policy: WARN skip locally, FAIL when
//! `CI=true` or `DO_HARNESS_REQUIRE_TOOLS=1`. Nothing here skips silently.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn script(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts")
        .join(name)
}

/// Resolves a system utility that a private `bin` needs to copy in.
#[cfg(unix)]
fn system_utility(utility: &str) -> std::path::PathBuf {
    use std::path::Path;

    ["/usr/bin", "/bin"]
        .into_iter()
        .map(|dir| Path::new(dir).join(utility))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("neither /usr/bin/{utility} nor /bin/{utility} is present"))
}

#[cfg(unix)]
#[test]
fn audit_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-audit.sh", &[]);
}

#[cfg(unix)]
#[test]
fn deps_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-deps.sh", &["grep", "bash"]);
}

/// Asserts a tool-dependent sensor fails open (`SKIP`) locally and closed
/// (`FAIL`) under `CI=true` or `DO_HARNESS_REQUIRE_TOOLS=1`, independent of
/// host tools.
///
/// The private `bin` is the script's whole `PATH`, so a host-installed
/// `cargo-audit`/`markdownlint-cli2`/… cannot change the outcome and the check
/// never needs to skip. Besides `dirname`, `utilities` names the system
/// programs a script reaches before its missing-tool branch: `check-deps.sh`
/// greps a manifest and shells out to `check-package-contract.sh`, so it needs
/// `grep` and `bash`; the others need neither.
#[cfg(unix)]
fn assert_missing_tool_policy(name: &str, utilities: &[&str]) {
    use std::fs;

    let temp = tempfile::tempdir().expect("create temp dir");
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).expect("create private bin");
    for utility in std::iter::once("dirname").chain(utilities.iter().copied()) {
        fs::copy(system_utility(utility), bin.join(utility))
            .unwrap_or_else(|err| panic!("copy {utility} into private bin: {err}"));
    }

    let run_scenario = |ci: bool, require_tools: bool| -> Output {
        let mut cmd = Command::new("/bin/bash");
        cmd.arg(script(name));
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
        "{name}: local run must SKIP without a lint runner: stdout={stdout} stderr={}",
        String::from_utf8_lossy(&local.stderr)
    );
    assert!(
        stdout.contains("SKIP:"),
        "{name}: local run must emit the SKIP: marker: {stdout}"
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
            "{name}: {label} run must fail closed without a lint runner: stdout={stdout} stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("FAIL:"),
            "{name}: {label} run must emit the FAIL: marker: {stdout}"
        );
    }
}

#[cfg(unix)]
#[test]
fn markdownlint_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-markdownlint.sh", &[]);
}

#[cfg(unix)]
#[test]
fn yamllint_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-yamllint.sh", &[]);
}

#[cfg(unix)]
#[test]
fn actionlint_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-actionlint.sh", &[]);
}

#[cfg(unix)]
#[test]
fn markdown_links_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-markdown-links.sh", &[]);
}

#[cfg(unix)]
#[test]
fn powerset_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-powerset.sh", &[]);
}

#[cfg(unix)]
#[test]
fn machete_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-machete.sh", &[]);
}
