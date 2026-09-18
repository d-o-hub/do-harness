//! Acceptance fixtures for the deterministic DORA collector.
//!
//! Every fixture pins commit timestamps and injects the clock with `--now`,
//! so no test reads a real clock and the asserted numbers are exact. The
//! point of these fixtures is the pair of contracts the design exists for:
//! a metric is only reported with its derivation, and identical input yields
//! byte-identical output.
//!
//! Sensor-level acceptance lives in `dora_sensor.rs`; shared fixtures in
//! `support::dora`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::dora::{
    T0, breach_names, commit_at, deployed_fixture, dora_json, fixture_repo, git, harness, run,
};

#[test]
fn deployed_fixture_derives_exact_metrics_and_breaches() {
    let (_dir, root) = deployed_fixture();
    let (stdout, stderr, snapshot) = dora_json(&root, T0 + 86_400, 1);

    assert_eq!(snapshot["deploy_count"], serde_json::json!(2));
    assert_eq!(
        snapshot["deploy_tags"],
        serde_json::json!(["v0.1.0", "v0.1.1"])
    );
    assert_eq!(snapshot["lead_samples"], serde_json::json!(3));
    assert_eq!(snapshot["lead_p50_seconds"], serde_json::json!(0));
    assert_eq!(snapshot["lead_p90_seconds"], serde_json::json!(100));
    assert_eq!(snapshot["deploys_failed"], serde_json::json!(1));
    assert_eq!(snapshot["mttr_restored"], serde_json::json!(0));
    assert_eq!(snapshot["mttr_unrestored"], serde_json::json!(1));
    assert_eq!(snapshot["mttr_seconds"], Value::Null);
    assert_eq!(
        breach_names(&snapshot),
        vec!["change_failure_rate", "unrestored_deploys"]
    );

    // The derivation manifest is the whole point: the number is only
    // reportable because its inputs are recorded with it.
    let ranges = snapshot["derivation"]["ranges"].as_array().unwrap();
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0]["tag"], serde_json::json!("v0.1.0"));
    assert_eq!(ranges[0]["range"], serde_json::json!("v0.1.0"));
    assert_eq!(ranges[1]["range"], serde_json::json!("v0.1.0..v0.1.1"));
    assert_eq!(
        ranges[1]["failure_range"],
        serde_json::json!("v0.1.1..HEAD")
    );
    let incidents = snapshot["derivation"]["incidents"].as_array().unwrap();
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0]["restore_ts"], Value::Null);
    assert_eq!(
        snapshot["derivation"]["clock_skew_commits"],
        serde_json::json!(0)
    );
    assert_eq!(
        snapshot["derivation"]["percentile_method"],
        serde_json::json!("nearest-rank")
    );

    assert!(stdout.contains("\"source_rev\""));
    assert!(stderr.contains("FINDINGS: 2"), "stderr: {stderr}");
    let coverage: Vec<&str> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("COVERAGE: "))
        .collect();
    assert_eq!(coverage.len(), 1, "exactly one COVERAGE line: {stderr}");
    let parsed: Value = serde_json::from_str(coverage[0]).expect("COVERAGE is JSON");
    assert_eq!(parsed["ranges"].as_array().unwrap().len(), 2);
}

#[test]
fn repeated_runs_are_byte_identical() {
    let (_dir, root) = deployed_fixture();
    let (first, _, _) = dora_json(&root, T0 + 86_400, 1);
    let (second, _, _) = dora_json(&root, T0 + 86_400, 1);
    assert_eq!(
        first, second,
        "a metric that is not provably re-derivable is an opinion with a decimal point"
    );
}

#[test]
fn clean_deploy_history_reports_no_breach() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    git(&root, &["tag", "v0.1.0"]);
    commit_at(&root, T0 + 100, "feat: a");

    let (_stdout, stderr, snapshot) = dora_json(&root, T0 + 86_400, 0);
    assert_eq!(snapshot["deploy_count"], serde_json::json!(1));
    assert_eq!(snapshot["deploys_failed"], serde_json::json!(0));
    assert!(breach_names(&snapshot).is_empty());
    assert!(stderr.contains("FINDINGS: 0"), "stderr: {stderr}");
}

