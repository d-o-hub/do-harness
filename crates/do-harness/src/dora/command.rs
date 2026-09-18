//! `do-harness dora`: CLI-facing derivation, rendering, and exit verdict.

use std::path::Path;

use anyhow::{Result, anyhow};

use crate::CliError;
use crate::dora::{self, Source};
use crate::report::Format;

/// Marker the sensor runner parses for the blessed findings ratchet.
const FINDINGS_MARKER: &str = "FINDINGS:";
/// Marker the evidence artifact folds into its coverage record.
const COVERAGE_MARKER: &str = "COVERAGE:";

/// Runs `do-harness dora`.
///
/// Exit codes are the verdict: `0` when no threshold is breached, `1` when at
/// least one is, and `2` for a usage, config, or discovery problem (including
/// a shallow clone whose ranges cannot be resolved). A collector that cannot
/// read git history must never read as healthy, so every failure path exits
/// `2` rather than emitting a zero-valued snapshot.
///
/// # Errors
///
/// Returns [`CliError::Usage`] for a non-work-tree root, a missing or invalid
/// policy file, a failed derivation, a failed `gh` enrichment, or a failed
/// snapshot write; returns [`CliError::Verify`] when the policy is breached.
pub async fn run(
    root: &Path,
    days: Option<i64>,
    format: Format,
    record: bool,
    source: Source,
    now: Option<i64>,
) -> std::result::Result<(), CliError> {
    let mut policy = dora::policy::load(root).map_err(CliError::Usage)?;
    if let Some(days) = days {
        if days <= 0 {
            return Err(CliError::Usage(anyhow!(
                "--days must be positive, got {days}"
            )));
        }
        // `--days` overrides the run, never the file: the pinned policy stays
        // the single source of truth and prior evidence keeps its meaning.
        policy.window_days = days;
    }

    let resolved_now = dora::resolve_now(now);
    let mut snapshot =
        dora::collect(root, &policy, resolved_now, source).map_err(CliError::Usage)?;

    if source == Source::Gh {
        // No git-only degradation: the `source` column would then lie.
        dora::gh::enrich(root, &mut snapshot).map_err(CliError::Usage)?;
    }

    if record {
        record_snapshot(root, &snapshot).await?;
    }

    emit(&snapshot, format);

    if snapshot.breaches.is_empty() {
        Ok(())
    } else {
        Err(CliError::Verify(anyhow!(
            "{} DORA threshold(s) breached",
            snapshot.breaches.len()
        )))
    }
}

/// Persists one snapshot row.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened or migrated, the
/// derivation manifest cannot be serialized, or the insert fails.
async fn record_snapshot(root: &Path, snapshot: &dora::DoraSnapshot) -> Result<(), CliError> {
    let derivation =
        serde_json::to_string(&snapshot.derivation).map_err(|err| CliError::Usage(err.into()))?;
    let breach_count =
        i64::try_from(snapshot.breaches.len()).map_err(|err| CliError::Usage(err.into()))?;
    let row = do_harness_db::NewDoraSnapshot {
        source: &snapshot.source,
        window_days: snapshot.window_days,
        window_start: snapshot.window_start,
        window_end: snapshot.window_end,
        source_rev: &snapshot.source_rev,
        deploy_count: snapshot.deploy_count,
        deploys_failed: snapshot.deploys_failed,
        lead_samples: snapshot.lead_samples,
        lead_p50_seconds: snapshot.lead_p50_seconds,
        lead_p90_seconds: snapshot.lead_p90_seconds,
        mttr_seconds: snapshot.mttr_seconds,
        mttr_restored: snapshot.mttr_restored,
        mttr_unrestored: snapshot.mttr_unrestored,
        breach_count,
        policy_fingerprint: &snapshot.policy_fingerprint,
        derivation: &derivation,
    };

    let conn = do_harness_db::connect_and_migrate(root)
        .await
        .map_err(|err| CliError::Usage(anyhow!(err)))?;
    do_harness_db::insert_dora_snapshot(&conn, &row)
        .await
        .map_err(|err| CliError::Usage(anyhow!(err)))?;
    Ok(())
}

/// Writes the report to stdout and the markers plus breach lines to stderr.
///
/// The markers go to stderr so `--format json` keeps stdout exactly one JSON
/// object while the sensor runner still parses `FINDINGS:`/`COVERAGE:` from
/// the combined capture.
fn emit(snapshot: &dora::DoraSnapshot, format: Format) {
    match format {
        Format::Json => match serde_json::to_string_pretty(snapshot) {
            Ok(json) => println!("{json}"),
            Err(err) => eprintln!("error: failed to serialize DORA snapshot: {err}"),
        },
        Format::Text => print!("{}", render_text(snapshot)),
    }

    for breach in &snapshot.breaches {
        eprintln!(
            "breach: {} {} > {}",
            breach.name, breach.observed, breach.limit
        );
    }
    eprintln!("{FINDINGS_MARKER} {}", snapshot.breaches.len());
    // Compact so the marker stays on exactly one line.
    match serde_json::to_string(&snapshot.derivation) {
        Ok(derivation) => eprintln!("{COVERAGE_MARKER} {derivation}"),
        Err(err) => eprintln!("error: failed to serialize DORA derivation: {err}"),
    }
}

/// Length of the abbreviated revision shown in text mode.
const SHORT_REV: usize = 7;

