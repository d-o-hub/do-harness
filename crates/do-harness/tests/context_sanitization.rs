//! Test suite for the optional context-sanitization check.
//!
//! The check is a process boundary over a third-party executable, so every
//! case runs the canonical script against an isolated fixture executable.
//! The fixtures are adversaries, not production fallbacks: a script that
//! answers with the wrong shape, the wrong encoding, or the raw sentinel must
//! fail the check, never pass it.
//!
//! Two properties matter most and are proved here: a disabled check reports
//! `SKIP` and is not a pass, and neither the terminal output nor the
//! persisted error signature can carry the child's output or the raw fixture.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod support;

/// The canonical helper script the harness ships.
fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates/scripts/check-context-sanitization.py")
}

/// The template's own copy of the script, as shipped to adopters.
fn template_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates/scripts/check-context-sanitization.py")
}

/// Runs the canonical script with the given environment, returning the
/// output.
fn run(env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new("python3");
    cmd.arg(template_script());
    support::isolate_command(&mut cmd);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.output().expect("spawn check-context-sanitization.py")
}

/// Writes an executable fixture that answers with the given script body.
fn fixture_executable(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("write fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");
    }
    path
}

/// A shell fixture that echoes a fixed body and exits with a fixed code.
fn shell_fixture(body: &str, code: i32) -> String {
    format!("#!/bin/sh\n{body}\nexit {code}\n")
}

/// The disabled mode reports a skip and exits zero.
#[test]
fn disabled_mode_skips() {
    let output = run(&[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "disabled must exit zero: {stdout}"
    );
    assert!(
        stdout.contains("SKIP: context-sanitization: not configured"),
        "disabled must report the documented skip: {stdout}"
    );
    assert!(
        !stdout.contains("PASS:"),
        "a disabled check must not report a pass: {stdout}"
    );
}

/// An explicit `disabled` behaves like an unset mode.
#[test]
fn explicit_disabled_mode_skips() {
    let output = run(&[("DO_HARNESS_CONTEXT_SANITIZATION", "disabled")]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "disabled must exit zero: {stdout}"
    );
    assert!(
        stdout.contains("SKIP: context-sanitization: not configured"),
        "disabled must report the documented skip: {stdout}"
    );
}

/// An invalid mode fails closed, with a fixed diagnostic.
#[test]
fn invalid_mode_fails() {
    let output = run(&[("DO_HARNESS_CONTEXT_SANITIZATION", "maybe")]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "an invalid mode must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: invalid mode"),
        "the diagnostic must be the fixed invalid-mode reason: {stdout}"
    );
}

/// Enabled without a configured executable fails closed, even without
/// `--strict`: the sensor is not usable, so it is not a pass.
#[test]
fn enabled_without_executable_fails() {
    let output = run(&[("DO_HARNESS_CONTEXT_SANITIZATION", "enabled")]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a missing executable must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: missing executable"),
        "the diagnostic must be the fixed missing-executable reason: {stdout}"
    );
}

/// Enabled with a path that does not exist fails closed.
#[test]
fn enabled_with_missing_path_fails() {
    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", "/nonexistent/do-context-shield"),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a missing path must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: missing executable"),
        "the diagnostic must be the fixed missing-executable reason: {stdout}"
    );
}

/// Enabled with a non-executable file fails closed.
#[test]
fn enabled_with_non_executable_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("not-executable");
    std::fs::write(&path, "not a program").expect("write file");

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a non-executable must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: missing executable"),
        "the diagnostic must be the fixed missing-executable reason: {stdout}"
    );
}

/// A child that exits non-zero fails the check.
#[test]
fn nonzero_child_exit_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_executable(dir.path(), "child.sh", &shell_fixture("cat >/dev/null", 3));

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a failing child must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: sanitizer invocation failed"),
        "the diagnostic must be the fixed invocation-failure reason: {stdout}"
    );
}

/// A child that produces no output fails the check: an empty response is not
/// a sanitized fixture.
#[test]
fn empty_output_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_executable(dir.path(), "child.sh", &shell_fixture("cat >/dev/null", 0));

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "an empty answer must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: malformed sanitized output"),
        "the diagnostic must be the fixed malformed-output reason: {stdout}"
    );
}

