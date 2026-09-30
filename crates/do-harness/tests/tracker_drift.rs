//! Acceptance fixture for the scaffolded `check-tracker-drift.sh` (#242).
//!
//! The reference `project-check` compares a status document's "N open PRs /
//! issues" claims with `gh`, reports each drift with its line number, and
//! declines to green when the tracker cannot be read.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::{TempDir, tempdir};

mod support;

use support::git_command;

/// Initializes a workspace (which scaffolds the reference script) and adds a
/// fake `gh` whose tracker counts come from the environment.
fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let status = git_command(&root)
        .args(["init", "-q", "-b", "main"])
        .status()
        .expect("git init");
    assert!(status.success());
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let gh = bin.join("gh");
    std::fs::write(
        &gh,
        "#!/bin/sh\ncase \"$1\" in\n  pr) printf '%s' \"${FAKE_OPEN_PRS:-0}\" ;;\n  \
         issue) printf '%s' \"${FAKE_OPEN_ISSUES:-3}\" ;;\n  *) echo \"unexpected fake gh call: $*\" >&2; exit 1 ;;\n\
         esac\nexit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .current_dir(&root)
        .args(["init", "--language", "generic"])
        .output()
        .expect("init");
    assert!(output.status.success(), "init failed");
    assert!(
        root.join("scripts/check-tracker-drift.sh").is_file(),
        "init must scaffold the reference drift check"
    );
    (dir, root, bin)
}

/// Runs the scaffolded drift check against `doc` with the fake tracker counts.
fn run_drift(root: &Path, bin: &Path, doc: &Path, env: [(&str, &str); 2]) -> (Option<i32>, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new("bash")
        .current_dir(root)
        .arg("scripts/check-tracker-drift.sh")
        .arg("--doc")
        .arg(doc)
        .env("PATH", format!("{}:{path}", bin.display()))
        .envs(env)
        .output()
        .expect("run check-tracker-drift.sh");
    (
        output.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[test]
fn stale_status_claims_are_reported_per_claim() {
    let (_dir, root, bin) = fixture();
    let doc = root.join("STATUS.md");
    std::fs::write(&doc, "# Status\n\nActive: 13 open PRs, 6 open issues.\n").unwrap();

    let (code, output) = run_drift(
        &root,
        &bin,
        &doc,
        [("FAKE_OPEN_PRS", "0"), ("FAKE_OPEN_ISSUES", "3")],
    );
    assert_eq!(code, Some(1), "drift must fail the check:\n{output}");
    assert!(
        output.contains("claims 13 open PRs, tracker reports 0"),
        "{output}"
    );
    assert!(
        output.contains("claims 6 open issues, tracker reports 3"),
        "{output}"
    );
    assert!(output.contains("FINDINGS: 2"), "{output}");
}

#[test]
fn matching_claims_green_the_check() {
    let (_dir, root, bin) = fixture();
    let doc = root.join("STATUS.md");
    std::fs::write(&doc, "Active: 0 open PRs, 3 open issues.\n").unwrap();

    let (code, output) = run_drift(
        &root,
        &bin,
        &doc,
        [("FAKE_OPEN_PRS", "0"), ("FAKE_OPEN_ISSUES", "3")],
    );
    assert_eq!(code, Some(0), "matching claims must pass:\n{output}");
    assert!(output.contains("FINDINGS: 0"), "{output}");
}

#[test]
fn missing_status_document_declines_to_green() {
    let (_dir, root, bin) = fixture();
    let missing = root.join("NO_SUCH_STATUS.md");

    let (code, output) = run_drift(
        &root,
        &bin,
        &missing,
        [("FAKE_OPEN_PRS", "0"), ("FAKE_OPEN_ISSUES", "0")],
    );
    assert_eq!(
        code,
        Some(1),
        "a missing document must not pass vacuously:\n{output}"
    );
    assert!(output.contains("not found"), "{output}");
    assert!(output.contains("FINDINGS: 1"), "{output}");
}