#[test]
fn tagless_history_reports_missing_measurements_as_unmeasured() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");

    let (_stdout, _stderr, snapshot) = dora_json(&root, T0 + 86_400, 1);
    assert_eq!(snapshot["deploy_count"], serde_json::json!(0));
    assert_eq!(snapshot["lead_p50_seconds"], Value::Null);
    assert_eq!(snapshot["lead_p90_seconds"], Value::Null);
    assert_eq!(snapshot["mttr_seconds"], Value::Null);
    // Both rules are intended: with no deploys there is no evidence of health,
    // and an unmeasurable lead time is a breach rather than a pass.
    assert_eq!(
        breach_names(&snapshot),
        vec!["deploys_below_min", "lead_p90"]
    );

    let (code, stdout, _stderr) = run(harness(&root)
        .arg("dora")
        .arg("--days")
        .arg("30")
        .arg("--now")
        .arg((T0 + 86_400).to_string()));
    assert_eq!(code, Some(1));
    assert!(stdout.contains("  lead time          p50 -  p90 -  (n=0)\n"));
    assert!(stdout.contains("  change failure rate -\n"));
    assert!(stdout.contains("  time to restore    -  (0 restored, 0 unrestored)\n"));
}

#[test]
fn missing_policy_is_a_usage_error_not_a_defaulted_measurement() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    std::fs::remove_file(root.join("plans/dora.json")).unwrap();

    let (code, _stdout, stderr) = run(harness(&root).arg("dora").arg("--now").arg(T0.to_string()));
    assert_eq!(code, Some(2), "stderr: {stderr}");
    assert!(stderr.contains("plans/dora.json"), "stderr: {stderr}");
}

#[test]
fn outside_a_work_tree_the_collector_refuses_to_answer() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("plans")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plans/dora.json"),
        root.join("plans/dora.json"),
    )
    .unwrap();

    let (code, stdout, stderr) = run(harness(&root).arg("dora").arg("--now").arg(T0.to_string()));
    assert_eq!(code, Some(2), "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        stderr.contains("not inside a git working tree"),
        "stderr: {stderr}"
    );
}

/// A tag cut directly on the revert commit still counts as the restorer.
///
/// Regression: restoration was found by scanning for a tag with
/// `deploy_ts > revert_ts`, so a release tagged on the revert commit itself
/// (`deploy_ts == revert_ts`) was skipped. The incident then reported as
/// unrestored — a spurious `unrestored_deploys` breach with a null MTTR — or
/// was credited to a later release, inflating the restore time.
#[test]
fn a_tag_on_the_revert_commit_restores_the_incident() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    git(&root, &["tag", "v0.1.0"]);
    commit_at(&root, T0 + 100, "feat: a");
    git(&root, &["tag", "v0.1.1"]);
    commit_at(
        &root,
        T0 + 300,
        "revert(edge): the fix shipped with the release",
    );
    git(&root, &["tag", "v0.1.2"]);

    let (_stdout, _stderr, snapshot) = dora_json(&root, T0 + 86_400, 1);
    assert_eq!(snapshot["mttr_restored"], serde_json::json!(1));
    assert_eq!(snapshot["mttr_unrestored"], serde_json::json!(0));
    assert_eq!(snapshot["mttr_seconds"], serde_json::json!(0));
    let incidents = snapshot["derivation"]["incidents"].as_array().unwrap();
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0]["restored_by"], serde_json::json!("v0.1.2"));
    assert_eq!(incidents[0]["restore_ts"], serde_json::json!(T0 + 300));
    // Restored, so the restore-failure rule must not fire.
    assert_eq!(breach_names(&snapshot), vec!["change_failure_rate"]);
}

