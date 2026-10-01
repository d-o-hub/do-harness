//! Test suite for `scripts/check-privacy.sh` (privacy sensor).
//!
//! Verifies acceptance criteria:
//! - A template file containing a real-looking email fails with `file:line`.
//! - Fresh `do-harness init` output passes the privacy sensor.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

mod support;

#[path = "../src/shell.rs"]
#[allow(dead_code)]
mod shell;

fn isolated_command(program: &str) -> Command {
    let mut command = Command::new(program);
    support::isolate_command(&mut command);
    command
}

fn harness(root: &std::path::Path) -> Command {
    let mut cmd = isolated_command(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

#[test]
fn privacy_sensor_rejects_real_email_in_templates() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script_path = repo_root.join("scripts/check-privacy.sh");

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    // Create a fake templates structure
    let template_dir = root.join("crates/do-harness/templates");
    std::fs::create_dir_all(&template_dir).unwrap();

    let bad_template = template_dir.join("sample.txt");
    std::fs::write(&bad_template, "author: john.doe@realcompany.com\n").unwrap();

    let mut cmd = shell::bash();
    support::clear_git_view(&mut cmd);

    let output = cmd
        .arg(&script_path)
        .arg(root)
        .output()
        .expect("run check-privacy.sh");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "check-privacy.sh should fail when a real-looking email is present in templates. Output:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    );

    // Standardize path separators for Windows stdout matching
    let normalized_stdout = stdout.replace('\\', "/");

    assert!(
        normalized_stdout.contains("crates/do-harness/templates/sample.txt:1: real or non-example email address 'john.doe@realcompany.com' found"),
        "expected file:line error message in stdout, got:\n{stdout}"
    );
    assert!(
        stdout.contains("FINDINGS: 1"),
        "expected FINDINGS: 1 in stdout, got:\n{stdout}"
    );
}

#[test]
fn fresh_init_output_passes_privacy_sensor() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script_path = repo_root.join("scripts/check-privacy.sh");

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    // Run do-harness init in a fresh directory
    let init_status = harness(root).arg("init").status().expect("run init");
    assert!(init_status.success(), "do-harness init failed");

    let mut cmd = shell::bash();
    support::clear_git_view(&mut cmd);

    // Execute check-privacy.sh against the fresh init workspace
    let output = cmd
        .arg(&script_path)
        .arg(root)
        .output()
        .expect("run check-privacy.sh");

    assert!(
        output.status.success(),
        "fresh init output should pass check-privacy.sh, but failed with:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("FINDINGS: 0"),
        "expected FINDINGS: 0, got:\n{stdout}"
    );
}
