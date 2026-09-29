//! Where recurring sensor fires come from: recorded beats and the event log.
//!
//! Split from `learn.rs` to keep that file under the modularity cap. The two
//! sources use different time units — `beats.started_at` is Unix seconds while
//! event files are dated in civil days — so the conversion lives here, next to
//! both readers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::events::days_from_civil;

/// Upper bound on `--days`, keeping the civil-day arithmetic far from overflow.
pub(super) const MAX_WINDOW_DAYS: i64 = 36_500;
/// Seconds in a day, for converting the window's first civil day to Unix time.
pub(super) const SECONDS_PER_DAY: i64 = 86_400;
/// Event name counted as a sensor fire in `.agents/events/**`.
pub(super) const FIRE_EVENT: &str = "sensor-fire";
/// `name` cap from the Agent Skills specification.
/// Fire counts per source.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Fires {
    /// Fires read from recorded beats.
    pub(super) beats: u32,
    /// Fires read from `.agents/events/**`.
    pub(super) events: u32,
}

impl Fires {
    /// Total fires in the window.
    pub(super) fn total(self) -> u32 {
        self.beats + self.events
    }
}

/// Fires recorded in beats for the current scope since `start_seconds`
/// (a Unix timestamp, matching `beats.started_at`).
pub(super) async fn beat_fires(root: &Path, start_seconds: i64) -> Result<BTreeMap<String, Fires>> {
    let mut fires = BTreeMap::new();
    if !do_harness_db::db_path(root).exists() {
        return Ok(fires);
    }
    let scope = crate::telemetry::BeatScope::resolve(root, None, false);
    let conn = do_harness_db::connect_and_migrate(root)
        .await
        .context("open the state database")?;
    for stat in do_harness_db::sensor_stats(&conn, Some(start_seconds), Some(&scope.key())).await? {
        if stat.failures > 0 {
            let count = u32::try_from(stat.failures).unwrap_or(u32::MAX);
            fires.insert(
                stat.name,
                Fires {
                    beats: count,
                    events: 0,
                },
            );
        }
    }
    Ok(fires)
}

/// Fires recorded as `sensor-fire` events under `.agents/events/**`.
pub(super) fn event_fires(root: &Path, start: i64) -> Result<BTreeMap<String, u32>> {
    let mut fires: BTreeMap<String, u32> = BTreeMap::new();
    let tree = root.join(".agents/events");
    let mut paths = Vec::new();
    collect_json(&tree, &mut paths);
    for path in paths {
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if record.get("event").and_then(serde_json::Value::as_str) != Some(FIRE_EVENT) {
            continue;
        }
        let Some(sensor) = record
            .get("payload")
            .and_then(|payload| payload.get("sensor"))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        if !in_window(root, &path, &record, start) {
            continue;
        }
        if !explicit_ok(&record) {
            *fires.entry(sensor.to_owned()).or_default() += 1;
        }
    }
    Ok(fires)
}

/// Whether a fire record explicitly reports success (and so is not a fire).
pub(super) fn explicit_ok(record: &serde_json::Value) -> bool {
    let payload = record.get("payload");
    let ok = payload
        .and_then(|payload| payload.get("ok"))
        .and_then(serde_json::Value::as_bool);
    let status = payload
        .and_then(|payload| payload.get("status"))
        .and_then(serde_json::Value::as_str);
    ok == Some(true) || status == Some("ok")
}

/// Whether an event file falls inside the window, by its `date` field or the
/// `YYYY/MM/DD` directories it lives under.
pub(super) fn in_window(root: &Path, path: &Path, record: &serde_json::Value, start: i64) -> bool {
    let from_field = record
        .get("date")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_date);
    let from_path = path.strip_prefix(root).ok().and_then(date_from_path);
    match from_field.or(from_path) {
        Some(ordinal) => ordinal >= start,
        None => true,
    }
}

/// The window's first day as a civil-day ordinal.
pub(super) fn window_start(now: i64, days: i64) -> i64 {
    let (year, month, day) = crate::events::civil_from_unix(now);
    days_from_civil(year, i64::from(month), i64::from(day)) - (days - 1)
}

/// Parses `YYYY-MM-DD` into a civil-day ordinal.
///
/// Components are range-checked: `days_from_civil` multiplies the year by
/// 146 097, so an unbounded year or month read from a checked-in record would
/// overflow instead of filtering the record out.
pub(super) fn parse_date(text: &str) -> Option<i64> {
    parse_components(text.split('-'))
}

/// Range-checks `year`, `month`, `day` components and converts them.
fn parse_components<'a>(mut parts: impl Iterator<Item = &'a str>) -> Option<i64> {
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=9999).contains(&year) {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Extracts `YYYY/MM/DD` from a repository-relative event path.
pub(super) fn date_from_path(rel: &Path) -> Option<i64> {
    let parts: Vec<&str> = rel
        .components()
        .filter_map(|part| part.as_os_str().to_str())
        .collect();
    let position = parts.iter().position(|part| *part == "events")?;
    parse_components(parts[position + 1..].iter().copied())
}

/// Recursively collects `*.json` files under `dir`.
///
/// `DirEntry::file_type` does not follow symlinks, so a link back into an
/// ancestor directory cannot make this recurse forever; a symlinked event file
/// is skipped rather than read.
pub(super) fn collect_json(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            collect_json(&path, files);
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            files.push(path);
        }
    }
}
