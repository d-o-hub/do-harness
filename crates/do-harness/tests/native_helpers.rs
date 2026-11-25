//! Parity fixtures: the native helper subcommands and their POSIX-shell
//! wrapper scripts must agree on the same fixture — same exit code, same
//! verdict lines. This is the test that pins the "both exist, same contract"
//! guarantee the Windows portability work rests on.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

#[path = "../src/shell.rs"]
#[allow(dead_code)]
mod shell;

/// The harness binary under test (cargo sets this for integration tests).
fn harness() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_do-harness"))
}

/// Removes hook-inherited git view so fixtures resolve their own history.
fn isolate(command: &mut Command) -> &mut Command {
    support::isolate_command(command)
}

/// Runs the shell wrapper script in `root`.
fn run_shell(root: &Path, script: &str, args: &[&str]) -> (Option<i32>, String, String) {
    let mut bash = shell::bash();
    let command = isolate(&mut bash)
        .arg(root.join("scripts").join(script))
        .args(args)
        .current_dir(root)
        .env("DO_HARNESS_BIN", harness())
        .env_remove("DO_HARNESS_COMMITLINT_COUNT");
    let output = command.output().expect("run shell helper");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs the native helper subcommand in `root`.
fn run_native(root: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(harness());
    isolate(&mut cmd);
    cmd.arg("--root").arg(root).args(args).current_dir(root);
    let output = cmd.output().expect("run native helper");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Creates a fixture repo with the wrapper scripts, returning (dir, root).
fn fixture_with_scripts() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir(root.join("scripts")).unwrap();
    for script in ["check-commitlint.sh", "check-deps.sh", "check-audit.sh"] {
        let source = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../scripts/{script}")),
        )
        .unwrap();
        std::fs::write(root.join("scripts").join(script), source).unwrap();
    }
    (dir, root)
}

