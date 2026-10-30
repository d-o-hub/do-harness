//! Candidate draft generation from failure beats and error signatures (WF-09).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use super::types::{FeedbackDraft, MAX_EXCERPT_BYTES};
use crate::redaction::Redactor;

const SECONDS_PER_DAY: i64 = 86400;
const DEFAULT_WINDOW_DAYS: i64 = 30;
const MAX_WINDOW_DAYS: i64 = 365;
const DEFAULT_MIN_FIRES: u32 = 3;
const COOLDOWN_SECONDS: i64 = 60;
/// Validates that `repo` has the `OWNER/REPO` shape.
pub fn validate_repo(repo: &str) -> Result<()> {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 || parts[0].trim().is_empty() || parts[1].trim().is_empty() {
        bail!("invalid target repository '{repo}': expected format 'OWNER/REPO'");
    }
    Ok(())
}

fn fingerprint_for(repo: &str, signature: &str, message: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(repo.as_bytes());
    hasher.update(b":");
    hasher.update(signature.as_bytes());
    hasher.update(b":");
    hasher.update(message.trim().as_bytes());
    format!("fp_{:x}", hasher.finalize())[..18].to_owned()
}

fn is_severe(signature: &str) -> bool {
    signature.starts_with("panic:")
        || signature.starts_with("safety:")
        || signature.starts_with("critical:")
        || signature.starts_with("corrupt:")
}

/// Identifies ordinary expected test failures (e.g. ATDD red phase) or consumer
/// test failures that must not be reported as harness defects.
fn is_expected_atdd_or_non_harness(signature: &str, message: &str) -> bool {
    let lower_sig = signature.to_ascii_lowercase();
    let lower_msg = message.to_ascii_lowercase();

    if lower_sig.contains("atdd")
        || lower_sig.contains("expected")
        || lower_msg.contains("[atdd-red]")
        || lower_msg.contains("expected failure")
        || lower_msg.contains("atdd: red")
        || lower_msg.contains("atdd red")
    {
        return true;
    }

    if signature == "sensor:test"
        && !lower_msg.contains("panic")
        && !lower_msg.contains("fatal")
        && !lower_msg.contains("segmentation fault")
        && !lower_msg.contains("harness")
        && (lower_msg.contains("assertion failed") || lower_msg.contains("test result: failed"))
    {
        return true;
    }

    false
}

/// Groups failed beats by verification execution, treating retries within 60s
/// as part of a single incident.
fn count_independent_beat_runs(
    beats: &[do_harness_types::Beat],
    sensor: &str,
    start_seconds: i64,
) -> usize {
    let mut timestamps: Vec<i64> = beats
        .iter()
        .filter(|b| {
            b.beat_type == do_harness_types::BeatType::Sensor
                && b.sensor_name.as_deref() == Some(sensor)
                && !b.is_ok()
                && b.started_at >= start_seconds
        })
        .map(|b| b.started_at)
        .collect();
    timestamps.sort_unstable();

    let mut independent_runs = 0;
    let mut last_ts = None;
    for ts in timestamps {
        if let Some(prev) = last_ts {
            if ts - prev >= COOLDOWN_SECONDS {
                independent_runs += 1;
                last_ts = Some(ts);
            }
        } else {
            independent_runs = 1;
            last_ts = Some(ts);
        }
    }
    independent_runs
}
/// Generates feedback drafts for eligible failures.
pub async fn generate_drafts(
    root: &Path,
    target_repo: &str,
    window_days: Option<i64>,
    min_fires: Option<u32>,
) -> Result<Vec<FeedbackDraft>> {
    validate_repo(target_repo)?;
    let days = window_days
        .unwrap_or(DEFAULT_WINDOW_DAYS)
        .clamp(1, MAX_WINDOW_DAYS);
    let min_count = i64::from(min_fires.unwrap_or(DEFAULT_MIN_FIRES).max(1));
    let now = do_harness_db::unix_now();
    let start_seconds = now.saturating_sub(days * SECONDS_PER_DAY);

    let redactor = Redactor::from_environment(&[]);
    let mut drafts = Vec::new();

    if !do_harness_db::db_path(root).exists() {
        return Ok(drafts);
    }

    let conn = do_harness_db::connect_and_migrate(root)
        .await
        .context("open state database for feedback")?;

    let signatures = do_harness_db::list_error_signatures(&conn, None)
        .await
        .context("list error signatures")?;

    let beats = do_harness_db::list_beats(&conn, None)
        .await
        .unwrap_or_default();
    let event_counts = read_event_signatures(root, start_seconds);

    let mut seen_fingerprints = BTreeSet::new();

    for sig in signatures {
        let severe = is_severe(&sig.signature);

        let raw_msg = sig
            .message
            .as_deref()
            .unwrap_or("no failure message recorded");
        let clean_msg = redactor.text(raw_msg);

        // Attribution gate: suppress expected ATDD red checks and non-harness consumer failures.
        if !severe && is_expected_atdd_or_non_harness(&sig.signature, &clean_msg) {
            continue;
        }

        let sensor_name = sig
            .signature
            .strip_prefix("sensor:")
            .unwrap_or(&sig.signature);
        let beat_runs = count_independent_beat_runs(&beats, sensor_name, start_seconds);
        let event_count =
            usize::try_from(event_counts.get(&sig.signature).copied().unwrap_or(0)).unwrap_or(0);

        // Distinguish retries from independent verification runs:
        let independent_runs = if beat_runs > 0 {
            beat_runs.max(event_count)
        } else {
            usize::try_from(sig.attempt_count)
                .unwrap_or(0)
                .max(event_count)
        };
        let runs_i64 = i64::try_from(independent_runs).unwrap_or(i64::MAX);
        if !severe && runs_i64 < min_count {
            continue;
        }

        let fp = fingerprint_for(target_repo, &sig.signature, &clean_msg);
        if seen_fingerprints.contains(&fp) {
            continue;
        }
        seen_fingerprints.insert(fp.clone());

        drafts.push(build_candidate_draft(
            target_repo,
            &sig,
            &clean_msg,
            fp,
            severe,
            independent_runs,
            days,
            now,
        ));
    }

    Ok(drafts)
}

