//! Acceptance tests for independently range-bound status evaluation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::range_selection::{committed_rust_change, fixture_repo, rev};
use support::{git, git_command, harness, run};

const NO_CHANGE: &str = "no change input: pass --since <REF> for committed changes or run verify without change filtering";

fn verify(
    root: &Path,
    set: &str,
    since: Option<&str>,
    extra: &[&str],
) -> (Option<i32>, String, String) {
    let mut command = harness(root);
    command.arg("verify").arg("--set").arg(set);
    if let Some(since) = since {
        command.arg("--since").arg(since);
    }
    command.args(extra).args(["--format", "json"]);
    run(&mut command)
}

fn status_raw(
    root: &Path,
    set: &str,
    since: Option<&str>,
    extra: &[&str],
) -> (Option<i32>, String, String) {
    let mut command = harness(root);
    command.arg("status").arg("--set").arg(set);
    if let Some(since) = since {
        command.arg("--since").arg(since);
    }
    command.args(extra).args(["--format", "json"]);
    run(&mut command)
}

fn status(
    root: &Path,
    set: &str,
    since: Option<&str>,
    extra: &[&str],
) -> (Option<i32>, Value, String) {
    let (code, stdout, stderr) = status_raw(root, set, since, extra);
    let report = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("status stdout is not JSON:\n{stdout}\n{stderr}"));
    (code, report, stderr)
}

fn assert_selection_changed(root: &Path, since: &str) -> Value {
    let (code, report, stderr) = status(root, "all", Some(since), &[]);
    assert_eq!(code, Some(1), "status should be stale:\n{report}\n{stderr}");
    assert_eq!(report["state"], "stale");
    assert_eq!(report["reason"], "selection_changed");
    report
}

fn replace_rs_argv(root: &Path, argv: &str) {
    let config_path = root.join("do-harness.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    let old = "name = \"rs-check\"\nargv = [\"true\"]";
    let replacement = format!("name = \"rs-check\"\nargv = {argv}");
    assert!(config.contains(old), "fixture sensor config changed");
    std::fs::write(config_path, config.replacen(old, &replacement, 1)).unwrap();
    git(root, &["add", "do-harness.toml"]);
    git(root, &["commit", "-qm", "test: configure rust sensor"]);
}

fn db_metadata(root: &Path) -> Vec<Option<(u64, std::time::SystemTime)>> {
    ["agent_state.db", "agent_state.db-wal", "agent_state.db-shm"]
        .iter()
        .map(|name| {
            std::fs::metadata(root.join(".do-harness").join(name))
                .ok()
                .map(|metadata| (metadata.len(), metadata.modified().unwrap()))
        })
        .collect()
}

#[test]
fn explicit_range_matches_and_status_is_read_only_with_override_support() {
    let (_dir, root) = fixture_repo();
    replace_rs_argv(
        &root,
        r#"["sh", "-c", "printf ran >> .do-harness/status-marker"]"#,
    );
    let base = committed_rust_change(&root);
    let (code, stdout, stderr) = verify(&root, "all", Some(&base), &["--record"]);
    assert_eq!(code, Some(0), "range verify failed:\n{stdout}\n{stderr}");

    let default_evidence = root.join(".do-harness/evidence.all.json");
    let evidence_before = std::fs::read(&default_evidence).unwrap();
    let marker_before = std::fs::read(root.join(".do-harness/status-marker")).unwrap();
    let database_before = db_metadata(&root);
    assert!(database_before.iter().any(Option::is_some));

    let (code, report, stderr) = status(&root, "all", Some(&base), &[]);
    assert_eq!(
        code,
        Some(0),
        "matching range should be green:\n{report}\n{stderr}"
    );
    assert_eq!(report["state"], "green");

    let override_path = root.join(".do-harness/evidence.override.json");
    let backup_path = root.join(".do-harness/evidence.all.saved.json");
    std::fs::copy(&default_evidence, &override_path).unwrap();
    std::fs::rename(&default_evidence, &backup_path).unwrap();
    let override_arg = override_path.to_str().unwrap();
    let (code, report, stderr) = status(&root, "all", Some(&base), &["--evidence", override_arg]);
    std::fs::rename(&backup_path, &default_evidence).unwrap();
    assert_eq!(
        code,
        Some(0),
        "explicit evidence should be green:\n{report}\n{stderr}"
    );
    assert_eq!(report["state"], "green");
    assert_eq!(std::fs::read(&default_evidence).unwrap(), evidence_before);
    assert_eq!(
        std::fs::read(root.join(".do-harness/status-marker")).unwrap(),
        marker_before
    );
    assert_eq!(db_metadata(&root), database_before);
}

#[test]
fn explicit_range_rejects_unfiltered_other_spelling_and_combined_evidence() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    let (code, stdout, stderr) = verify(&root, "all", None, &[]);
    assert_eq!(
        code,
        Some(0),
        "unfiltered verify failed:\n{stdout}\n{stderr}"
    );
    assert_selection_changed(&root, &base);

    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    git(&root, &["branch", "same-base", &base]);
    let (code, stdout, stderr) = verify(&root, "all", Some(&base), &[]);
    assert_eq!(code, Some(0), "range verify failed:\n{stdout}\n{stderr}");
    assert_selection_changed(&root, "same-base");

    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    std::fs::write(root.join("main.rs"), "fn main() { let worktree = true; }\n").unwrap();
    let (code, stdout, stderr) = verify(&root, "all", Some(&base), &["--changed"]);
    assert_eq!(code, Some(0), "combined verify failed:\n{stdout}\n{stderr}");
    assert_selection_changed(&root, &base);
}

