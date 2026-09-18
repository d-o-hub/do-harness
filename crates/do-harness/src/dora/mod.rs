//! Deterministic DORA deployment metrics derived from git history.
//!
//! The four DORA metrics — deployment frequency, lead time for changes,
//! change failure rate, and time to restore service — are **derived**, never
//! judged: every number comes from persisted git history plus the pinned
//! policy in `plans/dora.json`, so the same input yields byte-identical
//! output. Each snapshot carries its own derivation manifest (the resolved
//! window, the ref ranges scanned, the revert predicate, the percentile
//! method, and the incidents), so a number can never be read without its
//! provenance. Everything measured is an integer; the change failure rate is
//! kept as an exact `deploys_failed/deploy_count` pair rather than a rounded
//! float.

pub mod command;
pub mod gh;
pub mod git;
pub mod policy;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub use policy::{Breach, DoraPolicy};

/// Where the snapshot's numbers came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Source {
    /// Git history only: hermetic, offline, clock-injected.
    #[default]
    Git,
    /// Git history plus opt-in `gh` enrichment (network; never in CI).
    Gh,
}

impl Source {
    /// Stable lowercase label persisted in the snapshot.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Gh => "gh",
        }
    }
}

/// One incident: a revert and the deploy that restored service, when any did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Incident {
    /// Revert commit SHA.
    pub revert_sha: String,
    /// Revert commit timestamp.
    pub revert_ts: i64,
    /// Tag that restored service, strictly after the revert.
    pub restored_by: Option<String>,
    /// Timestamp of the restoring tag.
    pub restore_ts: Option<i64>,
}

/// One measured range, as recorded in the derivation manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeRecord {
    /// Short tag name.
    pub tag: String,
    /// Previous tag, when one exists.
    pub previous: Option<String>,
    /// Git revision range that supplied this deploy's lead-time samples.
    pub range: String,
    /// Git revision range that attributed failures to this deploy
    /// (`T..next`, or `T..HEAD` for the newest deploy).
    pub failure_range: String,
    /// First-parent commit count of `range`.
    pub commits: i64,
    /// Commit timestamp of the deployed revision.
    pub deploy_ts: i64,
}

/// The recorded provenance of one measurement.
///
/// This is what makes a number auditable and its disagreement reproducible:
/// it pins the definition (policy digest, percentile method, revert
/// predicate), the scope (window, tag glob, merge strategy), the exact ref
/// ranges scanned, and the incidents that drove failure attribution. It is
/// also emitted verbatim as the `COVERAGE:` marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Derivation {
    /// Tag glob that selected the deploys.
    pub tag_glob: String,
    /// Recorded merge strategy, which is why lead time measures
    /// commit-to-deploy on squash-only history.
    pub merge_strategy: String,
    /// Percentile estimator used.
    pub percentile_method: String,
    /// Revert predicate applied.
    pub revert_pattern: String,
    /// Window actually resolved for this run (CLI override or policy).
    pub window_days: i64,
    /// Hex SHA-256 of the raw policy bytes.
    pub policy_digest: String,
    /// Commits timestamped after their deploy (clock-skew evidence).
    pub clock_skew_commits: i64,
    /// Every range scanned, in deploy order.
    pub ranges: Vec<RangeRecord>,
    /// Every incident observed in the scanned ranges.
    pub incidents: Vec<Incident>,
}

/// The derived snapshot: the JSON contract, the printed report, and the
/// persisted row shape. Integers only, so output is byte-identical across
/// platforms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoraSnapshot {
    /// Window length in days used for this run.
    pub window_days: i64,
    /// Inclusive start of the resolved window.
    pub window_start: i64,
    /// End of the resolved window.
    pub window_end: i64,
    /// `git` or `gh`.
    pub source: String,
    /// `HEAD` revision at measurement time.
    pub source_rev: String,
    /// Deploys measured in the window.
    pub deploy_count: i64,
    /// Deploys whose own failure range contains at least one revert.
    pub deploys_failed: i64,
    /// Tags of the measured deploys, in deploy order.
    pub deploy_tags: Vec<String>,
    /// Lead-time samples behind the percentiles.
    pub lead_samples: i64,
    /// Median lead time in seconds, `None` with no samples.
    pub lead_p50_seconds: Option<i64>,
    /// 90th percentile lead time in seconds, `None` with no samples.
    pub lead_p90_seconds: Option<i64>,
    /// Median restore time over *restored* incidents, `None` when none.
    pub mttr_seconds: Option<i64>,
    /// Incidents with a restoring deploy.
    pub mttr_restored: i64,
    /// Incidents still unrestored; counted explicitly, because a rate that
    /// omits them is survivorship bias rather than a measurement.
    pub mttr_unrestored: i64,
    /// Threshold breaches, in rule order.
    pub breaches: Vec<Breach>,
    /// `sha256:`-prefixed digest of the pinned policy.
    pub policy_fingerprint: String,
    /// Full derivation manifest.
    pub derivation: Derivation,
    /// Opt-in `gh` enrichment payload; absent for `--source git`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<serde_json::Value>,
}

