//! Shared fixtures for the `pr ready` acceptance tests.
//!
//! Each fixture drives the real `do-harness` binary against a fake `gh` on
//! `PATH`, so readiness evaluation, exit codes, and report rendering are all
//! exercised without network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Creates a fake `gh` script that dispatches API and view queries based on
/// mock JSON data written in `mock_dir`.
pub fn fake_gh_router(mock_dir: &Path) -> PathBuf {
    let bin = mock_dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    let script_content = format!(
        r#"#!/bin/sh
MOCK_DIR="{}"
# gh pr view ...
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
    cat "$MOCK_DIR/view.json"
    exit 0
fi
# gh api ... — the endpoint is an argument, not `$2`: `--paginate` and
# `--jq` flags may precede it.
if [ "$1" = "api" ]; then
    endpoint=""
    for arg in "$@"; do
        case "$arg" in
            repos/*|graphql) endpoint="$arg" ;;
        esac
    done
    case "$endpoint" in
        *check-runs*)
            if [ -f "$MOCK_DIR/fail_api" ]; then
                echo "HTTP 403: rate limit exceeded" >&2
                exit 1
            fi
            if [ -f "$MOCK_DIR/check_runs.json" ]; then
                cat "$MOCK_DIR/check_runs.json"
            else
                # `--jq '.check_runs[]'` prints nothing when there are no runs.
                printf '%s' ''
            fi
            exit 0
            ;;
        *status*)
            if [ -f "$MOCK_DIR/status.json" ]; then
                cat "$MOCK_DIR/status.json"
            else
                printf '%s' '{{"statuses":[]}}'
            fi
            exit 0
            ;;
        *comments*)
            if [ -f "$MOCK_DIR/comments.json" ]; then
                cat "$MOCK_DIR/comments.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        graphql)
            # `gh` substitutes {{owner}}/{{repo}} only for `-F` fields; a raw `-f`
            # query ships the placeholders verbatim and resolves nothing, so
            # emulate that failure and let the flag regress loudly.
            for arg in "$@"; do
                if [ "$arg" = "-f" ]; then
                    printf '%s' '{{"data":{{"repository":null}},"errors":[{{"message":"Could not resolve to a Repository with the name {{owner}}/{{repo}}."}}]}}'
                    exit 0
                fi
            done
            if [ -f "$MOCK_DIR/graphql.json" ]; then
                cat "$MOCK_DIR/graphql.json"
            else
                printf '%s' '{{"data":{{"repository":{{"pullRequest":{{"reviewThreads":{{"nodes":[]}}}}}}}}}}'
            fi
            exit 0
            ;;
    esac
fi
echo "unknown fake gh call: $@" >&2
exit 1
"#,
        mock_dir.display()
    );
    std::fs::write(&script, script_content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

pub fn run_pr(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
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
