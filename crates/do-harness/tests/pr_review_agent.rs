//! Integration tests for `do-harness pr review --view agent --format json` (WF-03).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

#[cfg(unix)]
const EXEC_PERMS: u32 = 0o755;
#[cfg(unix)]
const READ_PERMS: u32 = 0o644;

mod support;

use support::harness;

fn git(dir: &Path, args: &[&str]) {
    let output = support::git_command(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
}

fn commit_file(dir: &Path, name: &str, content: &str, message: &str) {
    if let Some(parent) = Path::new(name).parent() {
        let full = dir.join(parent);
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(full).unwrap();
        }
    }
    std::fs::write(dir.join(name), content).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn review_agent(root: &Path, extra: &[&str]) -> Value {
    let mut cmd = harness(root);
    cmd.args([
        "pr", "review", "--base", "main", "--head", "feature", "--view", "agent", "--format",
        "json",
    ]);
    cmd.args(extra);
    let output = cmd.output().expect("spawn do-harness");
    assert!(
        output.status.success(),
        "review failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse agent json envelope")
}

#[test]
fn test_source_plus_lockfile_exemption() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"Cargo.lock\"]\n",
        "add gate policy",
    );
    commit_file(dir.path(), "Cargo.lock", "version = 1\n", "lockfile");
    commit_file(dir.path(), "src/lib.rs", "pub fn a() {}\n", "source");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "Cargo.lock", "version = 2\n", "bump lockfile");
    commit_file(
        dir.path(),
        "src/lib.rs",
        "pub fn a() {}\npub fn b() {}\n",
        "add b",
    );

    let envelope = review_agent(dir.path(), &[]);
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["exempt_count"], 1);
    assert_eq!(envelope["skipped_count"], 0);

    let exempt = envelope["exempt_units"].as_array().unwrap();
    assert_eq!(exempt.len(), 1);
    assert_eq!(exempt[0]["path"], "Cargo.lock");
    assert!(
        exempt[0].get("lines").is_none(),
        "agent view must omit payload lines from exempt units"
    );

    let req = envelope["review_required_units"].as_array().unwrap();
    assert_eq!(req.len(), 1);
    assert!(req[0].as_str().unwrap().starts_with("src/lib.rs"));

    let bytes = &envelope["measurement"];
    assert!(bytes["emitted_bytes"].as_u64().unwrap() > 0);
    assert!(bytes["audit_bytes"].as_u64().unwrap() > 0);
    assert!(
        bytes["emitted_bytes"].as_u64().unwrap() < bytes["audit_bytes"].as_u64().unwrap(),
        "emitted bytes should be smaller than audit bytes"
    );
}

#[test]
fn test_malicious_policy_in_pr_head() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"Cargo.lock\"]\n",
        "initial policy",
    );
    commit_file(dir.path(), "src/secret.rs", "secret_v1\n", "secret");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    // Malicious PR head attempts to exempt its own secret changes
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"Cargo.lock\", \"src/secret.rs\"]\n",
        "malicious policy",
    );
    commit_file(dir.path(), "src/secret.rs", "secret_v2\n", "tamper secret");

    let envelope = review_agent(dir.path(), &[]);
    let req = envelope["review_required_units"].as_array().unwrap();
    // Both the secret and the policy modification must remain review-required
    assert!(
        req.iter()
            .any(|u| u.as_str().unwrap().starts_with("src/secret.rs")),
        "secret must not be exempted by policy in PR head"
    );
    assert!(
        req.iter()
            .any(|u| u.as_str().unwrap().starts_with(".github/pr-gate.toml")),
        "policy change must remain review-required"
    );
}

#[test]
fn test_malformed_policy() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof\nmechanical = unclosed\n",
        "malformed policy",
    );
    commit_file(dir.path(), "src/foo.rs", "val1\n", "source");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "src/foo.rs", "val2\n", "update source");

    let envelope = review_agent(dir.path(), &[]);
    let warnings = envelope["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("gate policy")
                || w.as_str().unwrap().contains("pr-gate.toml")),
        "warnings should report policy problem: {warnings:?}"
    );

    let req = envelope["review_required_units"].as_array().unwrap();
    assert!(
        req.iter()
            .any(|u| u.as_str().unwrap().starts_with("src/foo.rs"))
    );
}

