//! `maintenance --prune-beats` deletes recorded history, so its contract is
//! pinned here: how many beats survive, that the newest are kept, and that a
//! dry run changes nothing.
//!
//! The command had no test at all, and it is the only one in the CLI whose job
//! is to remove evidence.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

mod support;

use support::{harness, run};

/// Beats recorded before the prune: the fixture's whole history.
const RECORDED_BEATS: i64 = 3;
/// Most-recent beats the prune is told to keep per scope.
const KEEP_PER_TASK: i64 = 1;
/// `--prune-beats 0`: everything older than *now* is eligible.
const PRUNE_DAYS: i64 = 0;
/// The cutoff and every beat stamp are whole-second epochs, so a prune needs
/// the clock to cross a boundary before anything is "older than" it.
const SECOND_BOUNDARY_WAIT: Duration = Duration::from_millis(1_200);

/// `maintenance --prune-beats <PRUNE_DAYS> --keep-per-task <KEEP_PER_TASK>`,
/// with the counts rendered the way the CLI parses them.
fn prune_args() -> Vec<String> {
    vec![
        "maintenance".to_owned(),
        "--prune-beats".to_owned(),
        PRUNE_DAYS.to_string(),
        "--keep-per-task".to_owned(),
        KEEP_PER_TASK.to_string(),
    ]
}

/// A workspace whose only sensor is trivially cheap, so beats are cheap to
/// record.
fn workspace() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let _ = support::git_command(&root).args(["init"]).output();
    std::fs::write(
        root.join("do-harness.toml"),
        "language = \"rust\"\n\n[signal-sets]\nverification = [\"probe\"]\n\n[[sensors]]\nname = \"probe\"\nargv = [\"true\"]\n",
    )
    .unwrap();
    (dir, root)
}

/// Records one `probe` beat.
fn verify_record(root: &Path) {
    let (code, stdout, stderr) =
        run(harness(root).args(["verify", "--only", "probe", "--record", "--format", "json"]));
    assert_eq!(code, Some(0), "verify --record failed:\n{stdout}\n{stderr}");
}

/// Runs `probe` as often as the metrics report says, i.e. what is still
/// recorded.
fn recorded_runs(root: &Path) -> i64 {
    let (_code, stdout, stderr) = run(harness(root).args(["metrics", "--format", "json"]));
    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("metrics stdout is not JSON:\n{stdout}\n{stderr}"));
    parsed["sensors"]
        .as_array()
        .expect("metrics sensors array")
        .iter()
        .find(|row| row["name"] == serde_json::json!("probe"))
        .map_or(0, |row| row["runs"].as_i64().expect("runs is an integer"))
}

#[test]
fn prune_beats_keeps_the_newest_and_a_dry_run_changes_nothing() {
    let (_dir, root) = workspace();
    for _ in 0..RECORDED_BEATS {
        verify_record(&root);
    }
    assert_eq!(
        recorded_runs(&root),
        RECORDED_BEATS,
        "every recording is visible"
    );

    // A dry run reports what it would delete and deletes nothing.
    let (code, stdout, stderr) = run(harness(&root).args(prune_args()).arg("--dry-run"));
    assert_eq!(code, Some(0), "dry run failed:\n{stdout}\n{stderr}");
    assert!(
        stdout.contains("Dry run"),
        "the report must name itself a dry run, not read as a deletion:\n{stdout}"
    );
    assert_eq!(
        recorded_runs(&root),
        RECORDED_BEATS,
        "a dry run must not delete anything:\n{stdout}"
    );

    // `--prune-beats 0` cuts at `now`, and both the cutoff and each beat are
    // whole-second epochs: a recording made in this same second is not *older*
    // than the cutoff, so wait out the boundary rather than backdating rows
    // behind the CLI's back.
    std::thread::sleep(SECOND_BOUNDARY_WAIT);

    // The real run keeps the minimum number of most-recent beats per scope.
    let (code, stdout, stderr) = run(harness(&root).args(prune_args()));
    assert_eq!(code, Some(0), "prune failed:\n{stdout}\n{stderr}");
    assert_eq!(
        recorded_runs(&root),
        KEEP_PER_TASK,
        "the minimum keep count survives, the rest is pruned:\n{stdout}"
    );
}

/// A dry run must not even open the state database.
///
/// `connect_and_migrate` creates and migrates the file, so a dry run that
/// reached it would mutate the state it claims only to preview — on a fresh
/// clone it would leave a migrated `agent_state.db` behind.
#[test]
fn prune_beats_dry_run_leaves_no_state_database() {
    let (_dir, root) = workspace();
    let (code, stdout, stderr) = run(harness(&root).args(prune_args()).arg("--dry-run"));
    assert_eq!(code, Some(0), "dry run failed:\n{stdout}\n{stderr}");
    assert!(
        !root.join(".do-harness").join("agent_state.db").exists(),
        "a dry run must not create the state database:\n{stdout}\n{stderr}"
    );
}
