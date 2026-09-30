//! `metrics pr`: the PR loop's own measures.
//!
//! `metrics` reports what the sensors did; this reports what the *loop* cost.
//! Every measure is derived from `gh` reads (pull requests, their commits,
//! check runs, workflow runs, and comments) with the rules documented in
//! `docs/cli.md`, and a snapshot is cached under the repository git directory so
//! repeated runs in a session do not re-hit the API.
//!
//! Measures and their rules:
//! - **time-to-green**: the head commit's check runs are all `completed` with a
//!   non-failing conclusion; the measure is the last completion timestamp minus
//!   the earliest commit timestamp on the PR ("first push" — the commit
//!   timestamp is the push-time proxy, since the timeline API would cost one
//!   paginated call per PR).
//! - **pushes**: commits on the PR head; **pushes per green** divides the sum by
//!   the number of green pull requests.
//! - **waivers by class**: comments whose text names a patch-coverage waiver
//!   class (`macro-field`, `guarded-arm`, `feature-gated` — the same labels
//!   `pr waivers` prints).
//! - **comment classes**: bot-authored comments are informational; a
//!   human-authored comment is actionable when it carries an action marker
//!   (listed in the docs), informational otherwise.
//! - **cancelled runs** and **reruns**: workflow runs for the head commit,
//!   counting `conclusion == "cancelled"` and `run_attempt - 1` respectively.

mod cache;
mod classify;
mod report;
mod window;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::CliError;
use crate::pr::gh::{CheckRun, Comment};
use crate::pr::loop_fetch::{self, Commit, WorkflowRun};
use crate::report::Format;

pub(crate) use cache::{cached, store};
pub(crate) use report::render_text;
pub(crate) use window::{now_epoch, parse_window};

/// Snapshot schema version; a bump invalidates cached entries.
pub const SCHEMA_VERSION: u32 = 1;

/// Conclusions that leave a check green.
const GREEN_CONCLUSIONS: [&str; 3] = ["success", "neutral", "skipped"];

/// Options for one `metrics pr` run.
#[derive(Debug, Clone)]
pub struct PrOpts {
    /// Repository as `OWNER/NAME`.
    pub repo: String,
    /// Window as written (`30d`) or a date (`YYYY-MM-DD`).
    pub since: String,
    /// Maximum pull requests measured, newest first.
    pub limit: usize,
    /// Ignore the cached snapshot.
    pub recompute: bool,
}

/// One pull request's loop measures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrRow {
    /// Pull request number.
    pub number: u64,
    /// `open` or `closed` (merged PRs are closed with `merged_at` set).
    pub state: String,
    /// Head commit the measures describe.
    pub head_sha: String,
    /// Commits on the head branch.
    pub pushes: u32,
    /// Earliest commit timestamp (the first-push proxy).
    pub first_push_at: Option<i64>,
    /// Last completion timestamp of a fully green head.
    pub green_at: Option<i64>,
    /// `green_at - first_push_at`, when both exist.
    pub time_to_green_secs: Option<u64>,
    /// Waiver comments by class.
    pub waivers: BTreeMap<String, usize>,
    /// Human comments carrying an action marker.
    pub actionable_comments: usize,
    /// Bot comments and human comments without an action marker.
    pub informational_comments: usize,
    /// Workflow runs for the head commit.
    pub runs: usize,
    /// Runs that concluded `cancelled`.
    pub cancelled_runs: usize,
    /// Summed `run_attempt - 1` over the head's runs.
    pub reruns: u32,
}

impl PrRow {
    /// Whether every measure for the head came out green in time.
    #[must_use]
    pub fn green(&self) -> bool {
        self.green_at.is_some()
    }
}

/// Repository totals over the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    /// Pull requests measured.
    pub prs: usize,
    /// Pull requests with a fully green head.
    pub green_prs: usize,
    /// Commits summed over the measured pull requests.
    pub pushes: u32,
    /// `pushes / green_prs`, when any pull request went green.
    pub pushes_per_green: Option<f64>,
    /// Median time to green over green pull requests (nearest rank).
    pub time_to_green_p50_secs: Option<u64>,
    /// 95th percentile time to green (nearest rank).
    pub time_to_green_p95_secs: Option<u64>,
    /// Waiver comments by class.
    pub waivers: BTreeMap<String, usize>,
    /// Actionable human comments.
    pub actionable_comments: usize,
    /// Informational comments.
    pub informational_comments: usize,
    /// Workflow runs summed over the heads.
    pub runs: usize,
    /// Cancelled runs summed over the heads.
    pub cancelled_runs: usize,
    /// `cancelled_runs / runs`, when any run was observed.
    pub cancelled_rate: Option<f64>,
    /// Summed re-runs.
    pub reruns: u32,
}

