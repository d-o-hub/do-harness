//! HTML and JSON rendering for the verification timeline.
//!
//! Every recorded string is HTML-escaped and every link is a local path, so a
//! hostile sensor name or verdict cannot inject markup into the page and the
//! rendered file needs no network to open.

use std::fmt::Write as _;

use anyhow::Result;
use serde_json::{Value, json};

use super::{Binding, TIMELINE_SCHEMA, Timeline, observations_for, sensor_rows};

/// Escapes text for HTML text nodes and attribute values.
pub(super) fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Renders the timeline as one self-contained HTML page.
pub(super) fn html(timeline: &Timeline) -> String {
    let mut page = String::new();
    page.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    page.push_str("<title>Verification timeline</title>\n<style>\n");
    page.push_str(STYLE);
    page.push_str("</style>\n</head>\n<body>\n<h1>Verification timeline</h1>\n");
    let _ = writeln!(
        page,
        "<p class=\"meta\">attempts: {} · order: supplied · gaps: {}</p>",
        timeline.attempts.len(),
        timeline.gaps.len()
    );
    for (index, attempt) in timeline.attempts.iter().enumerate() {
        page.push_str(&attempt_html(timeline, index, attempt));
    }
    page.push_str("<section class=\"gaps\">\n<h2>Coverage gaps</h2>\n");
    if timeline.gaps.is_empty() {
        page.push_str("<p>none</p>\n");
    } else {
        page.push_str("<ul>\n");
        for gap in &timeline.gaps {
            let _ = writeln!(page, "<li>{}</li>", escape(gap));
        }
        page.push_str("</ul>\n");
    }
    page.push_str("</section>\n</body>\n</html>\n");
    page
}

/// One attempt section.
fn attempt_html(timeline: &Timeline, index: usize, attempt: &super::Attempt) -> String {
    let document = &attempt.document;
    let artifact = escape(&attempt.path.to_string_lossy());
    let verdict = escape(&document.summary.verdict);
    let mut html = String::new();
    let _ = writeln!(html, "<section class=\"attempt {verdict}\">");
    let _ = writeln!(html, "<h2>Attempt {} — {verdict}</h2>", index + 1);
    let _ = writeln!(
        html,
        "<p class=\"meta\">artifact: <a href=\"{artifact}\">{artifact}</a> · revision: {} \
         · signal set: {} · started: {} · finished: {}</p>",
        escape(document.git_sha.as_deref().unwrap_or("unknown")),
        escape(document.signal_set.as_deref().unwrap_or("all")),
        document.started_at,
        document.finished_at
    );
    let _ = writeln!(
        html,
        "<p class=\"meta\">sensors: {} pass · {} fail · {} skip · task: {}</p>",
        document.summary.pass,
        document.summary.fail,
        document.summary.skip,
        document
            .task_id
            .map_or_else(|| "none".to_owned(), |id| id.to_string())
    );
    html.push_str(&freshness_html(timeline, index));
    html.push_str(&delta_html(timeline, index));
    html.push_str(&sensors_html(document));
    html.push_str(&skipped_html(document));
    if let Some(reason) = &document.invalidated_reason {
        let _ = writeln!(
            html,
            "<p class=\"gap\">invalidated during run: {}</p>",
            escape(reason)
        );
    }
    html.push_str("</section>\n");
    html
}

/// The freshness sequence for one attempt, or the explicit unknown when no
/// observation was supplied for it.
fn freshness_html(timeline: &Timeline, index: usize) -> String {
    let observations = observations_for(timeline, index);
    if observations.is_empty() {
        return "<p class=\"fresh unknown\">freshness: unknown (no bound status snapshot)</p>\n"
            .to_owned();
    }
    let mut html = String::from("<ul class=\"freshness\">\n");
    for binding in observations {
        let snapshot = &binding.snapshot;
        let state = escape(&snapshot.state);
        let path = escape(&snapshot.path.to_string_lossy());
        let _ = writeln!(
            html,
            "<li class=\"fresh {state}\">freshness: {state} ({}) observed at {} by <a href=\"{path}\">{path}</a></li>",
            escape(&snapshot.reason),
            snapshot.observed_at
        );
    }
    html.push_str("</ul>\n");
    html
}

/// What moved between this attempt and the one before it.
fn delta_html(timeline: &Timeline, index: usize) -> String {
    let Some(previous) = index
        .checked_sub(1)
        .map(|previous| &timeline.attempts[previous])
    else {
        return String::new();
    };
    let before = &previous.document;
    let after = &timeline.attempts[index].document;
    let mut changes: Vec<&str> = Vec::new();
    if before.git_sha != after.git_sha {
        changes.push("revision");
    }
    if before.workspace_fingerprint != after.workspace_fingerprint {
        changes.push("workspace fingerprint");
    }
    if before.policy_fingerprint != after.policy_fingerprint {
        changes.push("policy fingerprint");
    }
    if before.config_fingerprint != after.config_fingerprint {
        changes.push("config fingerprint");
    }
    if before.signal_set != after.signal_set {
        changes.push("signal set");
    }
    if before.input_manifest != after.input_manifest {
        changes.push("sensor input identity");
    }
    if changes.is_empty() {
        return "<p class=\"meta\">unchanged since the previous attempt</p>\n".to_owned();
    }
    format!(
        "<p class=\"meta\">changed since the previous attempt: {}</p>\n",
        escape(&changes.join(", "))
    )
}

