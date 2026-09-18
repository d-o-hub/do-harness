//! Pinned DORA derivation policy (`plans/dora.json`).
//!
//! Every key in the policy file changes the derived numbers — window length,
//! tag glob, merge strategy, percentile method, revert predicate, bot
//! allowlist, and the threshold bands. Pinning them in a committed file (and
//! declaring that file as the `dora` sensor's `coverage-inputs`) is what turns
//! definition drift into stale evidence instead of a silently incomparable
//! number. There is no built-in default: a defaulted policy is exactly the
//! definitional drift this design exists to prevent.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::DoraSnapshot;

/// Repository-relative path of the committed policy file.
pub const POLICY_PATH: &str = "plans/dora.json";

/// The only `revert_pattern` value the derivation supports.
///
/// The predicate is a fixed two-prefix check on the conventional-commit type
/// (`revert(` / `revert:`), not an evaluated regex. Shipping a pattern string
/// that is silently ignored would make the policy file lie about what it
/// controls, and supporting arbitrary regexes would add a dependency and a
/// new failure mode, so a single form is enforced instead.
pub const SUPPORTED_REVERT_PATTERN: &str = "^revert(\\(|:)";

/// The only `percentile_method` value the derivation implements.
pub const SUPPORTED_PERCENTILE_METHOD: &str = "nearest-rank";

/// Derivation policy for `do-harness dora`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoraPolicy {
    /// Free-form rationale for the pinned values. Read from the same file but
    /// never used in a derivation; it exists so the thresholds carry their
    /// reasoning next to them instead of in a separate document.
    #[serde(rename = "$comment", default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Rolling window in days selecting which deploys are measured.
    pub window_days: i64,
    /// Tag glob selecting deploy tags (e.g. `refs/tags/v*`).
    pub tag_glob: String,
    /// Recorded merge strategy of the repository; documentation-only, because
    /// it explains why `--first-parent` measures commit-to-deploy rather than
    /// PR-open-to-merge.
    pub merge_strategy: String,
    /// Percentile estimator name; only `nearest-rank` is implemented.
    pub percentile_method: String,
    /// Revert predicate; only `^revert(\(|:)` is implemented.
    pub revert_pattern: String,
    /// Author emails excluded from lead-time samples (bots, release engines).
    pub bot_allowlist: Vec<String>,
    /// Minimum deploys in the window before the deployment-frequency metric
    /// is considered measured.
    pub min_deploys: i64,
    /// Ceiling for the 90th percentile lead time, in days.
    pub max_lead_p90_days: i64,
    /// Ceiling for the change failure rate, as a fraction in `0.0..=1.0`.
    pub max_change_failure_rate: f64,
    /// Ceiling for the median time to restore, in hours.
    pub max_mttr_hours: i64,
    /// Maximum incidents left unrestored before the window is a breach.
    pub max_unrestored: i64,
}

/// Loads and validates `<root>/plans/dora.json`.
///
/// # Errors
///
/// Returns an error naming the full path when the file is missing or
/// unparseable, and an error when a policy value is outside the single form
/// the derivation implements.
pub fn load(root: &Path) -> Result<DoraPolicy> {
    let path = root.join(POLICY_PATH);
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read DORA policy at {}", path.display()))?;
    let policy: DoraPolicy = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse DORA policy at {}", path.display()))?;
    policy.validate()?;
    Ok(policy)
}

impl DoraPolicy {
    /// Rejects values the derivation does not implement.
    ///
    /// # Errors
    ///
    /// Returns an error naming the offending key and the accepted form.
    pub fn validate(&self) -> Result<()> {
        if self.revert_pattern != SUPPORTED_REVERT_PATTERN {
            bail!(
                "unsupported revert_pattern {:?}: the only implemented predicate is {SUPPORTED_REVERT_PATTERN:?}",
                self.revert_pattern
            );
        }
        if self.percentile_method != SUPPORTED_PERCENTILE_METHOD {
            bail!(
                "unsupported percentile_method {:?}: the only implemented method is {SUPPORTED_PERCENTILE_METHOD:?}",
                self.percentile_method
            );
        }
        if self.window_days <= 0 {
            bail!("window_days must be positive, got {}", self.window_days);
        }
        if !(0.0..=1.0).contains(&self.max_change_failure_rate) {
            bail!(
                "max_change_failure_rate must be a fraction in 0.0..=1.0, got {}",
                self.max_change_failure_rate
            );
        }
        // Every remaining threshold is a floor or a ceiling that is only
        // meaningful as a positive quantity. A negative value is not merely
        // odd, it inverts the rule: `mttr_unrestored > max_unrestored` with
        // `max_unrestored = -1` breaches on a window with zero incidents, so
        // the policy would report a failure the history never had.
        if self.min_deploys < 0 {
            bail!("min_deploys must not be negative, got {}", self.min_deploys);
        }
        if self.max_lead_p90_days <= 0 {
            bail!(
                "max_lead_p90_days must be positive, got {}",
                self.max_lead_p90_days
            );
        }
        if self.max_mttr_hours <= 0 {
            bail!(
                "max_mttr_hours must be positive, got {}",
                self.max_mttr_hours
            );
        }
        if self.max_unrestored < 0 {
            bail!(
                "max_unrestored must not be negative, got {}",
                self.max_unrestored
            );
        }
        Ok(())
    }