/// A measured window of one repository's PR loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrLoopSnapshot {
    /// Schema version of the snapshot.
    pub schema_version: u32,
    /// Repository as `OWNER/NAME`.
    pub repo: String,
    /// Window as written.
    pub since: String,
    /// Pull-request limit of the run.
    pub limit: usize,
    /// When the snapshot was fetched (Unix seconds).
    pub fetched_at: i64,
    /// Whether the snapshot came from the local cache.
    #[serde(default)]
    pub cached: bool,
    /// Per-pull-request measures, newest first.
    pub prs: Vec<PrRow>,
    /// Aggregates over `prs`.
    pub totals: Totals,
    /// Non-fatal diagnostics (a pull request whose documents failed to load).
    pub warnings: Vec<String>,
}

/// Measures one pull request from its raw documents.
#[must_use]
pub fn measure(
    number: u64,
    state: &str,
    head: &str,
    commits: &[Commit],
    check_runs: &[CheckRun],
    runs: &[WorkflowRun],
    comments: &[Comment],
) -> PrRow {
    let first_push_at = commits.iter().filter_map(timestamp_of).min();
    let green_at = green_at(check_runs);
    let time_to_green_secs = match (first_push_at, green_at) {
        (Some(first), Some(green)) if green >= first => {
            Some(u64::try_from(green - first).unwrap_or(0))
        }
        _ => None,
    };
    let mut waivers: BTreeMap<String, usize> = BTreeMap::new();
    let mut actionable_comments = 0;
    let mut informational_comments = 0;
    for comment in comments {
        for (class, count) in classify::waiver_classes(&comment.body) {
            *waivers.entry(class.to_owned()).or_default() += count;
        }
        if classify::is_actionable(comment) {
            actionable_comments += 1;
        } else {
            informational_comments += 1;
        }
    }
    let cancelled_runs = runs
        .iter()
        .filter(|run| run.conclusion.as_deref() == Some("cancelled"))
        .count();
    let reruns = runs
        .iter()
        .map(|run| run.run_attempt.unwrap_or(1).saturating_sub(1))
        .sum();
    PrRow {
        number,
        state: state.to_owned(),
        head_sha: head.to_owned(),
        pushes: u32::try_from(commits.len()).unwrap_or(u32::MAX),
        first_push_at,
        green_at,
        time_to_green_secs,
        waivers,
        actionable_comments,
        informational_comments,
        runs: runs.len(),
        cancelled_runs,
        reruns,
    }
}

/// Aggregates rows into repository totals.
#[must_use]
pub fn totals(rows: &[PrRow]) -> Totals {
    let mut sorted: Vec<u64> = rows
        .iter()
        .filter_map(|row| row.time_to_green_secs)
        .collect();
    sorted.sort_unstable();
    let green_prs = rows.iter().filter(|row| row.green()).count();
    let pushes: u32 = rows.iter().map(|row| row.pushes).sum();
    let mut waivers: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        for (class, count) in &row.waivers {
            *waivers.entry(class.clone()).or_default() += count;
        }
    }
    let runs: usize = rows.iter().map(|row| row.runs).sum();
    let cancelled_runs: usize = rows.iter().map(|row| row.cancelled_runs).sum();
    Totals {
        prs: rows.len(),
        green_prs,
        pushes,
        pushes_per_green: (green_prs > 0)
            .then(|| f64::from(pushes) / f64::from(u32::try_from(green_prs).unwrap_or(u32::MAX))),
        time_to_green_p50_secs: percentile(&sorted, 50),
        time_to_green_p95_secs: percentile(&sorted, 95),
        waivers,
        actionable_comments: rows.iter().map(|row| row.actionable_comments).sum(),
        informational_comments: rows.iter().map(|row| row.informational_comments).sum(),
        runs,
        cancelled_runs,
        cancelled_rate: (runs > 0).then(|| ratio(cancelled_runs, runs)),
        reruns: rows.iter().map(|row| row.reruns).sum(),
    }
}

