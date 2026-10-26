//! Canonical invalidation and task completion use the same provenance.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::path::Path;
use std::process::{Command, Output};

mod support;

fn git(root: &Path, args: &[&str]) {
    assert!(
        support::git_command(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap()
            .status
            .success()
    );
}

fn harness(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("do-harness.toml"),
        "language='generic'\n[signal-sets]\nverification=['probe']\n\n\
         [[sensors]]\nname='probe'\nargv=['true']\ninputs=['input.txt']\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("input.txt"), "one").unwrap();
    std::fs::write(dir.path().join(".gitignore"), ".do-harness/\n").unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "test: base"]);
    dir
}

fn state(root: &Path) -> String {
    let out = harness(
        root,
        &["status", "--set", "verification", "--format", "json"],
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn branch_and_empty_commit_invalidate_otherwise_identical_workspace() {
    let dir = fixture();
    assert!(
        harness(dir.path(), &["verify", "--set", "verification"])
            .status
            .success()
    );
    assert_eq!(state(dir.path()), "green");
    git(dir.path(), &["checkout", "-qb", "another"]);
    assert_eq!(state(dir.path()), "stale");
    assert!(
        harness(dir.path(), &["verify", "--set", "verification"])
            .status
            .success()
    );
    git(
        dir.path(),
        &["commit", "--allow-empty", "-qm", "test: revision"],
    );
    assert_eq!(state(dir.path()), "stale");
}

#[test]
fn missing_provenance_is_readable_but_never_fresh() {
    let dir = fixture();
    assert!(
        harness(dir.path(), &["verify", "--set", "verification"])
            .status
            .success()
    );
    let path = dir.path().join(".do-harness/evidence.verification.json");
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    doc.as_object_mut().unwrap().remove("input_manifest");
    std::fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_ne!(state(dir.path()), "green");
}

#[test]
fn done_rejects_a_passing_beat_after_its_inputs_change() {
    let dir = fixture();
    std::fs::create_dir(dir.path().join("plans")).unwrap();
    std::fs::write(dir.path().join("plans/methods.json"),
        r#"{"methods":[{"name":"one","subtasks":[{"name":"verify","sensor":"probe","spike_candidate":false}],"preconditions":[] }]}"#).unwrap();
    assert!(
        harness(
            dir.path(),
            &["task", "add", "fresh task", "--method", "one"]
        )
        .status
        .success()
    );
    assert!(
        harness(
            dir.path(),
            &["verify", "--only", "probe", "--record", "--task", "1"]
        )
        .status
        .success()
    );
    assert!(
        harness(dir.path(), &["task", "advance", "1"])
            .status
            .success()
    );
    std::fs::write(dir.path().join("input.txt"), "two").unwrap();
    assert!(!harness(dir.path(), &["task", "done", "1"]).status.success());
    assert!(
        harness(
            dir.path(),
            &["verify", "--only", "probe", "--record", "--task", "1"]
        )
        .status
        .success()
    );
    assert!(harness(dir.path(), &["task", "done", "1"]).status.success());
}

#[test]
fn each_canonical_file_class_invalidates_evidence() {
    for path in [
        "do-harness.toml",
        "input.txt",
        "plans/invariants.json",
        "Cargo.lock",
        "rust-toolchain.toml",
        "sensor.py",
        "new-source.rs",
    ] {
        let dir = fixture();
        std::fs::create_dir(dir.path().join("plans")).unwrap();
        for input in ["plans/invariants.json", "Cargo.lock", "rust-toolchain.toml"] {
            std::fs::write(dir.path().join(input), "original").unwrap();
        }
        std::fs::write(dir.path().join("sensor.py"), "print('ok')").unwrap();
        let config = std::fs::read_to_string(dir.path().join("do-harness.toml"))
            .unwrap()
            .replace("argv=['true']", "argv=['python3','sensor.py']");
        std::fs::write(dir.path().join("do-harness.toml"), config).unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "test: inputs"]);
        assert!(
            harness(dir.path(), &["verify", "--set", "verification"])
                .status
                .success()
        );
        assert_eq!(state(dir.path()), "green", "{path}");
        if path == "do-harness.toml" {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join(path))
                .unwrap()
                .write_all(b"\n# changed policy\n")
                .unwrap();
        } else {
            std::fs::write(dir.path().join(path), "changed").unwrap();
        }
        assert_eq!(state(dir.path()), "stale", "{path}");
    }
}

#[test]
fn declared_environment_changes_invalidate_without_recording_the_value() {
    let dir = fixture();
    let config = std::fs::read_to_string(dir.path().join("do-harness.toml")).unwrap()
        + "environment-inputs=['CANONICAL_PRIVATE']\n";
    std::fs::write(dir.path().join("do-harness.toml"), config).unwrap();
    let secret = "synthetic-private-value";
    let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(dir.path())
        .args(["verify", "--set", "verification"])
        .env("CANONICAL_PRIVATE", secret)
        .output()
        .unwrap();
    assert!(out.status.success());
    let bytes =
        std::fs::read_to_string(dir.path().join(".do-harness/evidence.verification.json")).unwrap();
    assert!(!bytes.contains(secret));
    assert_eq!(state(dir.path()), "stale");
}

#[test]
fn old_evidence_remains_readable_but_requires_regeneration() {
    const HISTORICAL_SCHEMAS: &[u64] = &[3, 4];
    let dir = fixture();
    for version in HISTORICAL_SCHEMAS {
        assert!(
            harness(dir.path(), &["verify", "--set", "verification"])
                .status
                .success()
        );
        let path = dir.path().join(".do-harness/evidence.verification.json");
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        doc["schema_version"] = (*version).into();
        doc.as_object_mut().unwrap().remove("input_manifest");
        std::fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(state(dir.path()), "stale");
    }
}

#[test]
fn concurrent_evidence_writers_publish_whole_chained_documents() {
    const WRITERS: usize = 4;
    let dir = fixture();
    let mut children = Vec::new();
    for _ in 0..WRITERS {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_do-harness"))
                .arg("--root")
                .arg(dir.path())
                .args(["verify", "--set", "verification"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    assert_eq!(state(dir.path()), "green");
    let doc: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.path().join(".do-harness/evidence.verification.json")).unwrap(),
    )
    .unwrap();
    assert!(doc["prev_hash"].as_str().is_some());
}