    /// Change failure rate ceiling in basis points, rounded once.
    ///
    /// The breach comparison cross-multiplies this integer against the deploy
    /// count, so the rate is never compared as a float.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn max_change_failure_rate_bp(&self) -> i64 {
        (self.max_change_failure_rate * 10_000.0).round() as i64
    }
}

/// Content digest of the policy file for fingerprinting: hex SHA-256 of the
/// raw bytes, or the absent marker when the file cannot be read.
#[must_use]
pub fn digest(root: &Path) -> String {
    use sha2::{Digest, Sha256};

    match std::fs::read(root.join(POLICY_PATH)) {
        Ok(bytes) => hex::encode(Sha256::digest(&bytes)),
        Err(_) => "absent".to_string(),
    }
}

/// Which way a threshold rule points.
///
/// The comparator is part of what a breach *means*, not a rendering detail:
/// a ceiling rule (`observed` must be at or below `limit`) and a floor rule
/// (`observed` must be at or above `limit`) read as opposite comparisons, so
/// carrying it on the breach keeps the report and the rule from drifting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// The observed value must not exceed the limit (`observed > limit` broke it).
    Ceiling,
    /// The observed value must reach the limit (`observed < limit` broke it).
    Floor,
}

impl Direction {
    /// The comparison operator that expresses the violation.
    #[must_use]
    pub fn operator(self) -> char {
        match self {
            Self::Ceiling => '>',
            Self::Floor => '<',
        }
    }
}

/// One threshold breach: what was measured against the pinned limit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Breach {
    /// Stable rule name (`lead_p90`, `change_failure_rate`, …).
    pub name: String,
    /// Observed value, formatted for stable JSON.
    pub observed: String,
    /// Direction the limit is enforced in.
    pub direction: Direction,
    /// Pinned limit the observation broke.
    pub limit: String,
}

impl Breach {
    /// The violation as one comparable phrase (`1/2 > 0.150`).
    #[must_use]
    pub fn comparison(&self) -> String {
        format!(
            "{} {} {}",
            self.observed,
            self.direction.operator(),
            self.limit
        )
    }
}

/// Seconds in one day.
const SECONDS_PER_DAY: i64 = 86_400;

/// Seconds in one hour.
const SECONDS_PER_HOUR: i64 = 3_600;

/// Evaluates every threshold rule against a snapshot.
///
/// The four rules are deliberately non-overlapping: zero incidents report
/// nothing, an only-unrestored window reports `unrestored_deploys` alone, and
/// a window with at least one recovery reports `mttr`. Without the
/// `mttr_restored > 0` precondition an unrestored incident would be reported
/// twice, and a repository with no incidents would report a restore failure it
/// never had.
#[must_use]
pub fn breaches(snapshot: &DoraSnapshot, policy: &DoraPolicy) -> Vec<Breach> {
    let mut found = Vec::new();

    if snapshot.deploy_count < policy.min_deploys {
        found.push(Breach {
            name: "deploys_below_min".to_string(),
            observed: snapshot.deploy_count.to_string(),
            direction: Direction::Floor,
            limit: policy.min_deploys.to_string(),
        });
    }

    // A missing p90 is a breach on purpose: "no samples" is not evidence of
    // health. In practice it is `None` only when `deploy_count == 0`, so it
    // accompanies `deploys_below_min` rather than replacing it.
    let lead_limit = policy.max_lead_p90_days * SECONDS_PER_DAY;
    match snapshot.lead_p90_seconds {
        Some(p90) if p90 <= lead_limit => {}
        Some(p90) => found.push(Breach {
            name: "lead_p90".to_string(),
            observed: format_seconds(p90),
            direction: Direction::Ceiling,
            limit: format_seconds(lead_limit),
        }),
        None => found.push(Breach {
            name: "lead_p90".to_string(),
            observed: "-".to_string(),
            direction: Direction::Ceiling,
            limit: format_seconds(lead_limit),
        }),
    }

    // Exact integer cross-multiplication, never a float compare.
    if snapshot.deploy_count > 0
        && snapshot.deploys_failed * 10_000
            > policy.max_change_failure_rate_bp() * snapshot.deploy_count
    {
        found.push(Breach {
            name: "change_failure_rate".to_string(),
            observed: format!("{}/{}", snapshot.deploys_failed, snapshot.deploy_count),
            direction: Direction::Ceiling,
            limit: format!("{:.3}", policy.max_change_failure_rate),
        });
    }

    if snapshot.mttr_restored > 0 {
        let mttr_limit = policy.max_mttr_hours * SECONDS_PER_HOUR;
        match snapshot.mttr_seconds {
            Some(mttr) if mttr <= mttr_limit => {}
            Some(mttr) => found.push(Breach {
                name: "mttr".to_string(),
                observed: format_seconds(mttr),
                direction: Direction::Ceiling,
                limit: format_seconds(mttr_limit),
            }),
            None => found.push(Breach {
                name: "mttr".to_string(),
                observed: "-".to_string(),
                direction: Direction::Ceiling,
                limit: format_seconds(mttr_limit),
            }),
        }
    }

    if snapshot.mttr_unrestored > policy.max_unrestored {
        found.push(Breach {
            name: "unrestored_deploys".to_string(),
            observed: snapshot.mttr_unrestored.to_string(),
            direction: Direction::Ceiling,
            limit: policy.max_unrestored.to_string(),
        });
    }

    found
}