#[allow(clippy::too_many_arguments)]
fn build_candidate_draft(
    target_repo: &str,
    sig: &do_harness_types::ErrorSignature,
    clean_msg: &str,
    fp: String,
    severe: bool,
    independent_runs: usize,
    days: i64,
    now: i64,
) -> FeedbackDraft {
    let excerpt = if clean_msg.len() > MAX_EXCERPT_BYTES {
        format!("{}... [truncated]", &clean_msg[..MAX_EXCERPT_BYTES])
    } else {
        clean_msg.to_owned()
    };

    let title = format!("[harness:{}] recurring failure signature", sig.signature);
    let occurrences = if severe { 1 } else { independent_runs };
    let impact = if severe {
        "Severe crash or safety assertion failure occurred.".to_owned()
    } else {
        format!(
            "Failure recurred {occurrences} independent times across distinct verification runs in the last {days} days."
        )
    };

    let reproduction = format!(
        "do-harness verify --only {}",
        sig.signature.trim_start_matches("sensor:")
    );
    let verification = "cargo check && do-harness verify".to_owned();

    let body = format!(
        "## Summary\n\
         Automated impact feedback candidate for `{sig_name}`.\n\n\
         ## Impact\n\
         {impact}\n\n\
         ## Observed Behavior\n\
         Sensor or execution failed with recorded error signature `{sig_name}`.\n\n\
         ## Reproduction\n\
         ```bash\n\
         {reproduction}\n\
         ```\n\n\
         ## Diagnostic Excerpt\n\
         ```text\n\
         {excerpt}\n\
         ```\n\n\
         ## Suggested Verification\n\
         ```bash\n\
         {verification}\n\
         ```\n\n\
         <!-- do-harness-feedback-fingerprint: {fp} -->\n",
        sig_name = sig.signature
    );

    FeedbackDraft {
        schema_version: 1,
        fingerprint: fp,
        target_repo: target_repo.to_owned(),
        title,
        body,
        impact,
        build_identity: env!("CARGO_PKG_VERSION").to_owned(),
        source_ids: vec![sig.id.to_string()],
        observed_behavior: format!("Error signature '{}' failed", sig.signature),
        expected_behavior: "Sensor completes cleanly with exit code 0".to_owned(),
        reproduction,
        diagnostic_excerpt: excerpt,
        suggested_verification: verification,
        created_at: now,
    }
}

fn read_event_signatures(root: &Path, start_seconds: i64) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    let event_dir = root.join(".agents/events");
    if !event_dir.is_dir() {
        return counts;
    }
    let mut paths = Vec::new();
    collect_files(&event_dir, &mut paths);
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if val.get("event").and_then(serde_json::Value::as_str) != Some("sensor-fire") {
            continue;
        }
        let ts = val
            .get("timestamp")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        if ts > 0 && ts < start_seconds {
            continue;
        }
        if let Some(sig) = val
            .get("payload")
            .and_then(|p| p.get("signature"))
            .and_then(serde_json::Value::as_str)
        {
            *counts.entry(sig.to_owned()).or_insert(0) += 1;
        }
    }
    counts
}

fn collect_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
}
