//! Opt-in `GitHub` enrichment for `do-harness dora --source gh`.
//!
//! This module is additive only. The four hermetic metrics are always
//! git-derived; the network-sourced numbers land in a separate `github`
//! object so the two can never be silently conflated. It exists because git
//! cannot supply two things: whether a deploy tag actually shipped (a release
//! run per tag), and PR-open → merge lead time (squash-only history makes
//! `--first-parent` measure commit-to-deploy instead). Like the
//! `eval --agent-cmd` / `distill --from-strikes` opt-ins, it is never used in
//! CI, hooks, or a sensor argv.
//!
//! Any `gh` failure — missing binary, `401`, malformed JSON — is an error.
//! There is deliberately no git-only degradation: the snapshot's `source`
//! column would then claim network provenance it never had.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::json;

use super::{DoraSnapshot, git, percentile};

/// The workflow whose runs are reconciled against deploy tags.
const RELEASE_WORKFLOW: &str = "release.yml";

/// Maximum rows requested from `gh` for each query.
const GH_LIMIT: u32 = 100;

/// Seconds in one day.
const SECONDS_PER_DAY: i64 = 86_400;

/// One release-workflow run as reported by `gh run list`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRow {
    conclusion: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "headSha")]
    head_sha: String,
}

/// One merged pull request as reported by `gh pr list`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PrRow {
    number: u64,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
}

/// Enriches `snapshot` with release-run reconciliation and PR lead time.
///
/// Sets `snapshot.github` and flips `snapshot.source` to `gh`. The hermetic
/// fields are left untouched.
///
/// Run rows are filtered to `createdAt` inside the snapshot's window; PR rows
/// are filtered to `mergedAt` inside the same window, so a "30-day" metric
/// never mixes a year of merges into its sample.
///
/// # Errors
///
/// Returns an error when `gh` is missing, unauthenticated, or returns a
/// payload this module cannot parse, and when the recorded tag glob no longer
/// resolves the deploy tags.
pub fn enrich(root: &Path, snapshot: &mut DoraSnapshot) -> Result<()> {
    let tags = git::matching_tags(root, &snapshot.derivation.tag_glob)?;
    let (window_start, window_end) = (snapshot.window_start, snapshot.window_end);

    let runs = fetch_runs(root)?;
    let run_stats = RunStats::from_runs(&runs, window_start, window_end)?;
    let pr_lead = fetch_pr_lead(root, window_start, window_end)?;

    // Tag-vs-release reconciliation: a tag whose revision never produced a
    // successful release run did not actually ship.
    let mismatches: Vec<String> = tags
        .iter()
        .filter(|tag| {
            snapshot.deploy_tags.contains(&tag.name)
                && !run_stats
                    .successful_heads
                    .iter()
                    .any(|head| head == &tag.rev)
        })
        .map(|tag| tag.name.clone())
        .collect();

    snapshot.github = Some(json!({
        "workflow": RELEASE_WORKFLOW,
        "runs_total": run_stats.total,
        "runs_failed": run_stats.failed,
        "runs_by_head": run_stats.by_head,
        "deploy_head_mismatches": mismatches,
        "pr_samples": pr_lead.len(),
        "pr_lead_p50_seconds": percentile(&pr_lead, 50),
        "pr_lead_p90_seconds": percentile(&pr_lead, 90),
    }));
    snapshot.source = super::Source::Gh.label().to_string();
    Ok(())
}

/// Release-run facts for the measurement window, plus deployment history.
struct RunStats {
    /// Release runs created inside the window.
    total: i64,
    /// Windowed runs that concluded anything other than `success`.
    failed: i64,
    /// Heads of *any* successful run, windowed or not.
    ///
    /// Tag reconciliation must not be windowed: a deploy tag whose release run
    /// fired shortly before `window_start` would otherwise look as though it
    /// never shipped, and the reconciliation exists to catch a tag that really
    /// did not.
    successful_heads: Vec<String>,
    /// Windowed run conclusion per head.
    by_head: BTreeMap<String, String>,
}

impl RunStats {
    /// Folds release runs into windowed counters and deployment history.
    ///
    /// # Errors
    ///
    /// Returns an error when a run carries no parsable `createdAt`.
    fn from_runs(runs: &[RunRow], window_start: i64, window_end: i64) -> Result<Self> {
        let mut stats = Self {
            total: 0,
            failed: 0,
            successful_heads: Vec::new(),
            by_head: BTreeMap::new(),
        };
        for run in runs {
            let Some(created_at) = run.created_at.as_deref() else {
                bail!(
                    "gh run list returned a run without createdAt (head {})",
                    run.head_sha
                );
            };
            let created = parse_rfc3339_utc(created_at)
                .with_context(|| format!("unparsable gh run createdAt {created_at:?}"))?;
            let conclusion = run.conclusion.as_deref();
            if conclusion == Some("success") {
                stats.successful_heads.push(run.head_sha.clone());
            }
            if created < window_start || created > window_end {
                continue;
            }
            stats.total += 1;
            if conclusion.is_some_and(|value| value != "success") {
                stats.failed += 1;
            }
            // A head can have several runs. `gh run list` is newest-first, so a
            // later insert would let the *oldest* conclusion win; prefer
            // `success` explicitly to keep the map independent of run order.
            let label = conclusion.unwrap_or("pending");
            stats
                .by_head
                .entry(run.head_sha.clone())
                .and_modify(|existing| {
                    if label == "success" {
                        existing.clear();
                        existing.push_str(label);
                    }
                })
                .or_insert_with(|| label.to_string());
        }
        Ok(stats)
    }
}

