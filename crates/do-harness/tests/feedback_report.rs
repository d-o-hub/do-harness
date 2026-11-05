//! Integration tests for CLI self-reporting to the tool's own repository.

#![allow(clippy::unwrap_used, clippy::expect_used)]
// The fake `gh` is a bash script, so these fixtures are Unix-only by
// construction (the Windows runner spawns `gh` directly and cannot exec a shebang).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

const EXEC_PERMS: u32 = 0o755;
const FIXTURE_REPO: &str = "acme/widget";

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    bin_dir: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let fake_gh = bin_dir.join("gh");
    let script = format!(
        r#"#!/bin/sh
ROOT="{}"
case "$1" in
    issue)
        case "$2" in
            list)
                if [ -f "$ROOT/mock_issues.json" ]; then
                    cat "$ROOT/mock_issues.json"
                else
                    echo "[]"
                fi
                exit 0
                ;;
            create)
                cat - > "$ROOT/last_created_body.txt"
                echo "$@" > "$ROOT/last_created_argv.txt"
                echo "https://github.com/acme/widget/issues/7"
                exit 0
                ;;
        esac
        ;;
esac
echo "unknown fake gh invocation: $@" >&2
exit 1
"#,
        root.display()
    );
    fs::write(&fake_gh, script).unwrap();
    let mut perms = fs::metadata(&fake_gh).unwrap().permissions();
    perms.set_mode(EXEC_PERMS);
    fs::set_permissions(&fake_gh, perms).unwrap();

    fs::write(
        root.join("do-harness.toml"),
        "language = \"rust\"\n[[sensors]]\nname = \"boom\"\nargv = [\"false\"]\n",
    )
    .unwrap();

    Fixture {
        _temp: temp,
        root,
        bin_dir,
    }
}

fn harness(fx: &Fixture) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(&fx.root);
    let old_path = std::env::var("PATH").unwrap_or_default();
    cmd.env("PATH", format!("{}:{}", fx.bin_dir.display(), old_path));
    cmd.env("DO_HARNESS_REPO", FIXTURE_REPO);
    cmd.env_remove("DO_HARNESS_AUTO_REPORT");
    cmd
}

#[test]
fn report_files_an_issue_with_details_and_log_tail() {
    let fx = fixture();
    let log = fx.root.join("failure.log");
    fs::write(&log, "line one\nline two\n").unwrap();

    let out = harness(&fx)
        .args([
            "feedback",
            "report",
            "--error",
            "boom: internal failure",
            "--log",
            log.to_str().unwrap(),
            "--yes",
        ])
        .output()
        .expect("run feedback report");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body = fs::read_to_string(fx.root.join("last_created_body.txt")).unwrap();
    assert!(body.contains("boom: internal failure"));
    assert!(body.contains("line two"));
    assert!(body.contains("do-harness-feedback-fingerprint"));
    let argv = fs::read_to_string(fx.root.join("last_created_argv.txt")).unwrap();
    assert!(argv.contains(FIXTURE_REPO));
}

#[test]
fn report_dry_run_does_not_create() {
    let fx = fixture();
    let out = harness(&fx)
        .args(["feedback", "report", "--error", "boom", "--dry-run"])
        .output()
        .expect("run feedback report");

    assert!(out.status.success());
    assert!(!fx.root.join("last_created_body.txt").exists());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Dry run"));
}

#[test]
fn report_links_an_existing_fingerprint() {
    let fx = fixture();
    let first = harness(&fx)
        .args(["feedback", "report", "--error", "boom", "--yes"])
        .output()
        .expect("first report");
    assert!(first.status.success());

    let second = harness(&fx)
        .args(["feedback", "report", "--error", "boom", "--yes"])
        .output()
        .expect("second report");
    assert!(second.status.success());
    assert!(String::from_utf8_lossy(&second.stdout).contains("Linked existing issue #7"));
}

