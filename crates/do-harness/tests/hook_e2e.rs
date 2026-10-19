//! End-to-end hook installation: the managed `commit-msg` hook blocks a bad
//! subject through a real `git commit` and lets a good one through.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

mod support;

use support::git_command;

fn harness() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    support::isolate_command(&mut cmd);
    cmd
}

fn run(cmd: &mut Command) -> Output {
    let out = cmd.output().expect("spawn");
    assert!(
        out.status.success(),
        "command failed: {:?}\nstdout: {}\nstderr: {}",
        cmd,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn git(dir: &Path, args: &[&str]) -> Output {
    git_command(dir).args(args).output().expect("spawn git")
}

fn git_ok(dir: &Path, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Runs git with the harness binary pinned, so the managed hooks execute the
/// binary under test instead of building a release one.
fn git_ok_with_bin(dir: &Path, args: &[&str], bin: &str) {
    let out = git_command(dir)
        .args(args)
        .env("DO_HARNESS_BIN", bin)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn push_with_bin(dir: &Path, bin: &str) -> Output {
    git_command(dir)
        .args(["push", "origin", "HEAD:refs/heads/main"])
        .env("DO_HARNESS_BIN", bin)
        .output()
        .expect("spawn git push")
}

#[test]
fn installed_commit_msg_hook_blocks_bad_subject() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    // `init` scaffolds a valid Rust crate at the root, so the pre-commit
    // `cargo fmt` sensor has something to format.
    run(harness().arg("--root").arg(root).arg("init"));

    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "core.hooksPath", ".git/hooks"]);
    git_ok(root, &["config", "user.email", "e2e@example.test"]);
    git_ok(root, &["config", "user.name", "e2e"]);
    run(harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("hook")
        .arg("install"));

    std::fs::write(root.join("note.txt"), "hello\n").expect("write note");
    git_ok(root, &["add", "-A"]);

    let bad = git_command(root)
        .args(["commit", "-m", "Bad Subject"])
        .env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"))
        .output()
        .expect("spawn git commit");
    assert!(
        !bad.status.success(),
        "uppercase subject must be blocked by the commit-msg hook"
    );

    let good = git_command(root)
        .args(["commit", "-m", "feat: e2e hook check"])
        .env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"))
        .output()
        .expect("spawn git commit");
    assert!(
        good.status.success(),
        "conventional subject must commit: {}",
        String::from_utf8_lossy(&good.stderr)
    );
}

#[test]
fn hook_status_and_doctor_report_shadowed_hooks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    run(harness().arg("--root").arg(root).arg("init"));

    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "core.hooksPath", ".git/hooks"]);
    git_ok(root, &["config", "user.email", "e2e@example.test"]);
    git_ok(root, &["config", "user.name", "e2e"]);

    run(harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("hook")
        .arg("install"));

    git_ok(root, &["config", "core.hooksPath", ".githooks"]);

    let status_out = harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("hook")
        .arg("status")
        .output()
        .expect("spawn status");
    let status_stdout = String::from_utf8_lossy(&status_out.stdout);
    assert!(
        status_stdout.contains("shadowed by core.hooksPath=.githooks"),
        "status stdout: {status_stdout}"
    );

    let doctor_out = harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("doctor")
        .output()
        .expect("spawn doctor");
    assert!(!doctor_out.status.success());
    let doctor_stderr = String::from_utf8_lossy(&doctor_out.stderr);
    let doctor_stdout = String::from_utf8_lossy(&doctor_out.stdout);
    assert!(
        doctor_stderr.contains("shadowed by core.hooksPath=.githooks")
            || doctor_stdout.contains("shadowed by core.hooksPath=.githooks"),
        "doctor output: stdout={doctor_stdout}, stderr={doctor_stderr}"
    );

    std::fs::create_dir_all(root.join(".githooks")).expect("mkdir .githooks");
    run(harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("hook")
        .arg("install"));

    let status_out2 = harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("hook")
        .arg("status")
        .output()
        .expect("spawn status");
    let status_stdout2 = String::from_utf8_lossy(&status_out2.stdout);
    assert!(
        status_stdout2.contains("pre-commit: installed"),
        "status stdout: {status_stdout2}"
    );
    assert!(
        !status_stdout2.contains("shadowed"),
        "status stdout should not contain shadowed: {status_stdout2}"
    );
}

