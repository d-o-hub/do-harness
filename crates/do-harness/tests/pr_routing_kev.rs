//! Hermetic integration tests for Kev router adapter and safety invariants.
//!
//! Tests:
//! 1. Schema compliance and adapter invocation via `semantic-route.sh`.
//! 2. Context truncation fallback to `deep` review.
//! 3. Anti-downgrade safeguards for security, public API, storage, and concurrency diffs.
//! 4. Zero-model path (no-effect) bypassing model calls.
//! 5. Provider error and invalid JSON fallback to `deep` route.
//! 6. Cache invalidation on head SHA / commit changes.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

mod support;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

fn adapter_script() -> PathBuf {
    repo_root().join("integrations/pr-triage/kev/kev_router_adapter.py")
}

fn wrapper_script() -> PathBuf {
    repo_root().join(".agents/skills/pr-triage/scripts/semantic-route.sh")
}

fn run_adapter_with_input(input_json: &Value) -> Value {
    let mut child = Command::new("python3")
        .arg(adapter_script())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn adapter");

    let stdin_str = serde_json::to_string(input_json).unwrap();
    {
        let mut stdin = child.stdin.take().expect("failed to open stdin");
        stdin.write_all(stdin_str.as_bytes()).unwrap();
    }

    let output = child.wait_with_output().expect("wait for adapter");
    assert!(output.status.success(), "adapter failed: {output:?}");

    serde_json::from_slice(&output.stdout).expect("valid JSON stdout")
}

#[test]
fn adapter_direct_execution_returns_valid_schema() {
    let input = serde_json::json!({
        "schema_version": 1,
        "pr": 42,
        "head_sha": "head123",
        "merge_base": "base123",
        "source": "raw",
        "content": "+// Documentation update\n"
    });

    let json = run_adapter_with_input(&input);
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["change_kind"], "docs");
    assert_eq!(json["confidence"], 0.95);
    assert_eq!(json["behavior_change"]["answer"], "no");
    assert_eq!(json["public_contract_change"]["answer"], "no");
    assert_eq!(json["security_sensitive"]["answer"], "no");
}

#[test]
fn adapter_truncation_forces_unknown_answers_and_zero_confidence() {
    // Oversized content exceeding 16KB default context
    let large_content = "+ fn big() { ".to_string() + &"a".repeat(20000) + " }\n";

    let input = serde_json::json!({
        "schema_version": 1,
        "pr": 42,
        "head_sha": "head123",
        "merge_base": "base123",
        "source": "raw",
        "content": large_content
    });

    let json = run_adapter_with_input(&input);
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["change_kind"], "unknown");
    assert_eq!(json["confidence"], 0.0);
    assert_eq!(json["behavior_change"]["answer"], "unknown");
    assert_eq!(json["metadata"]["truncated"], true);
}

#[test]
fn security_and_public_api_diffs_are_never_downgraded() {
    let cases = [
        (
            "+# doc comment touching auth_token verification\n",
            "security",
        ),
        ("+pub fn exported_api_contract() {}\n", "public-api"),
        ("+PRAGMA journal_mode = WAL;\n", "mixed"),
        ("+let lock = mutex.lock();\n", "mixed"),
    ];

    for (diff, expected_kind) in cases {
        let input = serde_json::json!({
            "schema_version": 1,
            "pr": 101,
            "head_sha": "head_risk",
            "merge_base": "base_risk",
            "source": "raw",
            "content": diff
        });

        let json = run_adapter_with_input(&input);
        assert_eq!(
            json["change_kind"], expected_kind,
            "diff with sensitive pattern must classify as {expected_kind}: {diff}"
        );
        assert_eq!(
            json["confidence"], 0.95,
            "high risk pattern must have high confidence escalation"
        );
    }
}

#[test]
fn zero_model_no_effect_path_does_not_call_adapter() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    let git = |args: &[&str]| {
        let res = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(res.status.success());
    };

    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("file.txt"), "hello").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    git(&["checkout", "-q", "-b", "feature"]);

    // Call semantic-route.sh with no diff
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(wrapper_script());
    cmd.arg("--base").arg("main").arg("--head").arg("feature");
    cmd.current_dir(repo);
    cmd.env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"));
    cmd.env("PR_TRIAGE_ROUTER", adapter_script());
    support::clear_git_view(&mut cmd);

    let output = cmd.output().expect("spawn semantic-route.sh");
    assert!(output.status.success());

    let json: Value = serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(json["status"], "no-effect");
    assert_eq!(json["reason"], "no_effective_change");
    assert_eq!(json["route"], "deep");
}

#[test]
fn provider_failure_or_invalid_json_falls_back_to_deep() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    let git = |args: &[&str]| {
        let res = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(res.status.success());
    };

    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("file.txt"), "base content\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);

    git(&["checkout", "-q", "-b", "feature"]);
    std::fs::write(repo.join("file.txt"), "base content\n+ new code\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "feature change"]);

    // Create a broken router binary that prints invalid JSON and exits
    let bad_router = temp.path().join("bad_router.sh");
    std::fs::write(&bad_router, "#!/bin/sh\necho 'corrupted output'\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bad_router, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut cmd = Command::new("/bin/bash");
    cmd.arg(wrapper_script());
    cmd.arg("--base").arg("main").arg("--head").arg("feature");
    cmd.current_dir(repo);
    cmd.env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"));
    cmd.env("PR_TRIAGE_ROUTER", &bad_router);
    support::clear_git_view(&mut cmd);

    let output = cmd.output().expect("spawn semantic-route.sh");
    assert!(output.status.success());

    let json: Value = serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(json["status"], "fallback");
    assert_eq!(json["route"], "deep");
    assert_eq!(json["reason"], "malformed_json_output");
}

#[test]
fn head_sha_change_invalidates_cached_route() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    let git = |args: &[&str]| -> String {
        let res = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(res.status.success());
        String::from_utf8_lossy(&res.stdout).trim().to_string()
    };

    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "# Base\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    let main_sha = git(&["rev-parse", "main"]);

    git(&["checkout", "-q", "-b", "feature"]);
    std::fs::write(repo.join("README.md"), "# Base\n+# Doc update 1\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "doc commit 1"]);
    let head_sha1 = git(&["rev-parse", "feature"]);

    let run_route = |head_ref: &str| {
        let mut cmd = Command::new("/bin/bash");
        cmd.arg(wrapper_script());
        cmd.arg("--base").arg(&main_sha).arg("--head").arg(head_ref);
        cmd.current_dir(repo);
        cmd.env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"));
        cmd.env("PR_TRIAGE_ROUTER", adapter_script());
        support::clear_git_view(&mut cmd);

        let output = cmd.output().expect("spawn semantic-route.sh");
        assert!(output.status.success());
        serde_json::from_str::<Value>(&String::from_utf8_lossy(&output.stdout)).unwrap()
    };

    let res1 = run_route(&head_sha1);
    assert_eq!(res1["status"], "ok");
    assert_eq!(res1["route"], "cheap");

    // Push new commit to feature branch
    std::fs::write(repo.join("lib.rs"), "pub fn new_api() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "code commit 2"]);
    let head_sha2 = git(&["rev-parse", "feature"]);

    let res2 = run_route(&head_sha2);
    assert_eq!(res2["status"], "ok");
    assert_eq!(
        res2["route"], "deep",
        "new commit with exported public contract must re-evaluate to deep route"
    );
}