#[test]
fn moved_base_ref_makes_status_stale_and_keeps_range_in_recovery() {
    let (_dir, root) = fixture_repo();
    let original = rev(&root, "HEAD");
    git(&root, &["branch", "base", &original]);
    std::fs::write(root.join("README.md"), "# intermediate\n").unwrap();
    git(&root, &["add", "README.md"]);
    git(&root, &["commit", "-qm", "test: intermediate commit"]);
    let intermediate = rev(&root, "HEAD");
    committed_rust_change(&root);

    let (code, stdout, stderr) = verify(&root, "all", Some("base"), &[]);
    assert_eq!(code, Some(0), "range verify failed:\n{stdout}\n{stderr}");
    git(&root, &["update-ref", "refs/heads/base", &intermediate]);
    let report = assert_selection_changed(&root, "base");
    assert!(
        report["recovery"]
            .as_str()
            .unwrap()
            .contains("--since 'base'"),
        "recovery must retain the requested base ref: {report}"
    );
}

#[test]
fn missing_and_invalid_ranges_fail_closed_with_actionable_recovery() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    let (code, report, stderr) = status(&root, "all", Some(&base), &[]);
    assert_eq!(
        code,
        Some(1),
        "missing range should exit 1:\n{report}\n{stderr}"
    );
    assert_eq!(report["state"], "missing");
    assert_eq!(report["reason"], "no_evidence");
    assert_eq!(
        report["recovery"],
        format!("do-harness verify --set all --record --since '{base}'")
    );

    git(&root, &["branch", "feature/it's", &base]);
    let (code, report, stderr) = status(&root, "all", Some("feature/it's"), &[]);
    assert_eq!(
        code,
        Some(1),
        "valid range should remain missing:\n{report}\n{stderr}"
    );
    assert_eq!(report["state"], "missing");
    assert_eq!(
        report["recovery"],
        r"do-harness verify --set all --record --since 'feature/it'\''s'"
    );

    let (code, stdout, stderr) = status_raw(&root, "all", Some("unknown-ref"), &[]);
    assert_eq!(
        code,
        Some(2),
        "unknown ref must be a usage error:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("'unknown-ref' does not resolve to a commit"),
        "{stderr}"
    );
    let (code, stdout, stderr) = status_raw(&root, "all", Some("HEAD"), &[]);
    assert_eq!(
        code,
        Some(2),
        "empty range must be a usage error:\n{stdout}\n{stderr}"
    );
    assert!(stderr.contains(NO_CHANGE), "{stderr}");

    let (_dir, unrelated) = fixture_repo();
    let base = rev(&unrelated, "HEAD");
    git(&unrelated, &["branch", "base", &base]);
    git(&unrelated, &["checkout", "--orphan", "unrelated-history"]);
    std::fs::write(
        unrelated.join("main.rs"),
        "fn main() { let other = true; }\n",
    )
    .unwrap();
    git(&unrelated, &["add", "-A"]);
    git(&unrelated, &["commit", "-qm", "test: unrelated history"]);
    let (code, stdout, stderr) = status_raw(&unrelated, "all", Some("base"), &[]);
    assert_eq!(
        code,
        Some(2),
        "unrelated history must be a usage error:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("merge-base") || stderr.contains("merge base"),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn shallow_missing_base_object_is_a_usage_error_without_fetching() {
    let (_source_dir, source) = fixture_repo();
    let base = committed_rust_change(&source);
    let clone_dir = tempfile::tempdir().unwrap();
    let clone_root = clone_dir.path().join("shallow");
    let source_url = format!("file://{}", source.display());
    let clone = git_command(clone_dir.path())
        .args([
            "clone",
            "--depth=1",
            source_url.as_str(),
            clone_root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        clone.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&clone.stderr)
    );

    let (code, stdout, stderr) = status_raw(&clone_root, "all", Some(&base), &[]);
    assert_eq!(
        code,
        Some(2),
        "missing shallow object must be a usage error:\n{stdout}\n{stderr}"
    );
    assert!(stderr.contains("does not resolve to a commit"), "{stderr}");
}

