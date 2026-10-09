//! Acceptance tests for evidence invalidation when inputs change during run or post-execution.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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

/// Creates a committed git repo with the given sensors and signal sets.
fn fixture_repo_with_sensors(
    sensors: &[(&str, &[&str])],
    sets: &[(&str, &[&str])],
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut config = String::from("[signal-sets]\n");
    for (set, names) in sets {
        use std::fmt::Write as _;
        let list = names
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        write!(config, "{set} = [{list}]").unwrap();
        config.push('\n');
    }
    for (name, argv) in sensors {
        use std::fmt::Write as _;
        let args = argv
            .iter()
            .map(|a| format!("\"{}\"", a.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            config,
            "\n[[sensors]]\nname = \"{name}\"\nargv = [{args}]\n"
        )
        .unwrap();
    }
    std::fs::write(root.join("do-harness.toml"), &config).unwrap();
    std::fs::write(root.join("lib.rs"), "v1\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);
    (dir, root)
}

/// Runs `status --set <set> --format json`, returning (exit code, document).
fn status(root: &Path, set: &str) -> (Option<i32>, Value) {
    let (code, stdout, stderr) = run(harness(root)
        .arg("status")
        .arg("--set")
        .arg(set)
        .arg("--format")
        .arg("json"));
    (
        code,
        serde_json::from_str(&stdout)
            .unwrap_or_else(|_| panic!("status stdout is not JSON:\n{stdout}\n{stderr}")),
    )
}

/// Test 1 & 2: Sensor A passes, then Sensor B (or external edit) changes A's source input.
/// Verifies both with --record and without --record that verify --strict and status cannot report green.
#[test]
fn sensor_b_mutating_source_invalidates_run_with_and_without_record() {
    for record_flag in &[false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let config = r#"
[signal-sets]
verification = ["a", "b"]

[[sensors]]
name = "a"
argv = ["true"]
inputs = ["lib.rs"]

[[sensors]]
name = "b"
argv = ["sh", "-c", "echo mutated > lib.rs"]
"#;
        std::fs::write(root.join("do-harness.toml"), config).unwrap();
        std::fs::write(root.join("lib.rs"), "v1\n").unwrap();
        git(&root, &["init", "-q"]);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-qm", "test: base"]);

        let mut cmd = harness(&root);
        cmd.arg("verify")
            .arg("--set")
            .arg("verification")
            .arg("--strict");
        if *record_flag {
            cmd.arg("--record");
        }
        let (code, stdout, stderr) = run(&mut cmd);
        assert_ne!(
            code,
            Some(0),
            "verify --strict must fail when inputs changed during run (record={record_flag}): stdout:\n{stdout}\nstderr:\n{stderr}"
        );

        let (status_code, doc) = status(&root, "verification");
        assert_eq!(
            status_code,
            Some(1),
            "status must fail when evidence was invalidated: {doc}"
        );
        assert_eq!(doc["state"], "red");
        assert_eq!(doc["reason"], "inputs_changed_during_run");
    }
}

/// Test 3: Barrier-coordinated external writer changes source during a sensor run.
/// Evidence is invalidated, verify --strict fails, and status reports red with reason "`inputs_changed_during_run`".
#[test]
fn barrier_coordinated_external_writer_invalidates_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let barrier_start = root.join("barrier_start");
    let barrier_release = root.join("barrier_release");

    let helper_script = root.join("run_sensor.sh");
    std::fs::write(
        &helper_script,
        format!(
            "#!/bin/sh\ntouch \"{}\"\nwhile [ ! -f \"{}\" ]; do sleep 0.01; done\n",
            barrier_start.display(),
            barrier_release.display()
        ),
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&helper_script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&helper_script, perms).unwrap();
    }

    let config = r#"
[signal-sets]
verification = ["slow_sensor"]

[[sensors]]
name = "slow_sensor"
argv = ["sh", "./run_sensor.sh"]
inputs = ["lib.rs"]
"#;

    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::write(root.join("lib.rs"), "v1\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);

    let root_clone = root.clone();
    let handle = std::thread::spawn(move || {
        let mut cmd = harness(&root_clone);
        cmd.arg("verify")
            .arg("--set")
            .arg("verification")
            .arg("--strict");
        run(&mut cmd)
    });

    // Wait for the sensor to reach the barrier with a 10s safety timeout
    let start = std::time::Instant::now();
    while !barrier_start.exists() {
        assert!(
            start.elapsed() <= std::time::Duration::from_secs(10),
            "timed out waiting for barrier_start to be created by sensor"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // External writer modifies source input during sensor run
    std::fs::write(root.join("lib.rs"), "v2\n").unwrap();

    // Release the barrier
    std::fs::write(&barrier_release, "go\n").unwrap();

    let (code, stdout, stderr) = handle.join().expect("join thread");
    assert_ne!(
        code,
        Some(0),
        "verify --strict must fail due to mid-run input mutation:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status_code, doc) = status(&root, "verification");
    assert_eq!(
        status_code,
        Some(1),
        "status must report failure for invalidated evidence: {doc}"
    );
    assert_eq!(doc["state"], "red");
    assert_eq!(doc["reason"], "inputs_changed_during_run");
}

/// Test 4: Sensor selection recorded in evidence equals selection actually executed,
/// even if workspace paths change during the run.
#[test]
fn selection_parity_preserved_when_paths_change_during_run() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    // Sensor 'a' modifies applicability paths (creates new.rs)
    let config = r#"
[signal-sets]
verification = ["a", "b"]

[[sensors]]
name = "a"
argv = ["sh", "-c", "echo '// new' > new.rs"]
artifacts = ["new.rs"]
when-changed = ["*.rs"]

[[sensors]]
name = "b"
argv = ["true"]
inputs = ["lib.rs"]
when-changed = ["lib.rs"]
"#;

    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::write(root.join("lib.rs"), "v1\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);

    // Modify lib.rs so --changed selects sensors 'a' and 'b' initially
    std::fs::write(root.join("lib.rs"), "v2\n").unwrap();

    let mut cmd = harness(&root);
    cmd.arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--changed");
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(
        code,
        Some(0),
        "verify --changed should complete: stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let evidence_path = root.join(".do-harness/evidence.verification.json");
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(&evidence_path).unwrap()).expect("json");

    let sensor_names: Vec<&str> = doc["sensors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();

    // Selection resolved before run included 'a' and 'b' (both matched lib.rs).
    // Selection recorded in evidence must equal that pre-resolved selection.
    assert_eq!(sensor_names, vec!["a", "b"]);
}

/// Test 5: Changes only to declared generated outputs (`artifacts`) or harness-owned state (`.do-harness/`)
/// do not cause spurious invalidation.
#[test]
fn declared_outputs_and_harness_state_do_not_cause_spurious_invalidation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let config = r#"
[signal-sets]
verification = ["generator"]

[[sensors]]
name = "generator"
argv = ["sh", "-c", "mkdir -p out && echo generated > out/artifact.txt"]
artifacts = ["out/*.txt"]
"#;

    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);

    let mut cmd = harness(&root);
    cmd.arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--strict");
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(
        code,
        Some(0),
        "verify --strict must succeed when only declared untracked artifacts are created: stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status_code, doc) = status(&root, "verification");
    assert_eq!(status_code, Some(0), "status must be green: {doc}");
    assert_eq!(doc["state"], "green");
}

/// Test 6: Unchanged workspaces retain normal green behavior.
#[test]
fn unchanged_workspace_retains_green_behavior() {
    let (dir, root) = fixture_repo_with_sensors(
        &[("pass_sensor", &["true"])],
        &[("verification", &["pass_sensor"])],
    );

    let mut cmd = harness(&root);
    cmd.arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--strict");
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(
        code,
        Some(0),
        "verify --strict must succeed on unchanged workspace: stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status_code, doc) = status(&root, "verification");
    assert_eq!(status_code, Some(0));
    assert_eq!(doc["state"], "green");

    drop(dir);
}

/// Test 7: Recorded-beat reuse continues rejecting changed input identities.
#[test]
fn recorded_beat_reuse_rejects_changed_input_identities() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let config = r#"
[signal-sets]
verification = ["p"]

[[sensors]]
name = "p"
argv = ["sh", "-c", "printf hit >> runs.log"]
inputs = ["input.txt"]
"#;

    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::write(root.join("input.txt"), "v1\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);

    // Record initial beat
    let mut cmd = harness(&root);
    cmd.arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--record");
    let (code, _, _) = run(&mut cmd);
    assert_eq!(code, Some(0));

    // Input changes
    std::fs::write(root.join("input.txt"), "v2\n").unwrap();

    // Verify with --unchanged=skip: must re-run instead of reusing
    let mut cmd = harness(&root);
    cmd.arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--record")
        .arg("--unchanged=skip")
        .arg("--format")
        .arg("json");
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(code, Some(0));

    let report: Value = serde_json::from_str(&stdout).unwrap();
    let execution = report["sensors"][0]["execution"].as_str().unwrap();
    assert_eq!(
        execution, "ran",
        "changed input identity must force a fresh run rather than reuse beat: {stderr}"
    );
}
