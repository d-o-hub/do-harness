//! BUGGY VARIANT (scenario: multi-file filter fix): counts-from-all bug.
//! `by_severity` counts come from the FULL parsed set, not the retained
//! filtered records. Used only by the adopter corpus as a planted defect.

use crate::parser::{Record, Severity};

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Render the report; the counts are computed from `all` parsed records
/// (planted bug) while the retained records are emitted as-is.
pub fn report(records: &[Record], all: &[Record]) -> String {
    let mut errors = 0usize;
    let mut warns = 0usize;
    let mut infos = 0usize;
    for record in all {
        match record.severity {
            Severity::Error => errors += 1,
            Severity::Warn => warns += 1,
            Severity::Info => infos += 1,
        }
    }
    let mut out = String::new();
    out.push_str("{\"total\":");
    out.push_str(&records.len().to_string());
    out.push_str(",\"by_severity\":{\"ERROR\":");
    out.push_str(&errors.to_string());
    out.push_str(",\"INFO\":");
    out.push_str(&infos.to_string());
    out.push_str(",\"WARN\":");
    out.push_str(&warns.to_string());
    out.push_str("},\"records\":[");
    for (index, record) in records.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"severity\":");
        out.push_str(&escape(record.severity.as_str()));
        out.push_str(",\"component\":");
        out.push_str(&escape(&record.component));
        out.push_str(",\"message\":");
        out.push_str(&escape(&record.message));
        out.push('}');
    }
    out.push_str("]}");
    out
}
