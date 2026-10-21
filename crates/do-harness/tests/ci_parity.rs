//! CI parity: tool-dependent sensors fail open locally and closed when required.
//!
//! The scripts resolve tools from `PATH`. Each check runs its script with a
//! private `bin` as the whole `PATH`, so `cargo-audit`/`cargo-deny`/`cargo-hack`
//! /`cargo-machete` are missing no matter what the host installed under
//! `/usr/bin`, and asserts the documented policy: WARN skip locally, FAIL when
//! `CI=true` or `DO_HARNESS_REQUIRE_TOOLS=1`. Nothing here skips silently.
//!
//! The `docs-*` sensors are the exception to the skip-locally half: their
//! command is `python3 scripts/docs.py <check> --check`, and `scripts/docs.py`
//! has no `SKIP:` path, so a missing dependency is RED in every environment.
//! Their cases below pin that instead of a warned pass.
//!
//! The `action-pins` cases live in the `action_pins` child module: that
//! sensor needs two tools (an interpreter and a YAML parser), so its parity
//! matrix is longer than the other sensors' and would push this file past
//! the decomposition threshold.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, Output};

#[cfg(unix)]
#[path = "ci_parity/action_pins.rs"]
mod action_pins;

#[cfg(unix)]
fn script(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts")
        .join(name)
}

/// The checkout root the sensors run from (the crate sits two levels down).
#[cfg(unix)]
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Resolves `name` through `PATH` (then `/usr/bin`, `/bin`), panicking when it
/// is absent: the docs cases below need an interpreter for `scripts/docs.py`,
/// so a host without one cannot exercise their policy.
#[cfg(unix)]
fn path_utility(name: &str) -> PathBuf {
    if let Ok(output) = Command::new("pyenv").args(["which", name]).output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                let path = PathBuf::from(path_str);
                if path.is_file() {
                    return path;
                }
            }
        }
    }
    std::env::var_os("PATH")
        .and_then(|value| {
            std::env::split_paths(&value)
                .map(|dir| dir.join(name))
                .find(|candidate| candidate.is_file())
        })
        .unwrap_or_else(|| system_utility(name))
}

/// Makes `bin` the command's whole `PATH` — so a host-installed tool cannot
/// change the outcome — and sets exactly the documented CI markers.
#[cfg(unix)]
fn sensor_env(cmd: &mut Command, bin: &std::path::Path, ci: bool, require_tools: bool) {
    cmd.env("PATH", bin);
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
}

/// Resolves a system utility that a private `bin` needs to copy in.
#[cfg(unix)]
fn system_utility(utility: &str) -> std::path::PathBuf {
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
        sensor_env(&mut cmd, &bin, ci, require_tools);
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

/// Runs the docs sensor line (`python3 scripts/docs.py <check> --check`) from
/// the checkout root with a private `bin` as the whole `PATH`, so `python3` is
/// missing however the host installed it.
#[cfg(unix)]
fn run_docs_without_python(
    root: &std::path::Path,
    bin: &std::path::Path,
    check: &str,
    ci: bool,
    require_tools: bool,
) -> Output {
    let mut cmd = Command::new("/bin/bash");
    cmd.arg("-c")
        .arg(format!("python3 scripts/docs.py {check} --check"));
    cmd.current_dir(root);
    sensor_env(&mut cmd, bin, ci, require_tools);
    cmd.output().expect("spawn docs.py")
}

/// The docs sensors replaced `check-markdown-links.sh`, and they keep its
/// fail-closed half without its skip half: `scripts/docs.py` has no `SKIP:`
/// path, so a missing `python3` cannot become a warned pass. Neither marker is
/// emitted and the run exits non-zero under `CI=true`,
/// `DO_HARNESS_REQUIRE_TOOLS=1`, and locally.
#[cfg(unix)]
#[test]
fn docs_sensors_fail_closed_without_python() {
    let root = repository_root();
    let temp = tempfile::tempdir().expect("create temp dir");
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).expect("create private bin");

    for check in ["links", "sync", "examples"] {
        for (label, ci, require_tools) in [
            ("local", false, false),
            ("CI=true", true, false),
            ("DO_HARNESS_REQUIRE_TOOLS=1", false, true),
        ] {
            let output = run_docs_without_python(&root, &bin, check, ci, require_tools);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                !output.status.success(),
                "docs-{check}: {label} run must fail closed without python3: \
                 stdout={stdout} stderr={stderr}"
            );
            assert!(
                !stdout.contains("SKIP:") && !stderr.contains("SKIP:"),
                "docs-{check}: {label} run must never emit the SKIP: marker: \
                 stdout={stdout} stderr={stderr}"
            );
            assert!(
                stderr.contains("python3"),
                "docs-{check}: {label} run must name the missing interpreter: stderr={stderr}"
            );
        }
    }
}

/// Runs `docs.py sync --check` with an interpreter that exists but no `cargo`
/// on `PATH`: the sync check's second dependency is the Rust toolchain.
#[cfg(unix)]
fn run_sync_without_cargo(
    python: &std::path::Path,
    root: &std::path::Path,
    bin: &std::path::Path,
    ci: bool,
    require_tools: bool,
) -> Output {
    let mut cmd = Command::new(python);
    cmd.arg(script("docs.py")).arg("sync").arg("--check");
    cmd.current_dir(root);
    sensor_env(&mut cmd, bin, ci, require_tools);
    cmd.output().expect("spawn docs.py")
}

/// `docs-sync` reaches the crate's generated-docs maintenance test through
/// `cargo`, so the interpreter alone is not enough. With `cargo` absent the
/// check names the missing tool and exits 2 — a failure, never a green skip.
#[cfg(unix)]
#[test]
fn docs_sync_fails_closed_without_cargo() {
    let root = repository_root();
    let python = path_utility("python3");
    let temp = tempfile::tempdir().expect("create temp dir");
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).expect("create private bin");
    std::fs::copy(system_utility("bash"), bin.join("bash")).unwrap();

    for (label, ci, require_tools) in [
        ("local", false, false),
        ("CI=true", true, false),
        ("DO_HARNESS_REQUIRE_TOOLS=1", false, true),
    ] {
        let output = run_sync_without_cargo(&python, &root, &bin, ci, require_tools);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(2),
            "docs-sync: {label} run must fail closed without cargo: stdout={stdout} stderr={stderr}"
        );
        assert!(
            stderr.contains("cargo not found on PATH"),
            "docs-sync: {label} run must name the missing tool: stderr={stderr}"
        );
        assert!(
            !stdout.contains("SKIP:") && !stderr.contains("SKIP:"),
            "docs-sync: {label} run must never emit the SKIP: marker: \
             stdout={stdout} stderr={stderr}"
        );
    }
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