/// `part / total` as a fraction, in `f64` (counts are small; the conversion
/// saturates rather than truncating silently).
fn ratio(part: usize, total: usize) -> f64 {
    let part = f64::from(u32::try_from(part).unwrap_or(u32::MAX));
    let total = f64::from(u32::try_from(total).unwrap_or(u32::MAX).max(1));
    part / total
}

/// Nearest-rank percentile over an ascending slice.
fn percentile(sorted: &[u64], percent: usize) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

/// Commit timestamp, preferring the committer stamp.
fn timestamp_of(commit: &Commit) -> Option<i64> {
    let stamp = commit
        .commit
        .committer
        .as_ref()
        .or(commit.commit.author.as_ref())?;
    stamp
        .date
        .as_deref()
        .and_then(crate::dora::gh::parse_rfc3339_utc)
}

/// The last completion timestamp of a fully green head, when it is green.
fn green_at(check_runs: &[CheckRun]) -> Option<i64> {
    if check_runs.is_empty() {
        return None;
    }
    let mut latest: Option<i64> = None;
    for run in check_runs {
        if run.status != "completed" {
            return None;
        }
        let conclusion = run.conclusion.as_deref()?;
        if !GREEN_CONCLUSIONS.contains(&conclusion) {
            return None;
        }
        let at = run
            .completed_at
            .as_deref()
            .and_then(crate::dora::gh::parse_rfc3339_utc)?;
        latest = Some(latest.map_or(at, |current: i64| current.max(at)));
    }
    latest
}

/// Runs `metrics pr` for one repository.
///
/// # Errors
///
/// Returns [`CliError::Usage`] for an unreadable window, and
/// [`CliError::Verify`] when a `gh` read fails part-way through.
pub fn run(root: &Path, opts: &PrOpts, format: Format) -> Result<(), CliError> {
    let since_epoch = parse_window(&opts.since).map_err(CliError::Usage)?;
    let mut snapshot = match cached(root, &opts.repo, &opts.since, opts.limit) {
        Some(mut snapshot) if !opts.recompute => {
            snapshot.cached = true;
            snapshot
        }
        _ => {
            let snapshot = fetch(root, opts, since_epoch).map_err(CliError::Verify)?;
            if let Err(err) = store(root, &snapshot) {
                // A cache that cannot be written never fails the report.
                eprintln!("warning: {err:#}");
            }
            snapshot
        }
    };
    snapshot.schema_version = SCHEMA_VERSION;
    if matches!(format, Format::Json) {
        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|err| CliError::Verify(anyhow::anyhow!("cannot serialize report: {err}")))?;
        println!("{json}");
    } else {
        print!("{}", render_text(&snapshot));
    }
    Ok(())
}

/// Fetches and measures the window.
fn fetch(root: &Path, opts: &PrOpts, since_epoch: i64) -> Result<PrLoopSnapshot> {
    let prs = loop_fetch::pull_requests(root, &opts.repo, since_epoch, opts.limit)
        .with_context(|| format!("cannot list pull requests of {}", opts.repo))?;
    let mut rows = Vec::with_capacity(prs.len());
    let mut warnings = Vec::new();
    for pr in &prs {
        let Some(head) = pr.head_sha() else {
            warnings.push(format!("pr {}: no head commit in the payload", pr.number));
            continue;
        };
        let documents = || -> Result<_> {
            Ok((
                loop_fetch::commits(root, &opts.repo, pr.number)?,
                loop_fetch::check_runs(root, &opts.repo, head)?,
                loop_fetch::runs(root, &opts.repo, head)?,
                loop_fetch::comments(root, &opts.repo, pr.number)?,
            ))
        };
        match documents() {
            Ok((commits, check_runs, runs, comments)) => rows.push(measure(
                pr.number,
                &pr.state,
                head,
                &commits,
                &check_runs,
                &runs,
                &comments,
            )),
            // One unreadable pull request must not void the snapshot; the
            // warning names it and the totals say how many were measured.
            Err(err) => warnings.push(format!("pr {}: {err:#}", pr.number)),
        }
    }
    let totals = totals(&rows);
    Ok(PrLoopSnapshot {
        schema_version: SCHEMA_VERSION,
        repo: opts.repo.clone(),
        since: opts.since.clone(),
        limit: opts.limit,
        fetched_at: now_epoch(),
        cached: false,
        prs: rows,
        totals,
        warnings,
    })
}

#[cfg(test)]
#[path = "pr_loop_tests.rs"]
mod pr_loop_tests;