#[test]
fn report_links_across_volatile_error_text() {
    let fx = fixture();
    let first = harness(&fx)
        .args([
            "feedback",
            "report",
            "--error",
            "verify is in flight (pid 9539, since 1793874401)",
            "--yes",
        ])
        .output()
        .expect("first report");
    assert!(first.status.success());

    let second = harness(&fx)
        .args([
            "feedback",
            "report",
            "--error",
            "verify is in flight (pid 9412, since 1793874399)",
            "--yes",
        ])
        .output()
        .expect("second report");
    assert!(second.status.success());
    assert!(
        String::from_utf8_lossy(&second.stdout).contains("Linked existing issue #7"),
        "a pid or timestamp change must not file a second issue: {}",
        String::from_utf8_lossy(&second.stdout)
    );
}

#[test]
fn automatic_mode_files_a_cli_failure() {
    let fx = fixture();
    let out = harness(&fx)
        .env("DO_HARNESS_AUTO_REPORT", "all")
        .args(["split", "/nonexistent/file.rs"])
        .output()
        .expect("run split");

    assert!(!out.status.success());
    let body = fs::read_to_string(fx.root.join("last_created_body.txt")).unwrap();
    assert!(body.contains("split"));
    assert!(body.contains("do-harness-feedback-fingerprint"));
}

#[test]
fn automatic_mode_skips_usage_failures_by_default() {
    let fx = fixture();
    let out = harness(&fx)
        .args(["split", "/nonexistent/file.rs"])
        .output()
        .expect("run split");

    assert!(!out.status.success());
    assert!(!fx.root.join("last_created_body.txt").exists());
}

#[test]
fn automatic_mode_skips_sensor_failures_by_default() {
    let fx = fixture();
    let out = harness(&fx).arg("verify").output().expect("run verify");

    assert!(!out.status.success());
    assert!(!fx.root.join("last_created_body.txt").exists());
}

#[test]
fn automatic_mode_can_be_disabled() {
    let fx = fixture();
    let out = harness(&fx)
        .env("DO_HARNESS_AUTO_REPORT", "off")
        .arg("verify")
        .output()
        .expect("run verify");

    assert!(!out.status.success());
    assert!(!fx.root.join("last_created_body.txt").exists());
}

#[test]
fn automatic_report_carries_the_cli_diagnostics() {
    let fx = fixture();
    let out = harness(&fx)
        .env("DO_HARNESS_AUTO_REPORT", "all")
        .arg("verify")
        .output()
        .expect("run verify");

    assert!(!out.status.success());
    let body = fs::read_to_string(fx.root.join("last_created_body.txt")).unwrap();
    assert!(body.contains("FAIL  boom"), "body: {body}");
    assert!(body.contains("Failed sensors: boom"), "body: {body}");
}

#[test]
fn report_warning_classifies_the_issue() {
    let fx = fixture();
    let out = harness(&fx)
        .args([
            "feedback",
            "report",
            "--error",
            "slow sensor",
            "--warning",
            "--yes",
        ])
        .output()
        .expect("run feedback report");

    assert!(out.status.success());
    let body = fs::read_to_string(fx.root.join("last_created_body.txt")).unwrap();
    assert!(body.contains("reported a warning"));
}

#[test]
fn automatic_reporting_failure_preserves_the_exit_code() {
    let fx = fixture();
    let baseline = harness(&fx)
        .args(["split", "/nonexistent/file.rs"])
        .output()
        .expect("baseline run");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(&fx.root);
    cmd.env("PATH", "/nonexistent");
    cmd.env("DO_HARNESS_REPO", FIXTURE_REPO);
    cmd.env("DO_HARNESS_AUTO_REPORT", "all");
    cmd.args(["split", "/nonexistent/file.rs"]);
    let reported = cmd.output().expect("reported run");

    assert!(!baseline.status.success());
    assert_eq!(reported.status.code(), baseline.status.code());
}