/// Deploys are ordered by deploy time, not by when the tag object was written.
///
/// Regression: tags were read in `creatordate` order. For an annotated tag
/// that is the tagger date, so back-filling an older release after a newer one
/// ordered the older last and inverted the predecessor relationship —
/// `P..T` became `descendant..ancestor`, an empty range reporting zero
/// lead-time samples.
#[test]
fn a_backfilled_annotated_tag_still_orders_by_deploy_time() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    commit_at(&root, T0 + 500, "feat: a");
    git(&root, &["tag", "v0.1.1"]);
    // Annotate the older commit now: its tagger date is "now", far after
    // v0.1.1's, but its deploy time is still the earlier commit.
    git(
        &root,
        &["tag", "-a", "v0.1.0", "-m", "backfilled", "HEAD~1"],
    );

    let (_stdout, _stderr, snapshot) = dora_json(&root, T0 + 86_400, 0);
    assert_eq!(
        snapshot["deploy_tags"],
        serde_json::json!(["v0.1.0", "v0.1.1"]),
        "deploy order must follow the commit timeline"
    );
    let ranges = snapshot["derivation"]["ranges"].as_array().unwrap();
    assert_eq!(ranges[1]["range"], serde_json::json!("v0.1.0..v0.1.1"));
    assert!(
        ranges[1]["commits"].as_i64().unwrap() > 0,
        "a descendant..ancestor range silently reports zero commits: {snapshot}"
    );
}

/// Policy thresholds that would invert a rule are rejected, not honoured.
///
/// Regression: only `window_days` and the rate were validated. A negative
/// `max_unrestored` makes `mttr_unrestored > max_unrestored` true for a window
/// with *zero* incidents, so the policy reported a failure the history never
/// had.
#[test]
fn an_inverting_policy_threshold_is_refused() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");

    let pristine = std::fs::read_to_string(root.join("plans/dora.json")).unwrap();
    for (key, value) in [
        ("max_unrestored", "-1"),
        ("max_lead_p90_days", "0"),
        ("max_mttr_hours", "-5"),
        ("min_deploys", "-1"),
    ] {
        // Rebuild from the pristine policy each iteration: a cumulative edit
        // would let the first case's violation mask the next case's message.
        let mut policy: Value = serde_json::from_str(&pristine).unwrap();
        policy[key] = serde_json::json!(value.parse::<i64>().unwrap());
        std::fs::write(
            root.join("plans/dora.json"),
            serde_json::to_string_pretty(&policy).unwrap(),
        )
        .unwrap();

        let (code, _stdout, stderr) =
            run(harness(&root).arg("dora").arg("--now").arg(T0.to_string()));
        assert_eq!(code, Some(2), "{key}={value} must be refused:\n{stderr}");
        assert!(stderr.contains(key), "the error must name {key}: {stderr}");
    }
}

/// The bot allowlist drops lead-time samples without touching commit counts.
///
/// A release-engine or bot commit is not human lead time, so it must not
/// contribute a sample — but it is still a commit in the range, so the range's
/// commit count and the failure attribution must not change. An over-broad
/// allowlist is not silently benign either: with every author excluded there
/// are no samples left, so the lead-time percentile is unmeasured and breaches
/// rather than passing as healthy.
#[test]
fn the_bot_allowlist_drops_samples_but_keeps_commit_counts() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    git(&root, &["tag", "v0.1.0"]);
    commit_at(&root, T0 + 100, "feat: a");
    commit_at(&root, T0 + 200, "feat: b");
    git(&root, &["tag", "v0.1.1"]);

    let (_stdout, _stderr, unfiltered) = dora_json(&root, T0 + 86_400, 0);
    assert_eq!(unfiltered["lead_samples"], serde_json::json!(3));
    let ranges = unfiltered["derivation"]["ranges"].as_array().unwrap();
    assert_eq!(ranges[1]["commits"], serde_json::json!(2));

    // Every fixture commit is authored by t@t, so allowlisting it empties the
    // sample set while the commit counts stay put.
    let mut policy: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("plans/dora.json")).unwrap())
            .unwrap();
    policy["bot_allowlist"] = serde_json::json!(["t@t"]);
    std::fs::write(
        root.join("plans/dora.json"),
        serde_json::to_string_pretty(&policy).unwrap(),
    )
    .unwrap();

    let (_stdout, _stderr, filtered) = dora_json(&root, T0 + 86_400, 1);
    assert_eq!(filtered["lead_samples"], serde_json::json!(0));
    assert_eq!(filtered["lead_p50_seconds"], Value::Null);
    let ranges = filtered["derivation"]["ranges"].as_array().unwrap();
    assert_eq!(
        ranges[1]["commits"],
        serde_json::json!(2),
        "filtering samples must not change the range's commit count"
    );
}