/// Seconds in one day.
const SECONDS_PER_DAY: i64 = 86_400;

/// Resolves the measurement clock: the explicit `--now` wins, otherwise the
/// system clock.
#[must_use]
pub fn resolve_now(explicit: Option<i64>) -> i64 {
    explicit.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |delta| {
                i64::try_from(delta.as_secs()).unwrap_or(i64::MAX)
            })
    })
}

/// Nearest-rank percentile over an ascending-sorted slice.
///
/// Rank is `ceil(pct * n / 100)` computed in integers, then read 1-based.
/// Only `percentile_method = "nearest-rank"` is implemented, and
/// [`DoraPolicy::validate`] rejects any other value, so the estimator and its
/// documented name cannot drift apart.
#[must_use]
pub fn percentile(sorted: &[i64], pct: u32) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let n = u64::try_from(sorted.len()).ok()?;
    let rank = (u64::from(pct) * n).div_ceil(100).max(1);
    let index = usize::try_from(rank - 1).ok()?;
    sorted.get(index).copied()
}

/// Derives a snapshot from git history.
///
/// Order of operations: resolve tags → filter to the window → per-deploy
/// lead-time ranges and samples → failure attribution → incidents →
/// sort the samples once → percentiles → evaluate the policy thresholds.
///
/// The window selects **which deploys** are measured, never which commits
/// contribute samples: a deploy inside the window drags in its whole range, so
/// its oldest commits may predate `window_start`. Applying the window to
/// individual commit timestamps would truncate lead time for exactly the
/// slowest deploys, which is the metric's whole signal.
///
/// # Errors
///
/// Returns an error when the repository is not a work tree, `HEAD` cannot be
/// resolved, or any git command fails (a shallow clone whose ranges cannot be
/// resolved must never read as zero).
pub fn collect(root: &Path, policy: &DoraPolicy, now: i64, source: Source) -> Result<DoraSnapshot> {
    if !git::is_work_tree(root) {
        anyhow::bail!("not inside a git working tree; dora derives from git history");
    }
    let source_rev = git::head_rev(root)?;
    let tags = git::matching_tags(root, &policy.tag_glob)?;

    let window_end = now;
    let window_start = now - policy.window_days * SECONDS_PER_DAY;

    let scan = scan_deploys(root, policy, &tags, window_start, window_end)?;

    let mut restored: Vec<i64> = scan
        .incidents
        .iter()
        .filter_map(|incident| incident.restore_ts.map(|ts| ts - incident.revert_ts))
        .collect();
    restored.sort_unstable();

    let mut samples = scan.samples;
    samples.sort_unstable();

    let policy_digest = policy::digest(root);
    let mut snapshot = DoraSnapshot {
        window_days: policy.window_days,
        window_start,
        window_end,
        source: source.label().to_string(),
        source_rev,
        deploy_count: i64::try_from(scan.ranges.len()).context("deploy count overflow")?,
        deploys_failed: scan.deploys_failed,
        deploy_tags: scan
            .ranges
            .iter()
            .map(|record| record.tag.clone())
            .collect(),
        lead_samples: i64::try_from(samples.len()).context("sample count overflow")?,
        lead_p50_seconds: percentile(&samples, 50),
        lead_p90_seconds: percentile(&samples, 90),
        mttr_seconds: percentile(&restored, 50),
        mttr_restored: i64::try_from(restored.len()).context("restore count overflow")?,
        mttr_unrestored: i64::try_from(
            scan.incidents
                .iter()
                .filter(|incident| incident.restore_ts.is_none())
                .count(),
        )
        .context("incident count overflow")?,
        breaches: Vec::new(),
        policy_fingerprint: format!("sha256:{policy_digest}"),
        derivation: Derivation {
            tag_glob: policy.tag_glob.clone(),
            merge_strategy: policy.merge_strategy.clone(),
            percentile_method: policy.percentile_method.clone(),
            revert_pattern: policy.revert_pattern.clone(),
            window_days: policy.window_days,
            policy_digest,
            clock_skew_commits: scan.clock_skew_commits,
            ranges: scan.ranges,
            incidents: scan.incidents,
        },
        github: None,
    };
    snapshot.breaches = policy::breaches(&snapshot, policy);
    Ok(snapshot)
}

