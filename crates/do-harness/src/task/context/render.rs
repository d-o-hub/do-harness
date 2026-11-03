//! Rendering for `task context`: the text view and the JSON contract.

use std::fmt::Write;

use anyhow::Result;

use super::ContextDocument;
use crate::report::Format;

/// Renders one document in the requested format.
///
/// # Errors
///
/// Returns an error when JSON serialization fails.
pub(super) fn document(doc: &ContextDocument, format: Format) -> Result<String> {
    match format {
        Format::Json => crate::report::json_pretty(doc),
        Format::Text => Ok(text(doc)),
    }
}

/// Compact human view for a new session.
fn text(doc: &ContextDocument) -> String {
    let mut out = String::new();
    header(&mut out, doc);
    gating(&mut out, doc);
    summary(&mut out, doc);
    failures(&mut out, doc);
    traces(&mut out, doc);
    workspace(&mut out, doc);
    skills(&mut out, doc);
    unknowns(&mut out, doc);
    out
}

fn header(out: &mut String, doc: &ContextDocument) {
    if let Some(task) = &doc.task {
        let _ = writeln!(out, "task {}: {} [{}]", task.id, task.title, task.status);
    } else {
        let _ = writeln!(out, "task: unavailable ({})", doc.database.reason);
    }
    if let Some(step) = &doc.step {
        let total = step
            .total_subtasks
            .map_or_else(|| "?".to_owned(), |t| t.to_string());
        let _ = writeln!(
            out,
            "step: {} (index {} of {}) sensor={} state={}",
            step.name.as_deref().unwrap_or("-"),
            step.index,
            total,
            step.sensor.as_deref().unwrap_or("-"),
            step.state
        );
    }
}

fn gating(out: &mut String, doc: &ContextDocument) {
    if let Some(gating) = &doc.gating {
        let _ = writeln!(
            out,
            "gating: state={} reason={} required={}",
            gating.state,
            gating.reason,
            list(&gating.required_sensors)
        );
        for sensor in &gating.sensors {
            let _ = writeln!(
                out,
                "  sensor {}: {}/{}",
                sensor.name, sensor.state, sensor.reason
            );
        }
    }
}

fn summary(out: &mut String, doc: &ContextDocument) {
    let _ = writeln!(
        out,
        "acceptance_criteria: {} ({})",
        if doc.acceptance_criteria.available {
            "available"
        } else {
            "unavailable"
        },
        doc.acceptance_criteria.reason
    );
    let _ = writeln!(
        out,
        "evidence: {}/{} recovery={}",
        doc.evidence.state,
        doc.evidence.reason,
        doc.evidence.recovery.as_deref().unwrap_or("-")
    );
    let action = &doc.next_action;
    let _ = writeln!(
        out,
        "next: {} — {} ({})",
        action.kind,
        action.command.as_deref().unwrap_or("-"),
        action.reason
    );
}

fn failures(out: &mut String, doc: &ContextDocument) {
    let _ = writeln!(
        out,
        "failures: {} shown, {} omitted",
        doc.failures.items.len(),
        doc.failures.omitted
    );
    for failure in &doc.failures.items {
        let _ = writeln!(
            out,
            "  {} attempts={} {}",
            failure.signature,
            failure.attempt_count,
            failure.message.as_deref().unwrap_or("")
        );
    }
}

fn traces(out: &mut String, doc: &ContextDocument) {
    let _ = writeln!(
        out,
        "traces: {} shown, {} omitted",
        doc.traces.items.len(),
        doc.traces.omitted
    );
    for trace in &doc.traces.items {
        let _ = writeln!(
            out,
            "  {} {}",
            trace.id,
            trace.command.as_deref().unwrap_or("(no command)")
        );
        if let Some(diff) = &trace.error_diff {
            let _ = writeln!(out, "    error_diff: {diff}");
        }
    }
}

fn workspace(out: &mut String, doc: &ContextDocument) {
    let workspace = &doc.workspace;
    let _ = writeln!(
        out,
        "workspace (scope={}, not task-scoped): branch={} revision={} changed={} shown, {} omitted",
        workspace.scope,
        workspace.branch.as_deref().unwrap_or("-"),
        workspace.git_revision.as_deref().unwrap_or("-"),
        workspace.changed.items.len(),
        workspace.changed.omitted
    );
    if let Some(error) = &workspace.discovery_error {
        let _ = writeln!(out, "  discovery_error: {error}");
    }
    for path in &workspace.changed.items {
        let _ = writeln!(out, "  {} {}", path.kind, path.path);
    }
}

fn skills(out: &mut String, doc: &ContextDocument) {
    let _ = writeln!(
        out,
        "skills: {} shown, {} omitted",
        doc.skills.items.len(),
        doc.skills.omitted
    );
    for skill in &doc.skills.items {
        let label = skill
            .short_description
            .as_deref()
            .unwrap_or(skill.path.as_str());
        let _ = writeln!(out, "  {:.3} {} — {}", skill.score, skill.name, label);
    }
    if !doc.paths.items.is_empty() {
        let _ = writeln!(out, "paths: {}", list(&doc.paths.items));
    }
}

fn unknowns(out: &mut String, doc: &ContextDocument) {
    for unknown in &doc.unknowns {
        let _ = writeln!(out, "unknown: {} ({})", unknown.section, unknown.reason);
    }
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_owned()
    } else {
        items.join(",")
    }
}