/// The per-sensor table: verdict, exit code, and recorded duration.
fn sensors_html(document: &crate::evidence::EvidenceDocument) -> String {
    let mut html = String::from(
        "<table>\n<thead><tr><th>sensor</th><th>verdict</th><th>exit</th>\
         <th>duration</th><th>recorded</th></tr></thead>\n<tbody>\n",
    );
    for row in sensor_rows(document) {
        let text = |key: &str| {
            row.get(key)
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned()
        };
        let number = |key: &str| row.get(key).and_then(Value::as_i64);
        let verdict = text("verdict");
        let _ = writeln!(
            html,
            "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&verdict),
            escape(&text("name")),
            escape(&verdict),
            number("exit_code").map_or_else(|| "-".to_owned(), |code| code.to_string()),
            number("duration_ms").map_or_else(|| "unknown".to_owned(), |ms| format!("{ms} ms")),
            if row
                .get("recorded")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "yes"
            } else {
                "no"
            },
        );
    }
    html.push_str("</tbody>\n</table>\n");
    html
}

/// Sensors the change-aware selection did not run, with their reasons.
fn skipped_html(document: &crate::evidence::EvidenceDocument) -> String {
    if document.skipped.is_empty() {
        return String::new();
    }
    let mut html = String::from("<p class=\"meta\">skipped by selection:</p>\n<ul>\n");
    for skipped in &document.skipped {
        let _ = writeln!(
            html,
            "<li>{}: {}</li>",
            escape(&skipped.name),
            escape(&skipped.reason)
        );
    }
    html.push_str("</ul>\n");
    html
}

/// Renders the timeline as a JSON document.
///
/// # Errors
///
/// Returns an error when the document cannot be serialized.
pub(super) fn json(timeline: &Timeline) -> Result<String> {
    let attempts: Vec<Value> = timeline
        .attempts
        .iter()
        .enumerate()
        .map(|(index, attempt)| {
            let document = &attempt.document;
            json!({
                "path": attempt.path.to_string_lossy(),
                "schema_version": document.schema_version,
                "verdict": document.summary.verdict,
                "git_sha": document.git_sha,
                "signal_set": document.signal_set,
                "task_id": document.task_id,
                "started_at": document.started_at,
                "finished_at": document.finished_at,
                "invalidated_reason": document.invalidated_reason,
                "sensors": sensor_rows(document),
                "skipped": document.skipped.iter().map(|skipped| json!({
                    "name": skipped.name,
                    "reason": skipped.reason,
                })).collect::<Vec<Value>>(),
                "chain": {
                    "prev_hash": document.prev_hash,
                    "chain_hash": document.chain_hash,
                },
                "freshness": observations_for(timeline, index)
                    .into_iter()
                    .map(freshness_value)
                    .collect::<Vec<Value>>(),
            })
        })
        .collect();
    let document = json!({
        "schema_version": TIMELINE_SCHEMA,
        "order": "supplied",
        "attempts": attempts,
        "gaps": timeline.gaps,
    });
    Ok(format!("{}\n", serde_json::to_string_pretty(&document)?))
}

/// One bound observation, as JSON.
fn freshness_value(binding: &Binding) -> Value {
    json!({
        "state": binding.snapshot.state,
        "reason": binding.snapshot.reason,
        "observed_at": binding.snapshot.observed_at,
        "snapshot": binding.snapshot.path.to_string_lossy(),
    })
}

/// Inline stylesheet: one file, no external request.
const STYLE: &str = "\
body { font-family: ui-sans-serif, system-ui, sans-serif; margin: 2rem auto; max-width: 60rem; \
padding: 0 1rem; color: #111; background: #fff; }
h1 { font-size: 1.5rem; }
h2 { font-size: 1.1rem; margin: 1.5rem 0 .5rem; }
section.attempt { border-left: 4px solid #999; padding-left: 1rem; margin-bottom: 2rem; }
section.attempt.pass { border-color: #157f3b; }
section.attempt.fail { border-color: #b3261e; }
p.meta { color: #444; font-size: .875rem; margin: .25rem 0; }
p.fresh.green { color: #157f3b; }
p.fresh.red { color: #b3261e; }
p.fresh.stale, p.fresh.unknown, p.fresh.missing, li.fresh.stale, li.fresh.unknown { color: #8a6d00; }
ul.freshness { font-size: .875rem; margin: .25rem 0; list-style: none; padding-left: 0; }
li.fresh.green { color: #157f3b; }
li.fresh.red { color: #b3261e; }
p.gap { color: #b3261e; font-weight: 600; }
table { border-collapse: collapse; width: 100%; font-size: .875rem; margin: .5rem 0; }
th, td { text-align: left; padding: .25rem .5rem; border-bottom: 1px solid #ddd; }
tr.fail td { color: #b3261e; }
tr.warn td { color: #8a6d00; }
tr.skip td { color: #666; }
a { color: #0b57d0; }
";