/// Formats a second count as days with two decimals (`27.03d`).
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_seconds(seconds: i64) -> String {
    format!("{:.2}d", seconds as f64 / SECONDS_PER_DAY as f64)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn policy() -> DoraPolicy {
        serde_json::from_str(
            r#"{
              "window_days": 30,
              "tag_glob": "refs/tags/v*",
              "merge_strategy": "squash",
              "percentile_method": "nearest-rank",
              "revert_pattern": "^revert(\\(|:)",
              "bot_allowlist": [],
              "min_deploys": 1,
              "max_lead_p90_days": 30,
              "max_change_failure_rate": 0.15,
              "max_mttr_hours": 24,
              "max_unrestored": 0
            }"#,
        )
        .unwrap()
    }

    fn snapshot() -> DoraSnapshot {
        serde_json::from_str(
            r#"{
              "window_days": 30, "window_start": 0, "window_end": 0,
              "source": "git", "source_rev": "deadbeef",
              "deploy_count": 2, "deploys_failed": 1, "deploy_tags": [],
              "lead_samples": 165, "lead_p50_seconds": 865179,
              "lead_p90_seconds": 2335088, "mttr_seconds": null,
              "mttr_restored": 0, "mttr_unrestored": 1, "breaches": [],
              "policy_fingerprint": "sha256:x",
              "derivation": {
                "tag_glob": "refs/tags/v*", "merge_strategy": "squash",
                "percentile_method": "nearest-rank",
                "revert_pattern": "^revert(\\(|:)", "window_days": 30,
                "policy_digest": "abc", "clock_skew_commits": 0,
                "ranges": [], "incidents": []
              }
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn rate_ceiling_rounds_to_basis_points() {
        assert_eq!(policy().max_change_failure_rate_bp(), 1500);
    }

    #[test]
    fn rate_breach_is_exact_integer_cross_multiplication() {
        // 1/7 = 0.142857… is below a 0.15 ceiling; 1/6 is above it.
        let mut snap = snapshot();
        snap.deploy_count = 7;
        snap.deploys_failed = 1;
        snap.lead_p90_seconds = Some(0);
        snap.mttr_unrestored = 0;
        assert!(breaches(&snap, &policy()).is_empty());
        snap.deploy_count = 6;
        assert_eq!(
            breaches(&snap, &policy())
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>(),
            vec!["change_failure_rate"]
        );
    }

    #[test]
    fn unrestored_incidents_report_once_and_not_as_mttr() {
        let names: Vec<String> = breaches(&snapshot(), &policy())
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert_eq!(names, vec!["change_failure_rate", "unrestored_deploys"]);
    }

    #[test]
    fn zero_incidents_report_no_restore_failure() {
        let mut snap = snapshot();
        snap.deploy_count = 1;
        snap.deploys_failed = 0;
        snap.lead_p90_seconds = Some(0);
        snap.mttr_unrestored = 0;
        assert!(breaches(&snap, &policy()).is_empty());
    }

    #[test]
    fn restored_incidents_report_mttr() {
        let mut snap = snapshot();
        snap.deploy_count = 1;
        snap.deploys_failed = 0;
        snap.lead_p90_seconds = Some(0);
        snap.mttr_unrestored = 0;
        snap.mttr_restored = 1;
        snap.mttr_seconds = Some(2 * SECONDS_PER_DAY);
        let names: Vec<String> = breaches(&snap, &policy())
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert_eq!(names, vec!["mttr"]);
    }

    #[test]
    fn missing_lead_p90_is_a_breach() {
        let mut snap = snapshot();
        snap.deploy_count = 1;
        snap.deploys_failed = 0;
        snap.lead_p90_seconds = None;
        snap.mttr_unrestored = 0;
        assert_eq!(
            breaches(&snap, &policy())
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>(),
            vec!["lead_p90"]
        );
    }

    #[test]
    fn unsupported_policy_values_are_rejected() {
        let mut p = policy();
        p.revert_pattern = "^revert".to_string();
        assert!(p.validate().is_err());
        let mut p = policy();
        p.percentile_method = "linear".to_string();
        assert!(p.validate().is_err());
        assert!(policy().validate().is_ok());
    }

    #[test]
    fn undocumented_unknown_field_is_rejected() {
        assert!(serde_json::from_str::<DoraPolicy>(r#"{"window_days": 1, "bogus": 2}"#).is_err());
    }
}
