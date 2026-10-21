//! Real-CLI regression for the generated Rust policy's change selection.
//!
//! The unit tests in `crate::applicability` prove the selection logic against
//! the built-in pack and the repository's own policy. This file proves the
//! third case: the policy `do-harness init` writes for an adopter, exercised
//! through the real binary against a real workspace.
//!
//! The workspace is dependency-free on purpose. A fixture that downloaded
//! crates would test the network, not the selection.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

mod support;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    support::isolate_command(&mut cmd);
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a `do-harness` subcommand, returning (exit code, stdout, stderr).
fn run(root: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let output = harness(root).args(args).output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Writes a dependency-free workspace member with a unit test and a doctest.
///
/// The doctest is what makes the `doctest` sensor provable: without one the
/// sensor would run zero doctests, and a gate that executes nothing is not a
/// pass.
fn write_member(root: &Path, name: &str) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        format!(
            "//! The {name} member.\n\n/// Returns the member's answer.\n///\n/// # Example\n///\n/// ```\n/// assert_eq!({name}::answer(), 42);\n/// ```\npub fn answer() -> u32 {{\n    42\n}}\n\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn answer_is_stable() {{\n        assert_eq!(super::answer(), 42);\n    }}\n}}\n"
        ),
    )
    .unwrap();
}

/// Writes a workspace with two members: one under `crates/`, one outside it.
fn write_workspace(root: &Path) {
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/db\", \"tools/tooling\"]\n",
    )
    .unwrap();
    for (path, name) in [("crates/db", "db"), ("tools/tooling", "tooling")] {
        let member = root.join(path);
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(
            member.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .unwrap();
        write_member(&member, name);
    }
}

/// Commits everything under the root, so the change detection has a baseline.
///
/// The subject is a conventional commit: the generated policy's `commitlint`
/// sensor reads the tip commit, so a fixture subject like `baseline` would
/// turn the init baseline red for a reason that has nothing to do with the
/// selection under test.
fn commit_all(root: &Path) {
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "-m",
            "chore(fixture): baseline workspace",
        ],
    ] {
        let status = support::git_command(root)
            .args(&args)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} must succeed");
    }
}

/// Initializes a git repository at the root.
fn init_repository(root: &Path) {
    let status = support::git_command(root)
        .args(["init", "--quiet"])
        .status()
        .expect("run git init");
    assert!(status.success(), "git init must succeed");
}

/// The generated policy names the Rust sensors a member-manifest change must
/// select.
const RUST_SENSORS: [&str; 5] = ["fmt", "check", "clippy", "test", "doctest"];

/// Reads the selected sensor names out of an `explain --format json` report.
fn selected_sensors(stdout: &str) -> Vec<String> {
    let report: Value = serde_json::from_str(stdout)
        .unwrap_or_else(|err| panic!("explain stdout is not JSON ({err}):\n{stdout}"));
    report["selected"]
        .as_array()
        .expect("selected")
        .iter()
        .filter_map(|entry| entry["name"].as_str().map(ToOwned::to_owned))
        .collect()
}

/// The generated policy selects the Rust sensors when a workspace member's
/// manifest changes, and a malformed member manifest then fails the
/// compile check.
///
/// This is the adopter-facing contract: without it, an adopter's dependency,
/// feature, or lint-policy edit in a member manifest silently skips
/// compilation, tests, and doctests.
#[test]
fn generated_policy_selects_rust_sensors_for_a_member_manifest_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root);
    write_workspace(root);
    commit_all(root);

    // The generated policy comes from `init`, so the selection under test is
    // the one an adopter actually gets.
    let (code, _stdout, stderr) = run(root, &["init", "--format", "json"]);
    assert_eq!(code, Some(0), "init must succeed:\n{stderr}");
    commit_all(root);

    // A member manifest changes, and nothing else does.
    let manifest = root.join("crates/db/Cargo.toml");
    let mut contents = std::fs::read_to_string(&manifest).unwrap();
    contents.push_str("\n[features]\ntriage_canary = []\n");
    std::fs::write(&manifest, contents).unwrap();

    let (code, stdout, stderr) = run(
        root,
        &[
            "explain",
            "--set",
            "verification",
            "--changed",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "explain must succeed:\n{stderr}");
    let selected = selected_sensors(&stdout);
    for name in RUST_SENSORS {
        assert!(
            selected.iter().any(|sensor| sensor == name),
            "a member manifest change must select `{name}` (selected {selected:?})"
        );
    }

    // The report must name the changed path honestly: the member manifest
    // itself, not a fabricated root-manifest change.
    let report: Value = serde_json::from_str(&stdout).expect("explain json");
    let changed = report["changed_files"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        changed.iter().any(|entry| entry == "crates/db/Cargo.toml"),
        "the report must name the changed member manifest (changed {changed:?})"
    );

    // A malformed member manifest now fails the compile check.
    std::fs::write(&manifest, "[package\nname = \"db\"\n").unwrap();
    let (code, stdout, stderr) = run(
        root,
        &[
            "verify",
            "--set",
            "verification",
            "--changed",
            "--only",
            "check",
            "--format",
            "json",
        ],
    );
    assert_ne!(
        code,
        Some(0),
        "a malformed member manifest must fail:\n{stdout}\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("verify json");
    assert!(
        report["failed"]
            .as_array()
            .expect("failed")
            .iter()
            .any(|name| name == "check"),
        "the compile check must be reported as failed: {report}"
    );
}

/// A deleted member manifest still selects the Rust sensors, and the missing
/// member then fails the compile check.
#[test]
fn generated_policy_selects_rust_sensors_for_a_deleted_member_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root);
    write_workspace(root);
    commit_all(root);

    let (code, _stdout, stderr) = run(root, &["init", "--format", "json"]);
    assert_eq!(code, Some(0), "init must succeed:\n{stderr}");
    commit_all(root);

    std::fs::remove_file(root.join("crates/db/Cargo.toml")).unwrap();

    let (code, stdout, stderr) = run(
        root,
        &[
            "explain",
            "--set",
            "verification",
            "--changed",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "explain must succeed:\n{stderr}");
    let selected = selected_sensors(&stdout);
    for name in RUST_SENSORS {
        assert!(
            selected.iter().any(|sensor| sensor == name),
            "a deleted member manifest must select `{name}` (selected {selected:?})"
        );
    }

    let (code, stdout, stderr) = run(
        root,
        &[
            "verify",
            "--set",
            "verification",
            "--changed",
            "--only",
            "check",
            "--format",
            "json",
        ],
    );
    assert_ne!(
        code,
        Some(0),
        "a deleted member manifest must fail the compile check:\n{stdout}\n{stderr}"
    );
}

/// A Markdown-only change still skips the Rust sensors: the correction must
/// not turn every change into a full Rust toolchain run.
#[test]
fn generated_policy_skips_rust_sensors_for_a_markdown_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root);
    write_workspace(root);
    commit_all(root);

    let (code, _stdout, stderr) = run(root, &["init", "--format", "json"]);
    assert_eq!(code, Some(0), "init must succeed:\n{stderr}");
    commit_all(root);

    std::fs::write(root.join("README.md"), "# Fixture\n").unwrap();

    let (code, stdout, stderr) = run(
        root,
        &[
            "explain",
            "--set",
            "verification",
            "--changed",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "explain must succeed:\n{stderr}");
    let selected = selected_sensors(&stdout);
    for name in RUST_SENSORS {
        assert!(
            !selected.iter().any(|sensor| sensor == name),
            "a Markdown-only change must skip `{name}` (selected {selected:?})"
        );
    }
}