/// A child that answers with JSON fails: the contract is plain text.
#[test]
fn json_output_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_executable(
        dir.path(),
        "child.sh",
        &shell_fixture("cat >/dev/null; printf '{\"text\":\"ok\"}\\n'", 0),
    );

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a JSON answer must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: malformed sanitized output"),
        "the diagnostic must be the fixed malformed-output reason: {stdout}"
    );
}

/// A child that leaks the raw sentinel fails, and the diagnostic never
/// carries the leaked value.
#[test]
fn raw_sentinel_leakage_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_executable(
        dir.path(),
        "child.sh",
        &shell_fixture("cat; printf 'extra AKIAIOSFODNN7EXAMPLE\\n'", 0),
    );

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "a leak must fail: {stdout}");
    assert!(
        stdout.contains("FAIL: context-sanitization: raw sentinel leakage"),
        "the diagnostic must be the fixed leakage reason: {stdout}"
    );
    assert!(
        !stdout.contains("AKIAIOSFODNN7EXAMPLE") && !stderr.contains("AKIAIOSFODNN7EXAMPLE"),
        "the raw sentinel must not appear in the diagnostics: {stdout} {stderr}"
    );
}

/// A child that writes the raw sentinel to stderr fails, and its stderr is
/// never printed.
#[test]
fn stderr_leakage_fails_without_printing_the_child_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_executable(
        dir.path(),
        "child.sh",
        &shell_fixture("cat >/dev/null; printf 'AKIAIOSFODNN7EXAMPLE\\n' >&2", 0),
    );

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "a leak must fail: {stdout}");
    assert!(
        stdout.contains("FAIL: context-sanitization: raw sentinel leakage"),
        "the diagnostic must be the fixed leakage reason: {stdout}"
    );
    assert!(
        !stderr.contains("AKIAIOSFODNN7EXAMPLE"),
        "the child's stderr must not be printed: {stderr}"
    );
}

/// A child that hangs fails with the timeout reason instead of hanging the
/// verification run.
#[test]
fn hanging_child_fails_with_the_timeout_reason() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_executable(dir.path(), "child.sh", "#!/bin/sh\nsleep 120\n");

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "a hang must fail: {stdout}");
    assert!(
        stdout.contains("FAIL: context-sanitization: sanitizer timeout"),
        "the diagnostic must be the fixed timeout reason: {stdout}"
    );
}

/// A child that answers with a wrong anchor fails: the anchors bind the
/// answer to the input that was sent.
#[test]
fn wrong_anchor_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_executable(
        dir.path(),
        "child.sh",
        &shell_fixture(
            "cat >/dev/null; printf 'do-harness-canary-begin:deadbeef\\nContact email: __DO_PRIVATE_EMAIL_1_0123456789ABCDEF__ with AWS key __DO_PRIVATE_REDACTED__\\ndo-harness-canary-end:deadbeef\\n'",
            0,
        ),
    );

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        code_of(&output),
        Some(1),
        "a wrong anchor must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: malformed sanitized output"),
        "the diagnostic must be the fixed malformed-output reason: {stdout}"
    );
}

/// A child that answers with a constant string fails: a fixed answer is not
/// a transformation of the input.
#[test]
fn constant_output_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_executable(
        dir.path(),
        "child.sh",
        &shell_fixture("cat >/dev/null; printf 'ok\\n'", 0),
    );

    let output = run(&[
        ("DO_HARNESS_CONTEXT_SANITIZATION", "enabled"),
        ("DO_CONTEXT_SHIELD_BIN", path.to_str().expect("path")),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        code_of(&output),
        Some(1),
        "a constant answer must fail: {stdout}"
    );
    assert!(
        stdout.contains("FAIL: context-sanitization: malformed sanitized output"),
        "the diagnostic must be the fixed malformed-output reason: {stdout}"
    );
}

/// The exit code of a failed run.
fn code_of(output: &Output) -> Option<i32> {
    output.status.code()
}

/// The canonical script is the one the template ships, and the repository
/// copy is the one the sensor runs.
#[test]
fn the_canonical_script_is_the_template() {
    let canonical = script();
    let template = template_script();
    assert_eq!(canonical, template);
    let body = std::fs::read_to_string(&template).expect("read the template");
    assert!(
        body.contains("DO_HARNESS_CONTEXT_SANITIZATION"),
        "the script must read the documented mode variable"
    );
    assert!(
        body.contains("DO_CONTEXT_SHIELD_BIN"),
        "the script must read the documented executable variable"
    );
}