/// Out-of-order commit timestamps are flagged, not silently clamped.
///
/// A commit timestamped after its own deploy is clock skew or a mistagged
/// release. It is counted as `clock_skew_commits` (data-quality evidence) and
/// still contributes a sample clamped at zero, so the skew is visible without
/// silently inflating or hiding the lead-time distribution.
#[test]
fn clock_skew_is_reported_and_clamped() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    git(&root, &["tag", "v0.1.0"]);
    // A commit stamped *after* the deploy that supposedly shipped it.
    commit_at(&root, T0 + 500, "feat: a (later stamp)");
    commit_at(&root, T0 + 100, "feat: b (earlier stamp)");
    git(&root, &["tag", "v0.1.1"]);

    let (_stdout, _stderr, snapshot) = dora_json(&root, T0 + 86_400, 0);
    assert_eq!(
        snapshot["derivation"]["clock_skew_commits"],
        serde_json::json!(1),
        "the post-deploy commit must be flagged: {snapshot}"
    );
    assert_eq!(
        snapshot["lead_samples"],
        serde_json::json!(3),
        "a skewed row is still a sample, clamped at zero"
    );
    assert_eq!(snapshot["lead_p50_seconds"], serde_json::json!(0));
}

/// Several reverts inside one deploy range are all counted.
///
/// Failure attribution counts the deploy once, but each revert is its own
/// incident, so an unrestored pair reports two — not one.
#[test]
fn multiple_reverts_are_separate_incidents() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    git(&root, &["tag", "v0.1.0"]);
    commit_at(&root, T0 + 100, "feat: a");
    git(&root, &["tag", "v0.1.1"]);
    commit_at(&root, T0 + 200, "revert(fixture): first");
    commit_at(&root, T0 + 300, "revert(fixture): second");

    let (_stdout, _stderr, snapshot) = dora_json(&root, T0 + 86_400, 1);
    assert_eq!(
        snapshot["deploys_failed"],
        serde_json::json!(1),
        "the deploy is counted once even with two reverts"
    );
    assert_eq!(snapshot["mttr_unrestored"], serde_json::json!(2));
    assert_eq!(
        snapshot["derivation"]["incidents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// A breach reports the comparator its rule actually uses.
///
/// Regression: every breach printed `>`, so the floor rule read as the
/// arithmetic contradiction `deploys_below_min 0 > 1`.
#[test]
fn floor_rules_report_the_less_than_comparator() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");

    let (_code, stdout, stderr) = run(harness(&root)
        .arg("dora")
        .arg("--now")
        .arg((T0 + 86_400).to_string()));
    assert!(
        stdout.contains("deploys_below_min 0 < 1"),
        "stdout: {stdout}"
    );
    assert!(
        stderr.contains("breach: deploys_below_min 0 < 1"),
        "stderr: {stderr}"
    );
    // Ceilings keep `>`.
    assert!(stderr.contains("breach: lead_p90 - > "), "stderr: {stderr}");
}