/// What one pass over the in-window deploys produced.
struct DeployScan {
    ranges: Vec<RangeRecord>,
    incidents: Vec<Incident>,
    samples: Vec<i64>,
    clock_skew_commits: i64,
    deploys_failed: i64,
}

/// Walks the in-window deploys once, collecting ranges, samples, and
/// incidents.
///
/// # Errors
///
/// Returns an error when any range cannot be resolved.
fn scan_deploys(
    root: &Path,
    policy: &DoraPolicy,
    tags: &[git::Tag],
    window_start: i64,
    window_end: i64,
) -> Result<DeployScan> {
    let mut scan = DeployScan {
        ranges: Vec::new(),
        incidents: Vec::new(),
        samples: Vec::new(),
        clock_skew_commits: 0,
        deploys_failed: 0,
    };

    for (index, tag) in tags.iter().enumerate() {
        if tag.deploy_ts < window_start || tag.deploy_ts > window_end {
            continue;
        }
        // The predecessor always comes from the FULL sorted tag list, so a
        // window boundary can never change a range.
        let previous = index
            .checked_sub(1)
            .and_then(|prev| tags.get(prev))
            .map(|tag| tag.name.clone());
        let range = previous
            .as_ref()
            .map_or_else(|| tag.name.clone(), |prev| format!("{prev}..{}", tag.name));
        let failure_range = tags.get(index + 1).map_or_else(
            || format!("{}..HEAD", tag.name),
            |next| format!("{}..{}", tag.name, next.name),
        );

        let lead = git::lead_scan(root, &range, tag.deploy_ts, &policy.bot_allowlist)?;
        scan.clock_skew_commits += lead.clock_skew_commits;
        scan.samples.extend(lead.samples);

        let revert_rows = git::reverts(root, &failure_range, &policy.revert_pattern)?;
        if !revert_rows.is_empty() {
            scan.deploys_failed += 1;
        }
        for (revert_sha, revert_ts) in revert_rows {
            scan.incidents
                .push(incident_from(tags, revert_sha, revert_ts));
        }

        scan.ranges.push(RangeRecord {
            tag: tag.name.clone(),
            previous,
            range: range.clone(),
            failure_range,
            commits: lead.commits,
            deploy_ts: tag.deploy_ts,
        });
    }

    scan.incidents.sort_by(|left, right| {
        (left.revert_ts, &left.revert_sha).cmp(&(right.revert_ts, &right.revert_sha))
    });
    scan.incidents
        .dedup_by(|left, right| left.revert_sha == right.revert_sha);
    Ok(scan)
}

/// Attributes one revert to its restoring deploy, when a tag follows it.
///
/// Unrestored incidents are kept with `restored_by`/`restore_ts` as `None`:
/// dropping them would make the restore metric survivorship-biased.
fn incident_from(tags: &[git::Tag], revert_sha: String, revert_ts: i64) -> Incident {
    let restoring = tags.iter().find(|tag| tag.deploy_ts > revert_ts);
    Incident {
        revert_sha,
        revert_ts,
        restored_by: restoring.map(|tag| tag.name.clone()),
        restore_ts: restoring.map(|tag| tag.deploy_ts),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn percentile_is_nearest_rank() {
        let samples: Vec<i64> = (0..165).collect();
        assert_eq!(percentile(&samples, 50), Some(82));
        assert_eq!(percentile(&samples, 90), Some(148));
        assert_eq!(percentile(&[], 50), None);
        assert_eq!(percentile(&[7], 50), Some(7));
        assert_eq!(percentile(&[7], 90), Some(7));
    }

    #[test]
    fn percentile_rank_uses_ceiling_division() {
        // n=3: p50 -> rank 2, p90 -> rank 3 (verified against the plan's
        // fixture expectations).
        assert_eq!(percentile(&[0, 100, 200], 50), Some(100));
        assert_eq!(percentile(&[0, 100, 200], 90), Some(200));
    }

    #[test]
    fn resolve_now_prefers_the_explicit_clock() {
        assert_eq!(resolve_now(Some(42)), 42);
        assert!(resolve_now(None) > 1_700_000_000);
    }

    #[test]
    fn incident_without_a_following_tag_is_unrestored() {
        let tags = vec![git::Tag {
            name: "v0.1.0".to_string(),
            rev: "a".to_string(),
            deploy_ts: 100,
        }];
        let incident = incident_from(&tags, "f".to_string(), 200);
        assert_eq!(incident.restored_by, None);
        assert_eq!(incident.restore_ts, None);
    }
}
