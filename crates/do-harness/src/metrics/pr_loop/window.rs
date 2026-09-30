//! Window parsing and the clock for the PR-loop measures.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

/// Current Unix time.
#[must_use]
pub(crate) fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |delta| {
            i64::try_from(delta.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Parses `30d` / `12h` / `2w` / `YYYY-MM-DD` into a Unix timestamp.
///
/// # Errors
///
/// Returns an error when the window is neither a duration nor a date.
pub(crate) fn parse_window(since: &str) -> Result<i64> {
    let trimmed = since.trim();
    if trimmed.is_empty() {
        anyhow::bail!("--since must be a duration (30d) or a date (YYYY-MM-DD)");
    }
    if let Some(days) = trimmed
        .strip_suffix('d')
        .and_then(|n| n.parse::<i64>().ok())
    {
        return Ok(now_epoch() - days * 86_400);
    }
    if let Some(hours) = trimmed
        .strip_suffix('h')
        .and_then(|n| n.parse::<i64>().ok())
    {
        return Ok(now_epoch() - hours * 3_600);
    }
    if let Some(weeks) = trimmed
        .strip_suffix('w')
        .and_then(|n| n.parse::<i64>().ok())
    {
        return Ok(now_epoch() - weeks * 7 * 86_400);
    }
    if let Some(at) = crate::dora::gh::parse_rfc3339_utc(&format!("{trimmed}T00:00:00Z")) {
        return Ok(at);
    }
    anyhow::bail!("--since must be a duration (30d, 12h, 2w) or a date (YYYY-MM-DD), got '{since}'")
}
