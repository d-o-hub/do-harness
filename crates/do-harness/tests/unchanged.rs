//! Unchanged-input reuse (`verify --unchanged`): a recorded passing beat is
//! reused only when the sensor's declared inputs are provably identical, and
//! reuse is reported as reuse — never as a fresh observation.
//!
//! These fixtures drive the real `do-harness` binary inside temporary git
//! repositories; the probe sensor appends to `runs.log`, so "ran" versus
//! "reused" is proven by process side effects, not by self-report.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use support::reuse::{
    PROBE, assert_ran, assert_reused, evidence, fixture, fixture_no_git, git, log, probe_config,
    status, verify, verify_code, verify_raw,
};

/// The full lifecycle: first record executes, the default `warn` mode
/// executes and advises, `skip` reuses without spawning, `run` forces a
/// fresh execution and re-seeds the cache.
#[test]
fn reuse_lifecycle_ran_warn_skip_run() {
    let (_dir, root) = fixture(&probe_config(PROBE));

    // 1. First recorded run executes and stores the input identity.
    let (value, stderr) = verify_code(&root, &["--record"], 0);
    assert_ran(&value, 0);
    assert_eq!(log(&root), "hit");
    assert!(
        !stderr.contains("would reuse"),
        "no cache entry exists yet: {stderr}"
    );
    assert_eq!(evidence(&root)["sensors"][0]["recorded"], true);

    // 2. Default `warn` mode executes `ran` (fresh completion evidence) but
    //    advises that the unchanged inputs would be reusable.
    let (value, stderr) = verify_code(&root, &["--record"], 0);
    assert_ran(&value, 0);
    assert_eq!(log(&root), "hithit");
    assert!(
        stderr.contains("unchanged inputs; would reuse beat"),
        "expected the eligibility advisory: {stderr}"
    );
    let doc = evidence(&root);
    assert_eq!(doc["summary"]["verdict"], "pass");
    assert_eq!(doc["summary"]["skip"], 0);

    // 3. `skip` reuses the recorded beat without spawning the sensor.
    let (value, stderr) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    let reused_beat = assert_reused(&value, 0);
    assert!(reused_beat > 0, "reused beat id: {value}");
    assert_eq!(log(&root), "hithit", "the command must not be spawned");
    assert!(
        stderr.contains("SKIP") && stderr.contains(&format!("reused beat {reused_beat}")),
        "expected an explicit reuse report: {stderr}"
    );
    let doc = evidence(&root);
    assert_eq!(doc["sensors"][0]["verdict"], "skip");
    assert_eq!(doc["sensors"][0]["recorded"], false);
    assert_eq!(doc["summary"]["skip"], 1);
    assert_eq!(doc["summary"]["verdict"], "fail");

    // 4. `run` forces execution even though inputs are unchanged, and the
    //    fresh pass seeds the cache again.
    let (value, _) = verify_code(&root, &["--record", "--unchanged=run"], 0);
    assert_ran(&value, 0);
    assert_eq!(log(&root), "hithithit");
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_reused(&value, 0);
    assert_eq!(log(&root), "hithithit");
}

/// Every changed input state runs: content edit, addition, deletion, and
/// rename of a matching path; a nonmatching edit leaves reuse intact.
#[test]
fn input_changes_force_a_fresh_run() {
    let config = probe_config(
        "name = \"probe\"\nargv = [\"sh\", \"-c\", \"printf hit >> runs.log\"]\ninputs = [\"input*.txt\"]\n",
    );
    let (_dir, root) = fixture(&config);
    verify_code(&root, &["--record"], 0);
    assert_eq!(log(&root), "hit");

    // Nonmatching edit: identical identity, reuse still applies.
    std::fs::write(root.join("other.txt"), "v2\n").unwrap();
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_reused(&value, 0);
    assert_eq!(log(&root), "hit");

    // Matching content edit: runs.
    std::fs::write(root.join("input.txt"), "v2\n").unwrap();
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);
    assert_eq!(log(&root), "hithit");

    // Addition matching the glob: runs.
    std::fs::write(root.join("input-extra.txt"), "x\n").unwrap();
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);

    // Deletion of a tracked matching file with the index entry still
    // present: the input set is incomplete, so the identity is unavailable.
    std::fs::remove_file(root.join("input-extra.txt")).unwrap();
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);

    // Rename into a matching name: both sides of the rename are covered by
    // the complete current path set.
    git(&root, &["mv", "input.txt", "input-moved.txt"]);
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);

    // Staged deletion leaves no matching path at all: never reuse.
    std::fs::remove_file(root.join("input-moved.txt")).unwrap();
    git(&root, &["add", "-A"]);
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);
}

