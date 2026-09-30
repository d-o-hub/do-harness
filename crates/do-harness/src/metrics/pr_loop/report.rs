//! Text rendering for the PR-loop snapshot.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::PrLoopSnapshot;

/// Renders the snapshot as a table plus a totals block.
#[must_use]
pub(crate) fn render_text(snapshot: &PrLoopSnapshot) -> String {
    let mut out = String::new();
    let source = if snapshot.cached { "cache" } else { "fetched" };
    let _ = writeln!(
        out,
        "metrics pr: {} since {} ({} PR(s), {} green) — {} {}",
        snapshot.repo,
        snapshot.since,
        snapshot.totals.prs,
        snapshot.totals.green_prs,
        source,
        stamp(snapshot.fetched_at)
    );
    if !snapshot.prs.is_empty() {
        out.push('\n');
        out.push_str("PR      state   pushes  time-to-green  waivers                    comments(a/i)  runs  cancelled  reruns\n");
        for row in &snapshot.prs {
            let _ = writeln!(
                out,
                "#{:<6} {:<7} {:>6}  {:>13}  {:<26} {:>13}  {:>4}  {:>9}  {:>6}",
                row.number,
                row.state,
                row.pushes,
                duration(row.time_to_green_secs).unwrap_or_else(|| "-".to_owned()),
                sum_summary(&row.waivers),
                format!(
                    "{} / {}",
                    row.actionable_comments, row.informational_comments
                ),
                row.runs,
                row.cancelled_runs,
                row.reruns,
            );
        }
    }
    let totals = &snapshot.totals;
    out.push('\n');
    let _ = writeln!(
        out,
        "time-to-green: p50 {}, p95 {}    pushes/green: {}",
        duration(totals.time_to_green_p50_secs).unwrap_or_else(|| "-".to_owned()),
        duration(totals.time_to_green_p95_secs).unwrap_or_else(|| "-".to_owned()),
        totals
            .pushes_per_green
            .map_or_else(|| "-".to_owned(), |value| format!("{value:.1}")),
    );
    let _ = writeln!(
        out,
        "waivers: {}    comments: actionable {}, informational {}",
        sum_summary(&totals.waivers),
        totals.actionable_comments,
        totals.informational_comments
    );
    let _ = writeln!(
        out,
        "runs: {}, cancelled {} ({})    reruns: {}",
        totals.runs,
        totals.cancelled_runs,
        totals
            .cancelled_rate
            .map_or_else(|| "-".to_owned(), |rate| format!("{:.1}%", rate * 100.0)),
        totals.reruns
    );
    for warning in &snapshot.warnings {
        let _ = writeln!(out, "warning: {warning}");
    }
    out
}

/// `macro-field 2, guarded-arm 5`, or `-` when empty.
fn sum_summary(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "-".to_owned();
    }
    counts
        .iter()
        .map(|(class, count)| format!("{class} {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Human duration (`45s`, `41m`, `2h10m`).
fn duration(seconds: Option<u64>) -> Option<String> {
    let seconds = seconds?;
    if seconds < 60 {
        return Some(format!("{seconds}s"));
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        let rest = seconds % 60;
        return Some(format!("{minutes}m{rest:02}s"));
    }
    let hours = minutes / 60;
    Some(format!("{}h{:02}m", hours, minutes % 60))
}

/// RFC 3339 UTC stamp for display.
fn stamp(epoch: i64) -> String {
    let (year, month, day) = crate::events::civil_from_unix(epoch);
    let seconds = epoch.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}