/// Merged pull requests in the window, as PR-open → merge durations.
///
/// This is the only source that can measure PR-open → merge: squash-only
/// history makes `--first-parent` unable to.
///
/// # Errors
///
/// Returns an error when `gh` fails or a row carries no parsable timestamp.
fn fetch_pr_lead(root: &Path, window_start: i64, window_end: i64) -> Result<Vec<i64>> {
    let prs: Vec<PrRow> = gh_json(
        root,
        &[
            "pr",
            "list",
            "--state",
            "merged",
            "--limit",
            &GH_LIMIT.to_string(),
            "--json",
            "number,createdAt,mergedAt",
        ],
    )?;
    let mut lead = Vec::new();
    for pr in &prs {
        let (Some(created_at), Some(merged_at)) =
            (pr.created_at.as_deref(), pr.merged_at.as_deref())
        else {
            continue;
        };
        let created = parse_rfc3339_utc(created_at)
            .with_context(|| format!("unparsable gh pr createdAt for #{}", pr.number))?;
        let merged = parse_rfc3339_utc(merged_at)
            .with_context(|| format!("unparsable gh pr mergedAt for #{}", pr.number))?;
        if merged < window_start || merged > window_end {
            continue;
        }
        lead.push((merged - created).max(0));
    }
    lead.sort_unstable();
    Ok(lead)
}

/// Reads the release-workflow run list through `gh`.
///
/// # Errors
///
/// Returns an error when `gh` fails or emits a payload whose rows do not
/// match [`RunRow`].
fn fetch_runs(root: &Path) -> Result<Vec<RunRow>> {
    gh_json(
        root,
        &[
            "run",
            "list",
            "--workflow",
            RELEASE_WORKFLOW,
            "--limit",
            &GH_LIMIT.to_string(),
            "--json",
            "conclusion,createdAt,headSha",
        ],
    )
}

/// Runs `gh` with `args` and parses stdout as JSON.
///
/// # Errors
///
/// Returns an error when `gh` cannot be spawned, exits non-zero (missing
/// repository, unauthenticated, rate-limited), or emits a payload that does
/// not match the expected row shape.
fn gh_json<T: serde::de::DeserializeOwned>(root: &Path, args: &[&str]) -> Result<Vec<T>> {
    let output = std::process::Command::new("gh")
        .current_dir(root)
        .args(args)
        .output()
        .context("failed to run gh (is the GitHub CLI installed?)")?;
    if !output.status.success() {
        bail!(
            "gh {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("unexpected gh {} JSON", args.join(" ")))
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fff]Z` into Unix seconds.
///
/// Deliberately narrow: the `gh` JSON fields this module reads are UTC
/// `Z`-suffixed timestamps, and accepting a wider grammar (offsets, missing
/// seconds) would silently accept a value it mis-parses. Fractional seconds
/// are ignored because every metric here is whole seconds.
///
/// Every component is range-checked with a *lower* bound as well as an upper
/// one. `parse::<i64>()` accepts a leading `-`, so a `>`-only bound let
/// `12:-30:00` through and subtracted time. The day is checked against the
/// month's real length rather than `1..=31`, because Hinnant's civil-day
/// algorithm rolls Feb 30 into March instead of rejecting it.
#[must_use]
fn parse_rfc3339_utc(value: &str) -> Option<i64> {
    let (date, rest) = value.split_once('T')?;
    let time = rest.strip_suffix('Z')?;
    let time = time.split('.').next()?;

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
    {
        return None;
    }

    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next()?.parse().ok()?;
    if time_parts.next().is_some()
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }

    let days = days_from_civil(year, month, day);
    Some(days * SECONDS_PER_DAY + hour * 3_600 + minute * 60 + second)
}

/// Length of `month` in `year` (proleptic Gregorian leap rule).
///
/// `month` is validated to `1..=12` before this is called, so the 30-day arms
/// pair with the 31-day ones rather than needing a fallback.
fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 31,
    }
}

/// Whether `year` is a leap year in the proleptic Gregorian calendar.
fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Hinnant's
/// `days_from_civil`), exact for every representable input.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_shift = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_shift + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parses_gh_style_timestamps_exactly() {
        assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_rfc3339_utc("2026-09-13T00:00:00Z"),
            Some(1_789_257_600)
        );
        assert_eq!(
            parse_rfc3339_utc("2026-09-13T04:15:22Z"),
            Some(1_789_257_600 + 4 * 3_600 + 15 * 60 + 22)
        );
        assert_eq!(
            parse_rfc3339_utc("2026-09-13T04:15:22.123Z"),
            Some(1_789_257_600 + 4 * 3_600 + 15 * 60 + 22)
        );
    }

    #[test]
    fn rejects_timestamps_it_would_mis_parse() {
        assert_eq!(parse_rfc3339_utc("2026-09-13T04:15:22+02:00"), None);
        assert_eq!(parse_rfc3339_utc("2026-09-13"), None);
        assert_eq!(parse_rfc3339_utc("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_rfc3339_utc("2026-09-13T25:00:00Z"), None);
        assert_eq!(parse_rfc3339_utc("not a date"), None);
    }

    #[test]
    fn civil_day_arithmetic_matches_known_epochs() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }
}