/// Runs `git` in `root` with pinned identity.
fn git(root: &Path, args: &[&str]) {
    let output = isolate(&mut Command::new("git"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Commits an empty change with a pinned date.
fn commit(root: &Path, subject: &str, date: &str) {
    git(
        root,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            subject,
            "--date",
            date,
        ],
    );
}

/// A conventional subject the shell and native linters both accept.
#[test]
fn native_and_shell_commitlint_accept_conventional_lowercase() {
    let (_dir, root) = fixture_with_scripts();
    git(&root, &["init", "-q"]);
    commit(&root, "fix: typo", "2024-01-01T00:00:00");

    let shell = run_shell(&root, "check-commitlint.sh", &[]);
    let native = run_native(&root, &["helpers", "commitlint"]);

    assert_eq!(shell.0, native.0, "exit codes must agree");
    assert_eq!(shell.1, native.1, "stdout verdicts must agree");
    assert_eq!(shell.2, native.2, "stderr must agree");
    assert_eq!(shell.0, Some(0));
    assert!(shell.1.contains("check-commitlint OK: 1 subject(s)"));
}

/// A non-conventional subject blocks in both, with the same FAIL line.
#[test]
fn native_and_shell_commitlint_reject_non_conventional() {
    let (_dir, root) = fixture_with_scripts();
    git(&root, &["init", "-q"]);
    commit(&root, "not conventional at all", "2024-01-01T00:00:00");

    let shell = run_shell(&root, "check-commitlint.sh", &[]);
    let native = run_native(&root, &["helpers", "commitlint"]);

    assert_eq!(shell.0, native.0);
    assert_eq!(shell.1, native.1);
    assert_eq!(shell.2, native.2);
    assert_eq!(shell.0, Some(1));
    assert!(
        shell
            .1
            .contains("FAIL: non-conventional commit subject: not conventional at all")
    );
}

/// The lowercase rule is enforced identically, including with a scope.
#[test]
fn native_and_shell_commitlint_enforce_lowercase_scope() {
    let (_dir, root) = fixture_with_scripts();
    git(&root, &["init", "-q"]);
    commit(&root, "fix(scope): Typo in Scope", "2024-01-01T00:00:00");

    let shell = run_shell(&root, "check-commitlint.sh", &[]);
    let native = run_native(&root, &["helpers", "commitlint"]);

    assert_eq!(shell.0, native.0);
    assert_eq!(shell.1, native.1);
    assert_eq!(shell.2, native.2);
    assert_eq!(shell.0, Some(1));
    assert!(
        shell
            .1
            .contains("FAIL: commit subject is not lowercase: fix(scope): Typo in Scope")
    );
}

/// `--message` mode (the commit-msg hook path) agrees for good and bad.
#[test]
fn native_and_shell_commitlint_message_mode_agree() {
    let (_dir, root) = fixture_with_scripts();
    let good = root.join("good-msg");
    let bad = root.join("bad-msg");
    std::fs::write(&good, "# template\nfix: typo\n").unwrap();
    std::fs::write(&bad, "# template\nFix: Typo\n").unwrap();

    for (file, expected) in [(&good, Some(0)), (&bad, Some(1))] {
        let shell = run_shell(
            &root,
            "check-commitlint.sh",
            &["--message", file.to_str().unwrap()],
        );
        let native = run_native(
            &root,
            &["helpers", "commitlint", "--message", file.to_str().unwrap()],
        );
        assert_eq!(shell.0, native.0, "{file:?}");
        assert_eq!(shell.1, native.1, "{file:?}");
        assert_eq!(shell.2, native.2, "{file:?}");
        assert_eq!(native.0, expected, "{file:?}");
    }
}

/// `--range` mode fails closed identically for an invalid range.
#[test]
fn native_and_shell_commitlint_invalid_range_fails_closed() {
    let (_dir, root) = fixture_with_scripts();
    git(&root, &["init", "-q"]);
    commit(&root, "fix: typo", "2024-01-01T00:00:00");

    let shell = run_shell(
        &root,
        "check-commitlint.sh",
        &["--range", "does-not-exist...HEAD"],
    );
    let native = run_native(
        &root,
        &["helpers", "commitlint", "--range", "does-not-exist...HEAD"],
    );

    assert_eq!(shell.0, native.0);
    assert_eq!(shell.0, Some(2));
    assert!(native.2.contains("git log failed"));
}

/// The breaking-change marker `!` is accepted by both.
#[test]
fn native_and_shell_commitlint_accept_breaking_change_marker() {
    let (_dir, root) = fixture_with_scripts();
    git(&root, &["init", "-q"]);
    commit(
        &root,
        "fix(distribution)!: drop legacy paths",
        "2024-01-01T00:00:00",
    );

    let shell = run_shell(&root, "check-commitlint.sh", &[]);
    let native = run_native(&root, &["helpers", "commitlint"]);

    assert_eq!(shell.0, native.0);
    assert_eq!(shell.0, Some(0));
}

/// The dependency-direction rule fails identically in both implementations.
#[test]
fn native_and_shell_deps_reject_types_dependency_on_storage() {
    let (_dir, root) = fixture_with_scripts();
    std::fs::create_dir_all(root.join("crates/types")).unwrap();
    std::fs::write(
        root.join("crates/types/Cargo.toml"),
        "[dependencies]\ndo-harness-db = \"0.1\"\n",
    )
    .unwrap();

    let shell = run_shell(&root, "check-deps.sh", &[]);
    let native = run_native(&root, &["helpers", "deps"]);

    assert_eq!(shell.0, native.0);
    assert_eq!(shell.0, Some(1));
    assert!(
        shell
            .1
            .contains("FAIL: do-harness-types must not depend on storage or adapters.")
    );
    assert_eq!(shell.1, native.1);
}

/// `do-harness helpers --help` lists all three native helpers.
#[test]
fn helpers_help_lists_native_subcommands() {
    let output = isolate(&mut Command::new(harness()))
        .args(["helpers", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    for name in ["commitlint", "deps", "audit"] {
        assert!(text.contains(name), "help must list {name}:\n{text}");
    }
}