/// The managed `pre-push` hook runs the real sensor suite: a workspace that
/// verifies green pushes, and one whose `check` sensor fails is stopped.
///
/// `hooks/tests.rs` asserts the installed script by string match; this is the
/// only place the script actually runs under a `git push`, so a hook that
/// silently stopped gating (or one that blocked everything) would show up here.
#[test]
fn installed_pre_push_hook_blocks_a_failing_workspace() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let remote = tempfile::tempdir().expect("tempdir");
    let bin = env!("CARGO_BIN_EXE_do-harness");

    run(harness().arg("--root").arg(root).arg("init"));
    git_ok(root, &["init", "-q"]);
    git_ok(remote.path(), &["init", "--bare", "-q"]);
    git_ok(root, &["config", "core.hooksPath", ".git/hooks"]);
    git_ok(root, &["config", "user.email", "e2e@example.test"]);
    git_ok(root, &["config", "user.name", "e2e"]);
    // A local path remote keeps the push hermetic; normalizing separators keeps
    // the same URL valid on Windows.
    let remote_url = remote.path().to_string_lossy().replace('\\', "/");
    git_ok(root, &["remote", "add", "origin", &remote_url]);
    run(harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("hook")
        .arg("install"));

    git_ok(root, &["add", "-A"]);
    git_ok_with_bin(
        root,
        &["commit", "-q", "-m", "chore: seed the hook e2e"],
        bin,
    );

    let pushed = push_with_bin(root, bin);
    assert!(
        pushed.status.success(),
        "a green workspace must push through the pre-push hook:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&pushed.stdout),
        String::from_utf8_lossy(&pushed.stderr)
    );

    // Break the crate: `check` fails, so the hook must stop the push and name
    // the sensor that failed.
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn broken() -> u32 {\n    \"not a number\"\n}\n",
    )
    .expect("write broken lib");
    git_ok(root, &["add", "-A"]);
    git_ok_with_bin(root, &["commit", "-q", "-m", "fix: break the crate"], bin);

    let blocked = push_with_bin(root, bin);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&blocked.stdout),
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(
        !blocked.status.success(),
        "a workspace that fails verification must not push:\n{combined}"
    );
    assert!(
        combined.contains("check"),
        "the blocked push must name the failing sensor:\n{combined}"
    );
}