#[test]
fn test_planted_public_contract_defect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        "src/api.rs",
        "pub fn greet() -> &'static str { \"hi\" }\n",
        "v1",
    );

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(
        dir.path(),
        "src/api.rs",
        "pub fn greet() -> u32 { 42 }\n",
        "breaking contract defect",
    );

    let envelope = review_agent(dir.path(), &[]);
    let req = envelope["review_required_units"].as_array().unwrap();
    assert_eq!(req.len(), 1);
    assert!(req[0].as_str().unwrap().starts_with("src/api.rs"));
}

#[test]
fn test_rename_mode_and_binary_changes() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "run.sh", "#!/bin/sh\necho hi\n", "script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(dir.path().join("run.sh"))
            .unwrap()
            .permissions();
        p.set_mode(EXEC_PERMS);
        std::fs::set_permissions(dir.path().join("run.sh"), p).unwrap();
        git(dir.path(), &["add", "run.sh"]);
        git(dir.path(), &["commit", "-q", "--amend", "--no-edit"]);
    }
    commit_file(dir.path(), "old.txt", "content\n", "old text");
    std::fs::write(dir.path().join("data.bin"), b"\x00\x01\x02\x03").unwrap();
    git(dir.path(), &["add", "data.bin"]);
    git(dir.path(), &["commit", "-q", "-m", "binary file"]);

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    // 1. Mode change
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(dir.path().join("run.sh"))
            .unwrap()
            .permissions();
        p.set_mode(READ_PERMS);
        std::fs::set_permissions(dir.path().join("run.sh"), p).unwrap();
        git(dir.path(), &["add", "run.sh"]);
    }
    // 2. Pure rename
    git(dir.path(), &["mv", "old.txt", "new.txt"]);
    // 3. Binary change
    std::fs::write(dir.path().join("data.bin"), b"\x00\xFF\xFE\xFD").unwrap();
    git(dir.path(), &["add", "data.bin"]);
    git(dir.path(), &["commit", "-q", "-m", "mode, rename, binary"]);

    let envelope = review_agent(dir.path(), &[]);
    let markers = envelope["markers"].as_array().expect("markers list");
    assert!(
        markers.iter().any(|m| m["kind"] == "rename-only"),
        "markers should include rename-only: {markers:?}"
    );
    assert!(
        markers.iter().any(|m| m["kind"] == "binary"),
        "markers should include binary: {markers:?}"
    );
    #[cfg(unix)]
    assert!(
        markers.iter().any(|m| m["kind"] == "mode-change"),
        "markers should include mode-change: {markers:?}"
    );
}

#[test]
fn test_no_policy() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "foo.txt", "base\n", "init");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "foo.txt", "head\n", "edit");

    let envelope = review_agent(dir.path(), &[]);
    assert_eq!(envelope["policy_digest"], "none");
    assert_eq!(envelope["exempt_count"], 0);
    assert_eq!(envelope["skipped_count"], 0);
    let req = envelope["review_required_units"].as_array().unwrap();
    assert_eq!(req.len(), 1);
    assert!(req[0].as_str().unwrap().starts_with("foo.txt"));
}

#[test]
fn test_tiny_diff_prefers_raw_form() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "README.md", "hello\n", "init");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "README.md", "world\n", "tiny edit");

    let envelope = review_agent(dir.path(), &[]);
    assert_eq!(
        envelope["representation"], "raw_diff",
        "tiny diff should select raw_diff representation"
    );
    assert!(envelope["raw_diff"].is_string());
    assert!(envelope.get("units").is_none());

    let req = envelope["review_required_units"].as_array().unwrap();
    assert_eq!(req.len(), 1);
    assert!(req[0].as_str().unwrap().starts_with("README.md"));
}
