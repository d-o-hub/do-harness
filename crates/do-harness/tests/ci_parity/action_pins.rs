//! CI-parity cases for the `action-pins` sensor.
//!
//! The sensor needs two things: an interpreter and a YAML parser. The
//! interpreter case is the wrapper's own `SKIP:`/`FAIL:` policy; the parser
//! case is the Python half's, because `python3` can exist while `PyYAML` does
//! not. Both must fail open locally and closed when the tools are required,
//! independently of host tools.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Output};

use super::{
    assert_missing_tool_policy, path_utility, repository_root, script, sensor_env, system_utility,
};

#[cfg(unix)]
#[test]
fn action_pins_sensor_fails_closed_only_when_required() {
    assert_missing_tool_policy("check-action-pins.sh", &[]);
}

/// The `action-pins` sensor needs two things: an interpreter and a YAML
/// parser. The interpreter case is the wrapper's own `SKIP:`/`FAIL:` policy;
/// the parser case is the Python half's, because `python3` can exist while
/// `PyYAML` does not.
///
/// `python3 -S` drops site-packages, which is exactly the state the CI
/// install step (`pip install -r
/// .agents/skills/skill-creator/scripts/requirements.txt`) is there to
/// prevent. The checker must apply the same policy to that state: SKIP
/// locally, FAIL when the tools are required.
#[cfg(unix)]
#[test]
fn action_pins_sensor_fails_closed_without_the_yaml_parser() {
    let python = path_utility("python3");
    let temp = tempfile::tempdir().expect("create temp dir");
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).expect("create private bin");

    let run_scenario = |ci: bool, require_tools: bool| -> Output {
        // The wrapper resolves its root with `dirname`, and the interpreter
        // must be reachable as `python3` through the private `bin`.
        std::fs::copy(system_utility("dirname"), bin.join("dirname")).unwrap();
        // `-S` drops site-packages, which is exactly the state the CI install
        // step prevents: the interpreter exists, the parser does not.
        let shim = bin.join("python3");
        std::fs::write(
            &shim,
            "#!/bin/sh\nexec \"$DO_HARNESS_TEST_PYTHON\" -S \"$@\"\n",
        )
        .expect("write python shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&shim)
                .expect("shim metadata")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&shim, permissions).expect("chmod shim");
        }

        let mut cmd = Command::new("/bin/bash");
        cmd.arg(script("check-action-pins.sh"));
        sensor_env(&mut cmd, &bin, ci, require_tools);
        cmd.env("DO_HARNESS_TEST_PYTHON", &python);
        cmd.output().expect("spawn check-action-pins.sh")
    };

    // The interpreter alone is not enough: the wrapper must report the
    // missing parser rather than a false pass.
    let local = run_scenario(false, false);
    assert!(
        local.status.success(),
        "a missing parser must be an advisory local skip"
    );
    let stdout = String::from_utf8_lossy(&local.stdout);
    let stderr = String::from_utf8_lossy(&local.stderr);
    assert!(
        !stdout.contains("FINDINGS: 0"),
        "a missing parser must not read as a clean check: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stderr.contains("PyYAML") || stdout.contains("PyYAML"),
        "the failure must name the missing parser: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("SKIP:") || stderr.contains("SKIP:"),
        "the local run must say it skipped: stdout={stdout} stderr={stderr}"
    );

    for (label, ci, require_tools) in [
        ("CI=true", true, false),
        ("DO_HARNESS_REQUIRE_TOOLS=1", false, true),
    ] {
        let output = run_scenario(ci, require_tools);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(1),
            "action-pins: {label} run must fail closed without the parser: \
             stdout={stdout} stderr={stderr}"
        );
        assert!(
            stdout.contains("FAIL:") && stdout.contains("PyYAML"),
            "action-pins: {label} run must explain the missing parser: \
             stdout={stdout} stderr={stderr}"
        );
        assert!(
            !stderr.contains("Traceback"),
            "action-pins: {label} run must not expose an import traceback: \
             stderr={stderr}"
        );
        assert!(
            !stdout.contains("FINDINGS: 0"),
            "action-pins: {label} run must not report a clean check without the parser: \
             stdout={stdout} stderr={stderr}"
        );
    }
}

/// The Python half owns the same policy, and it must hold when the checker
/// is invoked directly with `python3 -S` — the wrapper is not the only way
/// to reach it. `-S` drops site-packages, so `PyYAML` is absent while the
/// interpreter works: SKIP locally, FAIL when the tools are required.
#[cfg(unix)]
#[test]
fn action_pins_checker_applies_the_parser_policy_when_run_directly() {
    let root = repository_root();
    let python = path_utility("python3");
    let checker = root.join("scripts/check-action-pins.py");

    let run_scenario = |ci: bool, require_tools: bool| -> Output {
        let mut cmd = Command::new(python.to_string_lossy().as_ref());
        cmd.arg("-S").arg(&checker).arg("--root").arg(&root);
        // The interpreter is absolute, so the run does not depend on `PATH`:
        // the environment is cleared to the documented markers only, and the
        // checker must not read an ambient setting that changes its policy.
        cmd.env_clear();
        cmd.env("PATH", "");
        cmd.env_remove("BASH_ENV");
        cmd.env_remove("ENV");
        if ci {
            cmd.env("CI", "true");
        }
        if require_tools {
            cmd.env("DO_HARNESS_REQUIRE_TOOLS", "1");
        }
        cmd.output().expect("spawn check-action-pins.py")
    };

    let local = run_scenario(false, false);
    let stdout = String::from_utf8_lossy(&local.stdout);
    let stderr = String::from_utf8_lossy(&local.stderr);
    assert!(
        local.status.success(),
        "action-pins: the local direct run must skip without the parser: \
         stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("SKIP:") || stderr.contains("SKIP:"),
        "action-pins: the local direct run must say it skipped: \
         stdout={stdout} stderr={stderr}"
    );
    assert!(
        !stdout.contains("FINDINGS: 0"),
        "action-pins: a missing parser must not read as a clean check: \
         stdout={stdout} stderr={stderr}"
    );

    for (label, ci, require_tools) in [
        ("CI=true", true, false),
        ("DO_HARNESS_REQUIRE_TOOLS=1", false, true),
    ] {
        let output = run_scenario(ci, require_tools);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(1),
            "action-pins: {label} direct run must fail closed without the parser: \
             stdout={stdout} stderr={stderr}"
        );
        assert!(
            stdout.contains("FAIL:") && stdout.contains("PyYAML"),
            "action-pins: {label} direct run must explain the missing parser: \
             stdout={stdout} stderr={stderr}"
        );
        assert!(
            !stderr.contains("Traceback"),
            "action-pins: {label} direct run must not expose an import traceback: \
             stderr={stderr}"
        );
        assert!(
            !stdout.contains("FINDINGS: 0"),
            "action-pins: {label} direct run must not report a clean check without the parser: \
             stdout={stdout} stderr={stderr}"
        );
    }
}
