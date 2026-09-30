//! Unit tests for the PR-loop measures.
//!
//! The fixtures under `tests/fixtures/pr-metrics/` carry measured values: the
//! commits, check runs, workflow runs, and comments of pull requests #268 and
//! #269 of `d-o-hub/do-harness` were read from the API on 2026-09-30, so the
//! expected time-to-green values are real arithmetic over real timestamps. The
//! third pull request is a documented control for the paths the two merged PRs
//! cannot exercise (not green, a cancelled run, a re-run).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use crate::pr::gh::{CheckRun, Comment};
use crate::pr::loop_fetch::{Commit, WorkflowRun};

use super::*;

/// Fixture root.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pr-metrics")
}

/// Reads a fixture file, treating a missing one as empty (comment lists).
fn read(name: &str) -> String {
    std::fs::read_to_string(fixtures().join(name)).unwrap_or_default()
}

/// Parses a newline-delimited fixture.
fn lines<T: serde::de::DeserializeOwned>(text: &str) -> Vec<T> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Measures one fixture pull request from its committed documents.
fn measure_fixture(pr: u64, head: &str, state: &str) -> PrRow {
    let commits: Vec<Commit> = lines(&read(&format!("pr{pr}-commits.json")));
    let checks: Vec<CheckRun> = lines(&read(&format!("pr{pr}-check-runs.json")));
    let runs: Vec<WorkflowRun> = lines(&read(&format!("pr{pr}-runs.json")));
    let comments: Vec<Comment> = lines(&read(&format!("pr{pr}-comments.json")));
    measure(pr, state, head, &commits, &checks, &runs, &comments)
}

#[test]
fn measured_pull_requests_reproduce_the_fetched_numbers() {
    // #269: first push 15:31:54Z, last green completion 15:58:39Z.
    let row = measure_fixture(269, "c8188dca57ec51861fbd8ddf6a7bc7f18a8b0623", "closed");
    assert_eq!(row.pushes, 3);
    assert_eq!(row.green_at, Some(1_790_783_919));
    assert_eq!(row.time_to_green_secs, Some(26 * 60 + 45));
    assert_eq!(row.runs, 2);
    assert_eq!(row.cancelled_runs, 0);
    assert_eq!(row.reruns, 0);

    // #268: first push 12:50:16Z, last green completion 14:29:36Z.
    let row = measure_fixture(268, "14dc0b76a825655d4d7082347ce85507f778fd8a", "closed");
    assert_eq!(row.pushes, 3);
    assert_eq!(row.time_to_green_secs, Some(5_960));
}

#[test]
fn a_pull_request_that_is_not_green_has_no_time_to_green() {
    // The control's head carries an in-progress check run.
    let row = measure_fixture(270, "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111", "open");
    assert!(!row.green());
    assert_eq!(row.green_at, None);
    assert_eq!(row.time_to_green_secs, None);
    assert_eq!(row.first_push_at, Some(1_790_784_120));
}

#[test]
fn cancelled_runs_and_reruns_are_counted_from_the_head_runs() {
    let row = measure_fixture(270, "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111", "open");
    assert_eq!(row.runs, 2);
    assert_eq!(row.cancelled_runs, 1);
    assert_eq!(row.reruns, 1);
}

#[test]
fn waiver_comments_are_counted_by_class() {
    // The two waiver bodies are the real ones from the PRs that motivated the
    // measure: one names the unreachable defensive arm and the tracing field
    // expressions, the other the feature-gated paths.
    let row = measure_fixture(270, "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111", "open");
    assert_eq!(row.waivers.get("macro-field"), Some(&2));
    assert_eq!(row.waivers.get("guarded-arm"), Some(&1));
    assert_eq!(row.waivers.get("feature-gated"), Some(&1));

    // #269's review comment names the `guarded-arm` class, so the label alone
    // counts as a waiver of that class.
    let row = measure_fixture(269, "c8188dca57ec51861fbd8ddf6a7bc7f18a8b0623", "closed");
    assert_eq!(row.waivers.get("guarded-arm"), Some(&1));
    assert_eq!(row.waivers.len(), 1);
}

#[test]
fn comment_classes_split_on_author_and_action_markers() {
    let control = measure_fixture(270, "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111", "open");
    // A bot comment and two waiver comments are informational; the "Please
    // rerun …" comment is actionable.
    assert_eq!(control.informational_comments, 3);
    assert_eq!(control.actionable_comments, 1);

    let merged = measure_fixture(269, "c8188dca57ec51861fbd8ddf6a7bc7f18a8b0623", "closed");
    assert_eq!(merged.informational_comments, 1);
    assert_eq!(merged.actionable_comments, 1);

    let none = measure_fixture(268, "14dc0b76a825655d4d7082347ce85507f778fd8a", "closed");
    assert_eq!(none.informational_comments, 0);
    assert_eq!(none.actionable_comments, 0);
}

