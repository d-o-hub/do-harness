//! Edge cases for unchanged-input reuse: shadowing beats, non-cacheable
//! results, parallel/fail-fast ordering, and branch/task/global scope
//! isolation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use support::reuse::{
    PROBE, config_with_sensors, execution, fixture, git, git_branch, log, probe_config, task_add,
    verify_code,
};

/// A later failed beat always shadows an earlier pass, even when the
/// declared inputs are unchanged since that pass.
#[test]
fn a_later_failed_beat_shadows_the_recorded_pass() {
    let flaky = "name = \"probe\"\nargv = [\"sh\", \"-c\", \"printf hit >> runs.log; test ! -f fail.flag\"]\ninputs = [\"input.txt\"]\n";
    let (_dir, root) = fixture(&probe_config(flaky));
    verify_code(&root, &["--record"], 0);
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "reused");

    // An out-of-band flag makes the forced run fail; the newest beat is now
    // `failed` with no digest.
    std::fs::write(root.join("fail.flag"), "").unwrap();
    verify_code(&root, &["--record", "--unchanged=run"], 1);

    // Inputs are identical to the stored pass again (fail.flag is not a
    // declared input), but the reused beat must be the newest one.
    std::fs::remove_file(root.join("fail.flag")).unwrap();
    let (value, stderr) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "ran", "must not reuse: {value}");
    assert!(!stderr.contains("would reuse"), "stderr: {stderr}");

    // The fresh pass seeds a new reusable beat.
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "reused");
}

/// `SKIP:`-marked passes and warn-severity failures are never cached: the
/// cache only holds clean exits it can fully attribute to declared inputs.
#[test]
fn skip_markers_and_warnings_are_never_reused() {
    let blocks = "[[sensors]]\nname = \"skipmarker\"\nargv = [\"sh\", \"-c\", \"printf hit >> runs.log; echo 'SKIP: tool missing'\"]\ninputs = [\"input.txt\"]\n\
                  \n[[sensors]]\nname = \"warnfail\"\nargv = [\"sh\", \"-c\", \"printf warn >> runs.log; exit 1\"]\nseverity = \"warn\"\ninputs = [\"input.txt\"]\n";
    let config = config_with_sensors(&["skipmarker", "warnfail"], blocks);
    let (_dir, root) = fixture(&config);

    // First run: the SKIP-marker sensor passes with a `SKIP:` marker, the
    // warn sensor records a `warn` status.
    let (value, _) = verify_code(&root, &["--record"], 0);
    assert_eq!(execution(&value, 0), "ran");
    assert_eq!(execution(&value, 1), "ran");
    assert_eq!(log(&root), "hitwarn");

    // Neither status is cache-eligible: `skip` executes both again.
    let (value, stderr) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "ran", "SKIP: result must not reuse");
    assert_eq!(execution(&value, 1), "ran", "warn result must not reuse");
    assert!(!stderr.contains("would reuse"), "stderr: {stderr}");
    assert_eq!(log(&root), "hitwarnhitwarn");
}

/// `--jobs 2` and `--fail-fast` keep the same reuse decisions as the
/// sequential path: a reused sensor is reported in order, never cancels a
/// sibling, and never pretends the expensive command was spawned.
#[test]
fn parallel_and_fail_fast_keep_reuse_ordering() {
    let blocks = format!(
        "[[sensors]]\n{PROBE}\n[[sensors]]\nname = \"boom\"\nargv = [\"sh\", \"-c\", \"exit 1\"]\ninputs = [\"input.txt\"]\n"
    );
    let config = format!(
        "language = \"generic\"\njobs = 2\n\n[signal-sets]\nverification = [\"probe\", \"boom\"]\n\n{blocks}"
    );
    let (_dir, root) = fixture(&config);

    // The failing baseline records the probe pass plus a boom failure.
    let (value, _) = verify_code(&root, &["--record"], 1);
    assert_eq!(execution(&value, 0), "ran");
    assert_eq!(execution(&value, 1), "ran");
    assert_eq!(log(&root), "hit");

    // Parallel skip without fail-fast: probe reuses, boom executes and fails.
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 1);
    assert_eq!(execution(&value, 0), "reused");
    assert_eq!(execution(&value, 1), "ran");
    assert_eq!(log(&root), "hit", "reuse must not spawn the probe");

    // The forcing path re-executes the probe even though it is eligible.
    let (value, _) = verify_code(&root, &["--record", "--unchanged=run"], 1);
    assert_eq!(execution(&value, 0), "ran");
    assert_eq!(log(&root), "hithit");

    // The reused decision holds in the sequential fail-fast branch too: a
    // reused sibling never triggers cancellation, and the later failure
    // still stops nothing that already ran.
    let sequential = format!(
        "language = \"generic\"\n\n[signal-sets]\nverification = [\"probe\", \"boom\"]\n\n{blocks}"
    );
    let (_seq_dir, seq_root) = fixture(&sequential);
    verify_code(&seq_root, &["--record"], 1);
    let (value, _) = verify_code(
        &seq_root,
        &["--record", "--unchanged=skip", "--fail-fast"],
        1,
    );
    assert_eq!(execution(&value, 0), "reused");
    assert_eq!(execution(&value, 1), "ran");
    assert_eq!(log(&seq_root), "hit");
}