/// Renders the human table.
///
/// A measurement that does not exist prints as `-`, never as `0`: an absent
/// metric must not read as a healthy zero. The breach block is omitted
/// entirely when nothing breached, rather than printing an empty list.
#[must_use]
fn render_text(snapshot: &dora::DoraSnapshot) -> String {
    use std::fmt::Write as _;

    let rev: String = snapshot.source_rev.chars().take(SHORT_REV).collect();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "dora  window={}d  source={}  rev={rev}",
        snapshot.window_days, snapshot.source
    );
    let _ = writeln!(
        out,
        "  {:<18} {}  (failed {}){}",
        "deploys",
        snapshot.deploy_count,
        snapshot.deploys_failed,
        if snapshot.deploy_tags.is_empty() {
            String::new()
        } else {
            format!("  {}", snapshot.deploy_tags.join(", "))
        }
    );
    let _ = writeln!(
        out,
        "  {:<18} p50 {}  p90 {}  (n={})",
        "lead time",
        lead_label(snapshot.lead_p50_seconds),
        lead_label(snapshot.lead_p90_seconds),
        snapshot.lead_samples
    );
    let _ = writeln!(
        out,
        "  {:<18} {}",
        "change failure rate",
        rate_label(snapshot.deploys_failed, snapshot.deploy_count)
    );
    let _ = writeln!(
        out,
        "  {:<18} {}  ({} restored, {} unrestored)",
        "time to restore",
        snapshot
            .mttr_seconds
            .map_or_else(|| "-".to_string(), dora::policy::format_seconds),
        snapshot.mttr_restored,
        snapshot.mttr_unrestored
    );
    let _ = writeln!(out, "  {:<18} {}", "policy", snapshot.policy_fingerprint);

    if snapshot.breaches.is_empty() {
        let _ = writeln!(out, "  {:<18} none", "breaches");
    } else {
        for (index, breach) in snapshot.breaches.iter().enumerate() {
            let label = if index == 0 { "breaches" } else { "" };
            let _ = writeln!(
                out,
                "  {label:<18} {} {} > {}",
                breach.name, breach.observed, breach.limit
            );
        }
    }
    out
}

/// Formats a lead-time percentile as days, or `-` when unmeasured.
fn lead_label(seconds: Option<i64>) -> String {
    seconds.map_or_else(|| "-".to_string(), dora::policy::format_seconds)
}

/// Formats the change failure rate as an exact `failed/total = 0.000` triple.
#[must_use]
#[allow(clippy::cast_precision_loss)]
fn rate_label(failed: i64, total: i64) -> String {
    if total <= 0 {
        return "-".to_string();
    }
    format!("{failed}/{total} = {:.3}", failed as f64 / total as f64)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn snapshot() -> dora::DoraSnapshot {
        serde_json::from_str(
            r#"{
              "window_days": 30, "window_start": 1787053312, "window_end": 1789645312,
              "source": "git", "source_rev": "d8b4dbd76e15ddd70c2a1ebb016e493ebb6e0881",
              "deploy_count": 2, "deploys_failed": 1,
              "deploy_tags": ["v0.1.0", "v0.1.1"],
              "lead_samples": 165, "lead_p50_seconds": 865179,
              "lead_p90_seconds": 2335088, "mttr_seconds": null,
              "mttr_restored": 0, "mttr_unrestored": 1,
              "breaches": [
                {"name": "change_failure_rate", "observed": "1/2", "limit": "0.150"},
                {"name": "unrestored_deploys", "observed": "1", "limit": "0"}
              ],
              "policy_fingerprint": "sha256:9f3c",
              "derivation": {
                "tag_glob": "refs/tags/v*", "merge_strategy": "squash",
                "percentile_method": "nearest-rank",
                "revert_pattern": "^revert(\\(|:)", "window_days": 30,
                "policy_digest": "9f3c", "clock_skew_commits": 0,
                "ranges": [], "incidents": []
              }
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn text_report_matches_the_contract() {
        let text = render_text(&snapshot());
        assert!(text.starts_with("dora  window=30d  source=git  rev=d8b4dbd\n"));
        assert!(text.contains("  deploys            2  (failed 1)  v0.1.0, v0.1.1\n"));
        assert!(text.contains("  lead time          p50 10.01d  p90 27.03d  (n=165)\n"));
        assert!(text.contains("  change failure rate 1/2 = 0.500\n"));
        assert!(text.contains("  time to restore    -  (0 restored, 1 unrestored)\n"));
        assert!(text.contains("  policy             sha256:9f3c\n"));
        assert!(text.contains("  breaches           change_failure_rate 1/2 > 0.150\n"));
        assert!(text.contains("                     unrestored_deploys 1 > 0\n"));
    }

    #[test]
    fn unmeasured_metrics_print_as_dash_never_zero() {
        let mut snap = snapshot();
        snap.deploy_count = 0;
        snap.deploy_tags.clear();
        snap.lead_samples = 0;
        snap.lead_p50_seconds = None;
        snap.lead_p90_seconds = None;
        snap.deploys_failed = 0;
        snap.mttr_unrestored = 0;
        snap.breaches.clear();
        let text = render_text(&snap);
        assert!(text.contains("  lead time          p50 -  p90 -  (n=0)\n"));
        assert!(text.contains("  change failure rate -\n"));
        assert!(text.contains("  time to restore    -  (0 restored, 0 unrestored)\n"));
        assert!(text.contains("  breaches           none\n"));
    }
}
