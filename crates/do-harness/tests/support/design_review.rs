//! Shared helpers for the design-review integration tests. Not a test target:
//! imported by `design_review.rs` and `design_review_outcomes.rs` via
//! `#[path]` so every assertion runs against the real CLI binary.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

pub const FIXTURES: &str = "crates/do-harness/tests/fixtures/design-review";

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .expect("repository root")
}

pub fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_do-harness")
}

/// Runs the CLI in the repository root: (exit code, stdout, stderr).
pub fn run_cli<I: IntoIterator<Item = S>, S: AsRef<str>>(args: I) -> (Option<i32>, String, String) {
    let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
    let output = Command::new(binary())
        .args(&args)
        .current_dir(repo_root())
        .output()
        .expect("failed to spawn the do-harness binary");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Reviewer argv pointing the CLI's own bridge at a handoff directory.
pub fn bridge_reviewer(handoff: &Path, max_wait_s: &str) -> String {
    serde_json::to_string(&json!([
        binary(),
        "design-review",
        "bridge",
        "--handoff-dir",
        handoff.to_string_lossy(),
        "--max-wait-s",
        max_wait_s,
    ]))
    .expect("reviewer argv serialization")
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(repo_root().join(FIXTURES).join(name))
        .unwrap_or_else(|_| panic!("missing fixture {name}"))
}

pub fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "do-harness-design-review-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

pub fn record_of(path: &Path) -> Value {
    let raw = std::fs::read_to_string(path).expect("record file");
    serde_json::from_str(&raw).expect("record parses")
}

pub fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Writes a findings response into a bridge handoff directory.
pub fn place_findings(handoff: &Path, body: &str) {
    std::fs::write(handoff.join("findings.json"), body.as_bytes()).expect("place findings");
}

pub fn place_fixture_findings(handoff: &Path, expected_fixture: &str) {
    let expected: Value =
        serde_json::from_str(&fixture(expected_fixture)).expect("expected fixture parses");
    let body = serde_json::to_string(&json!({"findings": expected["findings"]})).expect("response");
    place_findings(handoff, &body);
}

/// The common `design-review run` argv up to the reviewer selection.
pub fn run_args(tag: &str, record_out: &Path) -> Vec<String> {
    vec![
        "design-review".to_owned(),
        "run".to_owned(),
        "--design-doc".to_owned(),
        format!("{FIXTURES}/flawed-design.md"),
        "--record-out".to_owned(),
        path_str(record_out),
        "--review-id".to_owned(),
        tag.to_owned(),
        "--objective".to_owned(),
        "Delegated admin permissions apply transitively and revoke immediately.".to_owned(),
        "--proposed-contract".to_owned(),
        "grant_delegation / revoke_delegation / effective_permissions".to_owned(),
        "--constraint".to_owned(),
        "revocation must be immediate".to_owned(),
        "--open-question".to_owned(),
        "Is eventual consistency acceptable?".to_owned(),
        "--doc-path".to_owned(),
        format!("{FIXTURES}/flawed-design.md"),
    ]
}

/// Runs `design-review run` against the flawed fixture design with the
/// supplied extra argv and returns the parsed record.
pub fn run_flawed_review(tag: &str, extra: &[String]) -> Value {
    let tmp = temp_dir(tag);
    let record_out = tmp.join("record.json");
    let mut args = run_args(tag, &record_out);
    args.extend_from_slice(extra);
    let (code, stdout, stderr) = run_cli(&args);
    assert_eq!(code, Some(0), "run failed: {stdout} {stderr}");
    record_of(&record_out)
}