/// Beats are namespaced by scope: a bare `--record` belongs to the current
/// branch, `--global` has its own cache, and `--task <id>` has its own; no
/// scope can borrow another's recorded pass, and each reuses its own.
#[test]
fn scoped_beats_stay_in_their_namespace() {
    let (_dir, root) = fixture(&probe_config(PROBE));
    let task_id = task_add(&root);
    let scoped = task_id.to_string();

    // A task-scoped pass, then a default (branch-scoped) skip run: the
    // branch scope has no beat of its own, so it must run.
    verify_code(&root, &["--record", "--task", &scoped], 0);
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "ran");
    assert_eq!(log(&root), "hithit");

    // The branch namespace now reuses the pass it just recorded...
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "reused");
    assert_eq!(log(&root), "hithit");

    // ...the task namespace still reuses its own pass...
    let (value, _) = verify_code(
        &root,
        &["--record", "--unchanged=skip", "--task", &scoped],
        0,
    );
    assert_eq!(execution(&value, 0), "reused");
    assert_eq!(log(&root), "hithit");

    // ...and the global namespace must run: nothing was recorded there.
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip", "--global"], 0);
    assert_eq!(execution(&value, 0), "ran");
    assert_eq!(log(&root), "hithithit");

    // Once recorded globally, it reuses independently of branch and task.
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip", "--global"], 0);
    assert_eq!(execution(&value, 0), "reused");
    assert_eq!(log(&root), "hithithit");

    // An unrelated task id has no beats: it runs rather than borrowing.
    let other = task_add(&root);
    let (value, _) = verify_code(
        &root,
        &["--record", "--unchanged=skip", "--task", &other.to_string()],
        0,
    );
    assert_eq!(execution(&value, 0), "ran");
    assert_eq!(log(&root), "hithithithit");
}

/// A beat recorded on one branch never satisfies a lookup on another, even
/// at an identical HEAD with identical input bytes; switching back to the
/// original branch resumes reuse.
#[test]
fn branch_switch_isolates_recorded_beats() {
    let (_dir, root) = fixture(&probe_config(PROBE));
    let initial = git_branch(&root);

    // Record on the initial branch: one execution, then reuse there.
    verify_code(&root, &["--record"], 0);
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "reused");
    assert_eq!(log(&root), "hit");

    // Same HEAD, new branch: the recorded beat belongs to the other branch.
    git(&root, &["switch", "-c", "alternate"]);
    let (value, stderr) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(
        execution(&value, 0),
        "ran",
        "another branch's beat must not be reused: {value}"
    );
    assert!(!stderr.contains("would reuse"), "stderr: {stderr}");
    assert_eq!(log(&root), "hithit");

    // Switching back resumes reuse of the original branch's beat; the
    // `alternate` branch keeps its own recorded pass meanwhile.
    git(&root, &["switch", &initial]);
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "reused");
    assert_eq!(log(&root), "hithit");

    git(&root, &["switch", "alternate"]);
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(execution(&value, 0), "reused");
    assert_eq!(log(&root), "hithit");
}

/// Evidence recording stays honest under reuse: a reused sensor never
/// rewrites the recorded beat and never shrinks the strike history.
#[test]
fn reuse_writes_no_new_beat() {
    let (_dir, root) = fixture(&probe_config(PROBE));
    let (value, _) = verify_code(&root, &["--record"], 0);
    let beat = value["sensors"][0]["reused_beat_id"].as_i64();
    assert_eq!(beat, None, "a fresh run has no reused beat");

    // Two reuse runs in a row must be stable: the same beat id comes back
    // and no later beat shadows it.
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    let first = value["sensors"][0]["reused_beat_id"].as_i64();
    let (value, _) = verify_code(&root, &["--record", "--unchanged=skip"], 0);
    assert_eq!(
        value["sensors"][0]["reused_beat_id"].as_i64(),
        first,
        "reuse must not create beats: {value}"
    );
    assert!(first.is_some_and(|id| id > 0));
    assert_eq!(log(&root), "hit");
}