#[test]
fn insufficient_stronger_coverage_failed_evidence_and_running_marker_keep_states() {
    let (_dir, root) = fixture_repo();
    let config_path = root.join("do-harness.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    let config = config.replacen(
        "all = [\"rs-check\", \"docs\"]",
        "all = [\"rs-check\", \"docs\"]\nstronger = [\"rs-check\", \"docs\", \"extra\"]",
        1,
    );
    std::fs::write(
        config_path,
        format!("{config}\n[[sensors]]\nname = \"extra\"\nargv = [\"true\"]\n"),
    )
    .unwrap();
    git(&root, &["add", "do-harness.toml"]);
    git(&root, &["commit", "-qm", "test: add stronger set"]);
    let base = rev(&root, "HEAD");
    std::fs::write(root.join("README.md"), "# changed docs\n").unwrap();
    git(&root, &["add", "README.md"]);
    git(&root, &["commit", "-qm", "test: docs change"]);
    committed_rust_change(&root);
    let (code, stdout, stderr) = verify(&root, "stronger", Some(&base), &["--only", "rs-check"]);
    assert_eq!(
        code,
        Some(0),
        "partial stronger run failed:\n{stdout}\n{stderr}"
    );
    let (code, report, stderr) = status(&root, "all", Some(&base), &[]);
    assert_eq!(
        code,
        Some(1),
        "inadequate coverage must not pass:\n{report}\n{stderr}"
    );
    assert_eq!(report["state"], "missing");
    let (_dir, partial) = fixture_repo();
    let base = rev(&partial, "HEAD");
    std::fs::write(partial.join("README.md"), "# changed docs\n").unwrap();
    git(&partial, &["add", "README.md"]);
    git(&partial, &["commit", "-qm", "test: docs change"]);
    committed_rust_change(&partial);
    let (code, stdout, stderr) = verify(&partial, "all", Some(&base), &["--only", "rs-check"]);
    assert_eq!(
        code,
        Some(0),
        "partial same-set verify failed:\n{stdout}\n{stderr}"
    );
    let (code, report, stderr) = status(&partial, "all", Some(&base), &[]);
    assert_eq!(
        code,
        Some(1),
        "incomplete same-set coverage must stay missing:\n{report}\n{stderr}"
    );
    assert_eq!(report["state"], "missing");
    assert_eq!(report["reason"], "insufficient_coverage");
    assert_eq!(report["missing_sensors"], serde_json::json!(["docs"]));

    let (_dir, failed) = fixture_repo();
    replace_rs_argv(&failed, "[\"false\"]");
    let base = committed_rust_change(&failed);
    let (code, stdout, stderr) = verify(&failed, "all", Some(&base), &[]);
    assert_eq!(
        code,
        Some(1),
        "failing verify should be red:\n{stdout}\n{stderr}"
    );
    let (code, report, stderr) = status(&failed, "all", Some(&base), &[]);
    assert_eq!(
        code,
        Some(1),
        "matching failed evidence should be red:\n{report}\n{stderr}"
    );
    assert_eq!(report["state"], "red");
    assert_eq!(report["reason"], "failures");

    let (_dir, running) = fixture_repo();
    let base = committed_rust_change(&running);
    let markers = running.join(".do-harness/evidence.all.running");
    std::fs::create_dir_all(&markers).unwrap();
    let marker = markers.join("999999-1.json");
    std::fs::write(&marker, r#"{"pid":1,"started_at":1,"set":"all"}"#).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(marker)
        .unwrap();
    fs2::FileExt::lock_shared(&held).unwrap();
    let (code, report, stderr) = status(&running, "all", Some(&base), &[]);
    assert_eq!(
        code,
        Some(3),
        "valid range with live marker must be running:\n{report}\n{stderr}"
    );
    assert_eq!(report["state"], "running");
    drop(held);
}