#[test]
fn linked_worktree_e2e_hook_execution_and_lifecycle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let parent = dir.path().join("parent");
    let linked = dir.path().join("linked");
    let bin = env!("CARGO_BIN_EXE_do-harness");

    std::fs::create_dir_all(&parent).expect("mkdir parent");
    run(harness().arg("--root").arg(&parent).arg("init"));

    git_ok(&parent, &["init", "-q"]);
    git_ok(&parent, &["config", "user.email", "e2e@example.test"]);
    git_ok(&parent, &["config", "user.name", "e2e"]);

    git_ok(&parent, &["add", "-A"]);
    git_ok_with_bin(
        &parent,
        &["commit", "-q", "-m", "chore: initial commit"],
        bin,
    );

    let linked_str = linked.to_string_lossy().replace('\\', "/");
    git_ok(
        &parent,
        &["worktree", "add", "-b", "fixture", &linked_str],
    );

    // Install hooks from linked worktree side
    let install_out = harness()
        .current_dir(&linked)
        .arg("--root")
        .arg(&linked)
        .arg("hook")
        .arg("install")
        .output()
        .expect("spawn hook install in linked worktree");
    assert!(
        install_out.status.success(),
        "hook install from linked worktree must succeed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&install_out.stdout),
        String::from_utf8_lossy(&install_out.stderr)
    );

    // Verify hook status from both parent and linked sides
    for side in [&parent, &linked] {
        let status_out = harness()
            .current_dir(side)
            .arg("--root")
            .arg(side)
            .arg("hook")
            .arg("status")
            .env("DO_HARNESS_BIN", bin)
            .output()
            .expect("spawn hook status");
        assert!(
            status_out.status.success(),
            "hook status must succeed from {}:\n{}",
            side.display(),
            String::from_utf8_lossy(&status_out.stderr)
        );
        let stdout = String::from_utf8_lossy(&status_out.stdout);
        assert!(
            stdout.contains("pre-commit: installed"),
            "status stdout: {stdout}"
        );
        assert!(!stdout.contains("shadowed"), "status stdout: {stdout}");

        let diff_out = harness()
            .current_dir(side)
            .arg("--root")
            .arg(side)
            .arg("hook")
            .arg("diff")
            .env("DO_HARNESS_BIN", bin)
            .output()
            .expect("spawn hook diff");
        assert!(
            diff_out.status.success(),
            "hook diff must succeed from {}:\n{}",
            side.display(),
            String::from_utf8_lossy(&diff_out.stderr)
        );
    }

    // A commit from the linked worktree with bad commit message subject must be blocked
    std::fs::write(linked.join("note.txt"), "hello worktree\n").expect("write note");
    git_ok(&linked, &["add", "-A"]);

    let bad_commit = git_command(&linked)
        .args(["commit", "-m", "Bad Subject Message"])
        .env("DO_HARNESS_BIN", bin)
        .output()
        .expect("spawn git commit in linked worktree");
    assert!(
        !bad_commit.status.success(),
        "bad commit subject must be blocked in linked worktree:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&bad_commit.stdout),
        String::from_utf8_lossy(&bad_commit.stderr)
    );
    let bad_stderr = String::from_utf8_lossy(&bad_commit.stderr);
    assert!(
        bad_stderr.contains("non-conventional commit subject")
            || bad_stderr.contains("Conventional-commit invariant violated"),
        "stderr must report commitlint failure:\n{bad_stderr}"
    );

    // Break code formatting in linked worktree so pre-commit fmt sensor fails
    std::fs::write(
        linked.join("src/lib.rs"),
        "pub fn  unformatted ( )   {   1+1;  }\n",
    )
    .expect("write unformatted lib.rs");
    git_ok(&linked, &["add", "-A"]);

    let failing_pre_commit = git_command(&linked)
        .args(["commit", "-m", "feat: valid conventional subject"])
        .env("DO_HARNESS_BIN", bin)
        .output()
        .expect("spawn git commit in linked worktree");
    assert!(
        !failing_pre_commit.status.success(),
        "a commit with broken code must be blocked by pre-commit hook in linked worktree"
    );
    let failing_combined = format!(
        "{}{}",
        String::from_utf8_lossy(&failing_pre_commit.stdout),
        String::from_utf8_lossy(&failing_pre_commit.stderr)
    );
    assert!(
        failing_combined.contains("fmt") || failing_combined.contains("Failed sensors"),
        "failing pre-commit hook output must include verification stdout/stderr:\n{failing_combined}"
    );

    // Add a foreign hook in the shared parent hooks directory
    let parent_hooks = parent.join(".git/hooks");
    std::fs::write(
        parent_hooks.join("pre-push"),
        "#!/bin/sh\necho 'foreign pre-push'\n",
    )
    .expect("write foreign hook");

    // Uninstall from linked worktree side
    let uninstall_out = harness()
        .current_dir(&linked)
        .arg("--root")
        .arg(&linked)
        .arg("hook")
        .arg("uninstall")
        .output()
        .expect("spawn hook uninstall in linked worktree");
    assert!(
        uninstall_out.status.success(),
        "uninstall from linked worktree must succeed:\n{}",
        String::from_utf8_lossy(&uninstall_out.stderr)
    );

    assert!(
        !parent_hooks.join("pre-commit").exists(),
        "managed pre-commit hook should be removed"
    );
    assert!(
        !parent_hooks.join("commit-msg").exists(),
        "managed commit-msg hook should be removed"
    );
    assert!(
        parent_hooks.join("pre-push").exists(),
        "foreign pre-push hook must be preserved"
    );
}