/// The complete sensor spec and the raw config bytes are part of the
/// identity: an `argv` or `inputs` edit invalidates the recorded pass.
#[test]
fn config_edits_invalidate_the_recorded_identity() {
    let (_dir, root) = fixture(&probe_config(PROBE));
    verify_code(&root, &["--record"], 0);
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_reused(&value, 0);

    // argv edit: the command itself is pinned.
    std::fs::write(
        root.join("do-harness.toml"),
        probe_config(
            "name = \"probe\"\nargv = [\"sh\", \"-c\", \"printf hit2 >> runs.log\"]\ninputs = [\"input.txt\"]\n",
        ),
    )
    .unwrap();
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);
    assert_eq!(log(&root), "hithit2");

    // inputs-declaration edit: the declared set itself is pinned.
    std::fs::write(
        root.join("do-harness.toml"),
        probe_config(
            "name = \"probe\"\nargv = [\"sh\", \"-c\", \"printf hit3 >> runs.log\"]\ninputs = [\"input.txt\", \"other.txt\"]\n",
        ),
    )
    .unwrap();
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);
    assert_eq!(log(&root), "hithit2hit3");
}

/// Reuse is weak evidence: `--strict` fails the run and `status` refuses to
/// report green until a forced fresh run.
#[test]
fn reuse_is_not_fresh_evidence_for_strict_or_status() {
    let (_dir, root) = fixture(&probe_config(PROBE));
    verify_code(&root, &["--record"], 0);
    assert_eq!(status(&root).1["state"], "green");

    let (code, value, stderr) = verify(&root, &["--record", "--unchanged=skip", "--strict"]);
    assert_eq!(
        code,
        Some(1),
        "strict must reject reused evidence: {value}\n{stderr}"
    );
    assert!(
        stderr.contains("strict evidence check failed"),
        "expected the strict rejection: {stderr}"
    );
    let (code, doc) = status(&root);
    assert_eq!(code, Some(1), "reused evidence is not green: {doc}");
    assert_ne!(doc["state"], "green");

    verify_code(&root, &["--record", "--unchanged=run"], 0);
    let (code, doc) = status(&root);
    assert_eq!(code, Some(0), "a forced fresh run must be green: {doc}");
    assert_eq!(doc["state"], "green");
}

/// Explicit `skip` without `--record` is a usage error, a sensor without an
/// `inputs` declaration always runs, and a non-git workspace always runs.
#[test]
fn unsafe_cases_always_run() {
    let (_dir, root) = fixture(&probe_config(PROBE));
    let (code, _stdout, stderr) = verify_raw(&root, &["--unchanged=skip"]);
    assert_eq!(
        code,
        Some(2),
        "skip without --record must be a usage error: {stderr}"
    );
    assert!(stderr.contains("requires --record"), "stderr: {stderr}");

    // No declaration: no cache, whatever the mode.
    let undeclared =
        probe_config("name = \"probe\"\nargv = [\"sh\", \"-c\", \"printf hit >> runs.log\"]\n");
    let (_undeclared_dir, undeclared_root) = fixture(&undeclared);
    verify_code(&undeclared_root, &["--record"], 0);
    verify_code(&undeclared_root, &["--record"], 0);
    let (value, _) = verify_code(&undeclared_root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);
    assert_eq!(log(&undeclared_root), "hithithit");

    // Not a git repository: input enumeration fails closed.
    let (_plain_dir, plain_root) = fixture_no_git(&probe_config(PROBE));
    verify_code(&plain_root, &["--record"], 0);
    verify_code(&plain_root, &["--record"], 0);
    let (value, _) = verify_code(&plain_root, &["--record", "--unchanged=skip"], 0);
    assert_ran(&value, 0);
    assert_eq!(log(&plain_root), "hithithit");
}
