//! Acceptance fixtures for the shipped `check-loc.sh` scope flags.
//!
//! The rust pack used to enforce the ceiling on `*.rs` only, so a repository
//! with a front-end inherited the blind spot unless it forked the script. These
//! fixtures drive the real binary and the real git repository: the scaffolded
//! script must keep its default scope, accept `--root`/`--ext` for other trees,
//! prune generated directories, and be selected by a web-only change.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::{TempDir, tempdir};

mod support;

use support::git_command;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs a git command inside `root`, asserting success.
fn git(root: &Path, args: &[&str]) {
    let status = git_command(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .expect("spawn git");
    assert!(
        status.success(),
        "git {args:?} failed in {}",
        root.display()
    );
}

/// `lines` lines of TypeScript-shaped source, so the file is a plausible
/// front-end module rather than a repeated filler line.
fn big_module(lines: usize) -> String {
    let mut body = String::new();
    for index in 0..lines {
        use std::fmt::Write as _;
        let _ = writeln!(body, "export const value{index}: number = {index};");
    }
    body
}

/// A committed repository with the rust pack's scaffolded scripts and config.
fn scaffolded_repo() -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    git(&root, &["init", "-q"]);
    let (code, stdout, stderr) = run(harness(&root).arg("init").arg("--language").arg("rust"));
    assert_eq!(code, Some(0), "init failed:\n{stdout}\n{stderr}");
    (dir, root)
}

/// Runs the scaffolded `check-loc.sh` in `root`, returning (exit code,
/// `stdout`, `stderr`).
fn loc_script(root: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let output = Command::new("bash")
        .arg("scripts/check-loc.sh")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run check-loc.sh");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Writes the front-end tree every fixture uses: one honest module, one that
/// breaches the ceiling, and two generated copies that must stay invisible.
fn write_frontend(root: &Path) {
    for (path, lines) in [
        ("web/components/small.ts", 20),
        ("web/components/big.ts", 600),
        ("web/node_modules/pkg/vendor.ts", 600),
        ("web/dist/bundle.ts", 600),
    ] {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, big_module(lines)).unwrap();
    }
}

/// The default scope stays Rust-only: a front-end tree neither fails nor is
/// named, so existing repositories keep their current verdicts.
// Executing the script directly is Unix-only: on Windows a bare `bash` may
// resolve to the WSL stub, so the CLI's own shell resolution is what the
// `verify --changed` fixture below exercises there.
#[cfg(unix)]
#[test]
fn default_scope_ignores_frontend_sources() {
    let (_dir, root) = scaffolded_repo();
    write_frontend(&root);

    let (code, stdout, _) = loc_script(&root, &[]);
    assert_eq!(code, Some(0), "default scope must stay green:\n{stdout}");
    assert!(stdout.contains("FINDINGS: 0"), "{stdout}");
    assert!(
        !stdout.contains("big.ts"),
        "front-end must stay out: {stdout}"
    );
}

/// `--root`/`--ext` bring another tree under the ceiling, pruned of generated
/// directories, while reporting the unchanged `FINDINGS` contract.
// Executing the script directly is Unix-only: on Windows a bare `bash` may
// resolve to the WSL stub, so the CLI's own shell resolution is what the
// `verify --changed` fixture below exercises there.
#[cfg(unix)]
#[test]
fn scoped_scan_covers_frontend_and_prunes_generated_trees() {
    let (_dir, root) = scaffolded_repo();
    write_frontend(&root);

    let (code, stdout, _) = loc_script(&root, &["--root", "web", "--ext", "ts,tsx"]);
    assert_eq!(code, Some(1), "scoped scan must fail:\n{stdout}");
    assert!(
        stdout.contains("web/components/big.ts has 600 lines (max 500)"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("node_modules") && !stdout.contains("dist/bundle.ts"),
        "generated trees must be pruned: {stdout}"
    );
    assert!(stdout.contains("FINDINGS: 1"), "{stdout}");
    assert!(
        !stdout.contains("SPAN:") && !stdout.contains("CODE:"),
        "Rust-only hints must not be fabricated for other extensions: {stdout}"
    );
}

/// A mistyped root fails closed instead of reading as a vacuous green: the
/// sensor would otherwise enforce nothing while reporting `FINDINGS: 0`.
// Executing the script directly is Unix-only: on Windows a bare `bash` may
// resolve to the WSL stub, so the CLI's own shell resolution is what the
// `verify --changed` fixture below exercises there.
#[cfg(unix)]
#[test]
fn missing_configured_root_is_an_error() {
    let (_dir, root) = scaffolded_repo();
    write_frontend(&root);

    let (code, stdout, stderr) = loc_script(&root, &["--root", "webb", "--ext", "ts"]);
    assert_eq!(
        code,
        Some(2),
        "a typo must fail closed:\n{stdout}\n{stderr}"
    );
    assert!(stderr.contains("--root webb does not exist"), "{stderr}");
    assert!(
        !stdout.contains("FINDINGS"),
        "no verdict on a bad scope: {stdout}"
    );
}

/// A web-only change selects the scoped sensor and enforces the ceiling through
/// `verify`, which is the criterion a forked script used to satisfy by hand.
#[test]
fn verify_changed_selects_scoped_loc_for_a_web_edit() {
    let (_dir, root) = scaffolded_repo();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"
language = "generic"

[signal-sets]
all = ["loc-frontend"]

[[sensors]]
name = "loc-frontend"
argv = ["bash", "scripts/check-loc.sh", "--root", "web", "--ext", "ts"]
when-changed = ["web/**/*.ts"]
"#,
    )
    .unwrap();
    write_frontend(&root);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);

    // Only the front-end module changes.
    let edited = root.join("web/components/big.ts");
    let mut body = std::fs::read_to_string(&edited).unwrap();
    body.push_str("export const extra = true;\n");
    std::fs::write(&edited, body).unwrap();

    let (code, stdout, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(1), "the ceiling must fail:\n{stdout}\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    let names: Vec<&str> = report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .map(|sensor| sensor["name"].as_str().expect("sensor name"))
        .collect();
    assert_eq!(names, vec!["loc-frontend"], "web edit must select it");
    assert_eq!(report["failed"], serde_json::json!(["loc-frontend"]));

    // A Rust-only change does not select it: the web edit is committed first so
    // the working tree holds only the Rust source.
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: web edit"]);
    std::fs::write(root.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "unrelated change must stay green:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(
        report["sensors"].as_array().expect("sensors array").len(),
        0,
        "a Rust-only edit must not select the scoped sensor: {stdout}"
    );
}
