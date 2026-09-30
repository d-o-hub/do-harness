//! Shared fixtures for the `metrics pr` acceptance tests.
//!
//! The fake `gh` serves the committed documents under
//! `tests/fixtures/pr-metrics/` per endpoint, so fetching, measuring, caching,
//! and rendering are exercised without network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs the real binary against `root` with the fake `gh` first on `PATH`.
pub fn run_metrics(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let mut command = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    command
        .arg("--root")
        .arg(root)
        .args(["metrics", "pr"])
        .args(args)
        .env("PATH", format!("{}:{path}", bin.display()));
    let output = command.output().expect("run do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Copies the committed fixture documents into `dest`.
pub fn copy_fixtures(dest: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pr-metrics");
    for entry in std::fs::read_dir(source).expect("fixture directory") {
        let entry = entry.expect("fixture entry");
        let target = dest.join(entry.file_name());
        std::fs::copy(entry.path(), target).expect("copy fixture");
    }
}

/// Creates a fake `gh` that routes `gh api` reads to the fixture documents.
pub fn fake_gh_router(mock_dir: &Path) -> PathBuf {
    let bin = mock_dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    let script_content = format!(
        r#"#!/bin/sh
MOCK_DIR="{}"
if [ "$1" != "api" ]; then
    echo "unexpected gh invocation: $*" >&2
    exit 1
fi
endpoint=""
for arg in "$@"; do
    case "$arg" in
        repos/*) endpoint="$arg" ;;
    esac
done
case "$endpoint" in
    */pulls\?*) cat "$MOCK_DIR/pulls.json" ;;
    */pulls/268/commits*) cat "$MOCK_DIR/pr268-commits.json" ;;
    */pulls/269/commits*) cat "$MOCK_DIR/pr269-commits.json" ;;
    */pulls/270/commits*) cat "$MOCK_DIR/pr270-commits.json" ;;
    */commits/c8188dca*/check-runs*) cat "$MOCK_DIR/pr269-check-runs.json" ;;
    */commits/14dc0b76*/check-runs*) cat "$MOCK_DIR/pr268-check-runs.json" ;;
    */commits/aaaa1111*/check-runs*) cat "$MOCK_DIR/pr270-check-runs.json" ;;
    */actions/runs*head_sha=c8188dca*) cat "$MOCK_DIR/pr269-runs.json" ;;
    */actions/runs*head_sha=14dc0b76*) cat "$MOCK_DIR/pr268-runs.json" ;;
    */actions/runs*head_sha=aaaa1111*) cat "$MOCK_DIR/pr270-runs.json" ;;
    */issues/268/comments*) cat "$MOCK_DIR/pr268-comments.json" ;;
    */issues/269/comments*) cat "$MOCK_DIR/pr269-comments.json" ;;
    */issues/270/comments*) cat "$MOCK_DIR/pr270-comments.json" ;;
    *) echo "unrouted endpoint: $endpoint" >&2; exit 1 ;;
esac
exit 0
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
