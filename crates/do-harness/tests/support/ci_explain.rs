//! Shared fixtures for the `ci-explain` acceptance tests.
//!
//! Each fixture drives the real `do-harness` binary against a fake `gh` on
//! `PATH`, so endpoint selection, flag shapes, and report rendering are all
//! exercised without network access.

#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

pub fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// A repository root with the two sensors the fixtures map onto.
pub fn fixture_root(config_extra: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(
        root.join("do-harness.toml"),
        format!(
            r#"[[sensors]]
name = "check"
argv = ["cargo", "check", "--workspace"]

[[sensors]]
name = "clippy"
argv = ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]
{config_extra}"#
        ),
    )
    .unwrap();
    (dir, root)
}

/// Fake `gh` dispatching the run/job/annotation/log queries `ci-explain` makes.
pub fn fake_gh(root: &Path) -> PathBuf {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    std::fs::write(
        &script,
        format!(
            r#"#!/bin/sh
MOCK_DIR="{}"
case "$1" in
    api)
        # Endpoint selection ignores `--paginate`; `--slurp` needs gh >= 2.48,
        # which the repository's accepted gh (2.45) does not have.
        for arg in "$@"; do
            if [ "$arg" = "--slurp" ]; then
                echo "fake gh: --slurp requires gh >= 2.48" >&2
                exit 1
            fi
        done
        endpoint=""
        for arg in "$@"; do
            case "$arg" in
                repos/*) endpoint="$arg" ;;
            esac
        done
        case "$endpoint" in
            */actions/jobs/*/logs*)
                # Job-log fallback for `gh run view` producing nothing.
                if [ -f "$MOCK_DIR/joblog.txt" ]; then
                    cat "$MOCK_DIR/joblog.txt"
                    exit 0
                fi
                exit 1
                ;;
            */jobs*)
                cat "$MOCK_DIR/jobs.json"
                exit 0
                ;;
            */annotations*)
                if [ -f "$MOCK_DIR/annotations.json" ]; then
                    cat "$MOCK_DIR/annotations.json"
                else
                    printf '%s' '[]'
                fi
                exit 0
                ;;
            */actions/runs/*)
                cat "$MOCK_DIR/run.json"
                exit 0
                ;;
        esac
        ;;
    run)
        shift
        # gh run view --job <id> --log-failed
        while [ "$#" -gt 0 ]; do
            if [ "$1" = "--job" ]; then
                job="$2"
                shift 2
                continue
            fi
            shift
        done
        if [ -f "$MOCK_DIR/log-$job.txt" ]; then
            cat "$MOCK_DIR/log-$job.txt"
            exit 0
        fi
        printf '%s' ''
        exit 0
        ;;
esac
echo "unknown fake gh call: $@" >&2
exit 1
"#,
            root.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

pub fn run_ci(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = harness(root)
        .args(args)
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

pub fn report(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("ci-explain --format json must emit JSON")
}