#[test]
fn totals_aggregate_percentiles_and_rates() {
    let rows = vec![
        measure_fixture(270, "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111", "open"),
        measure_fixture(269, "c8188dca57ec51861fbd8ddf6a7bc7f18a8b0623", "closed"),
        measure_fixture(268, "14dc0b76a825655d4d7082347ce85507f778fd8a", "closed"),
    ];
    let totals = totals(&rows);
    assert_eq!(totals.prs, 3);
    assert_eq!(totals.green_prs, 2);
    assert_eq!(totals.pushes, 8);
    assert_eq!(totals.pushes_per_green, Some(4.0));
    assert_eq!(totals.time_to_green_p50_secs, Some(1_605));
    assert_eq!(totals.time_to_green_p95_secs, Some(5_960));
    assert_eq!(totals.waivers.get("macro-field"), Some(&2));
    assert_eq!(totals.waivers.get("guarded-arm"), Some(&2));
    assert_eq!(totals.waivers.get("feature-gated"), Some(&1));
    assert_eq!(totals.actionable_comments, 2);
    assert_eq!(totals.informational_comments, 4);
    assert_eq!(totals.runs, 6);
    assert_eq!(totals.cancelled_runs, 1);
    assert_eq!(totals.reruns, 1);
    let rate = totals.cancelled_rate.unwrap();
    assert!((rate - 1.0 / 6.0).abs() < 1e-9, "{rate}");
}

#[test]
fn an_empty_window_has_no_ratios() {
    let totals = totals(&[]);
    assert_eq!(totals.prs, 0);
    assert_eq!(totals.pushes_per_green, None);
    assert_eq!(totals.cancelled_rate, None);
    assert_eq!(totals.time_to_green_p50_secs, None);
}

#[test]
fn windows_parse_durations_and_dates() {
    let now = now_epoch();
    let thirty = parse_window("30d").unwrap();
    assert!((now - thirty - 30 * 86_400).abs() <= 2, "{thirty}");
    let twelve = parse_window("12h").unwrap();
    assert!((now - twelve - 12 * 3_600).abs() <= 2, "{twelve}");
    let two_weeks = parse_window("2w").unwrap();
    assert!((now - two_weeks - 14 * 86_400).abs() <= 2, "{two_weeks}");
    assert_eq!(
        parse_window("2026-09-30").unwrap(),
        crate::dora::gh::parse_rfc3339_utc("2026-09-30T00:00:00Z").unwrap()
    );
    for bad in ["", "last month", "30", "2026-13-01"] {
        assert!(parse_window(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_report_renders_real_durations_and_totals() {
    let rows = vec![
        measure_fixture(270, "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111", "open"),
        measure_fixture(269, "c8188dca57ec51861fbd8ddf6a7bc7f18a8b0623", "closed"),
    ];
    let snapshot = PrLoopSnapshot {
        schema_version: SCHEMA_VERSION,
        repo: "d-o-hub/do-harness".to_owned(),
        since: "30d".to_owned(),
        limit: 30,
        fetched_at: 1_790_791_200,
        cached: false,
        totals: totals(&rows),
        prs: rows,
        warnings: Vec::new(),
    };
    let text = render_text(&snapshot);
    assert!(
        text.starts_with(
            "metrics pr: d-o-hub/do-harness since 30d (2 PR(s), 1 green) — fetched 2026-09-30T"
        ),
        "{text}"
    );
    assert!(text.contains("#269"), "{text}");
    assert!(text.contains("26m45s"), "{text}");
    assert!(
        text.contains("time-to-green: p50 26m45s, p95 26m45s"),
        "{text}"
    );
    assert!(
        text.contains("waivers: feature-gated 1, guarded-arm 2, macro-field 2"),
        "{text}"
    );
    assert!(text.contains("runs: 4, cancelled 1 (25.0%)"), "{text}");
}

#[test]
fn the_snapshot_cache_round_trips_and_expires() {
    let dir = tempfile::tempdir().unwrap();
    let status = crate::changes::git_command(dir.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    assert!(status.success());
    let opts = PrOpts {
        repo: "d-o-hub/do-harness".to_owned(),
        since: "30d".to_owned(),
        limit: 30,
        recompute: false,
    };
    let snapshot = PrLoopSnapshot {
        schema_version: SCHEMA_VERSION,
        repo: opts.repo.clone(),
        since: opts.since.clone(),
        limit: opts.limit,
        fetched_at: now_epoch(),
        cached: false,
        prs: Vec::new(),
        totals: totals(&[]),
        warnings: Vec::new(),
    };
    store(dir.path(), &snapshot).unwrap();
    let loaded =
        cached(dir.path(), &snapshot.repo, &snapshot.since, snapshot.limit).expect("fresh entry");
    assert_eq!(loaded.repo, "d-o-hub/do-harness");

    // An entry older than the TTL is ignored rather than served stale.
    let mut expired = snapshot.clone();
    expired.fetched_at = now_epoch() - 2 * 600;
    store(dir.path(), &expired).unwrap();
    assert!(cached(dir.path(), &expired.repo, &expired.since, expired.limit).is_none());
}
