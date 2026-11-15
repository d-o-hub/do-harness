//! Acceptance tests for rejecting empty change-aware verification input.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::range_selection::{committed_rust_change, fixture_repo, sensor_names};
use support::{git, harness, run};

const NO_CHANGE: &str = "no change input: pass --since <REF> for committed changes or run verify without change filtering";

fn fixture_with_always_sensor() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, root) = fixture_repo();
    let config_path = root.join("do-harness.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    let config = config.replace(
        "all = [\"rs-check\", \"docs\"]",
        "all = [\"rs-check\", \"docs\", \"always\"]",
    );
    assert!(config.contains("all = [\"rs-check\", \"docs\", \"always\"]"));
    std::fs::write(
        config_path,
        format!(
            "{config}\n[[sensors]]\nname = \"always\"\nargv = [\"sh\", \"-c\", \"printf ran >> .do-harness/always-ran\"]\n"
        ),
    )
    .unwrap();
    git(&root, &["add", "do-harness.toml"]);
    git(&root, &["commit", "-qm", "test: add always sensor"]);
    (dir, root)
}

fn run_empty_rejected(root: &Path, args: &[&str]) {
    let (code, stdout, stderr) = run(harness(root).args(args));
    assert_eq!(
        code,
        Some(2),
        "expected usage rejection:\n{stdout}\n{stderr}"
    );
    assert!(stderr.contains(NO_CHANGE), "missing diagnostic:\n{stderr}");
    assert!(stdout.is_empty(), "rejection printed a report:\n{stdout}");
}

fn assert_empty_rejected(root: &Path, args: &[&str]) {
    run_empty_rejected(root, args);
    assert!(
        !root.join(".do-harness").exists(),
        "rejected selection created managed state"
    );
}

fn sensor_beat_count(root: &Path) -> i64 {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build runtime")
        .block_on(async {
            let connection = do_harness_db::connect_and_migrate(root)
                .await
                .expect("open state database");
            do_harness_db::sensor_stats(&connection, None, None)
                .await
                .expect("read sensor beat counts")
                .into_iter()
                .map(|stat| stat.runs)
                .sum()
        })
}

fn assert_no_running_marker(root: &Path) {
    let markers = root.join(".do-harness/evidence.all.running");
    if markers.exists() {
        assert!(
            std::fs::read_dir(markers).unwrap().next().is_none(),
            "rejected selection published a running marker"
        );
    }
}

#[test]
fn empty_change_filters_reject_before_markers_sensors_or_state() {
    let (_dir, root) = fixture_with_always_sensor();
    for args in [
        &["verify", "--set", "all", "--changed"][..],
        &["verify", "--set", "all", "--changed", "--strict"],
        &["verify", "--set", "all", "--changed", "--only", "always"],
        &["verify", "--set", "all", "--changed", "--record"],
        &["--dry-run", "verify", "--set", "all", "--changed"],
        &["verify", "--set", "all", "--since", "HEAD"],
        &["verify", "--set", "all", "--since", "HEAD", "--changed"],
    ] {
        assert_empty_rejected(&root, args);
    }
}

#[test]
fn rejection_preserves_existing_evidence_and_recorded_beats() {
    let (_dir, root) = fixture_with_always_sensor();
    let base = committed_rust_change(&root);
    let (code, stdout, stderr) = run(harness(&root).args([
        "verify",
        "--set",
        "all",
        "--since",
        base.as_str(),
        "--record",
        "--format",
        "json",
    ]));
    assert_eq!(code, Some(0), "range verify failed:\n{stdout}\n{stderr}");
    let _: Value = serde_json::from_str(&stdout).expect("verify report");

    let evidence_path = root.join(".do-harness/evidence.all.json");
    let evidence_before = std::fs::read(&evidence_path).expect("evidence artifact");
    let marker_before = std::fs::read(root.join(".do-harness/always-ran")).unwrap();
    let beats_before = sensor_beat_count(&root);
    assert!(beats_before > 0, "recorded run must write sensor beats");
    assert_no_running_marker(&root);

    run_empty_rejected(
        &root,
        &["verify", "--set", "all", "--since", "HEAD", "--record"],
    );
    assert_eq!(
        std::fs::read(&evidence_path).unwrap(),
        evidence_before,
        "rejected verify changed existing evidence"
    );
    assert_eq!(
        std::fs::read(root.join(".do-harness/always-ran")).unwrap(),
        marker_before,
        "rejected verify executed the always-applicable sensor"
    );
    assert_eq!(sensor_beat_count(&root), beats_before);
    assert_no_running_marker(&root);
}

#[test]
fn nonempty_committed_and_working_tree_changes_select_unconstrained_sensor() {
    let (_dir, root) = fixture_with_always_sensor();
    let base = committed_rust_change(&root);
    let (code, stdout, stderr) = run(harness(&root).args([
        "verify",
        "--set",
        "all",
        "--since",
        base.as_str(),
        "--format",
        "json",
    ]));
    assert_eq!(code, Some(0), "committed range failed:\n{stdout}\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("range report");
    assert_eq!(sensor_names(&report), ["rs-check", "always"]);

    std::fs::write(root.join("main.rs"), "fn main() { let working = true; }\n").unwrap();
    let (code, stdout, stderr) =
        run(harness(&root).args(["verify", "--set", "all", "--changed", "--format", "json"]));
    assert_eq!(
        code,
        Some(0),
        "working-tree change failed:\n{stdout}\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("working-tree report");
    assert_eq!(sensor_names(&report), ["rs-check", "always"]);
}
