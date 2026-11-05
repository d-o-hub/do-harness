//! Stable JSON report generation with counts and preserved record order.

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

/// Render the retained records as a stable JSON report. Keys are emitted in a
/// fixed order and records preserve their input order.
pub fn report(records: &[Record]) -> String {
    let mut errors = 0usize;
    let mut warns = 0usize;
    let mut infos = 0usize;
    for record in records {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn reports_stable_counts_and_order() {
        let records = parse("[INFO] build: started\n[WARN] db: slow query\n[ERROR] db: connection refused\n");
        let report = report(&records);
        assert!(report.starts_with("{\"total\":3,\"by_severity\":{\"ERROR\":1,\"INFO\":1,\"WARN\":1},\"records\":["));
        assert!(report.contains("\"severity\":\"INFO\",\"component\":\"build\""));
        assert!(report.ends_with("]}"));
    }

    #[test]
    fn escapes_quotes_and_newlines() {
        let records = parse("[WARN] db: \"fast\" retry\n");
        let report = report(&records);
        assert!(report.contains("\\\"fast\\\""));
    }

    #[test]
    fn empty_input_yields_zero_counts() {
        assert_eq!(report(&[]), "{\"total\":0,\"by_severity\":{\"ERROR\":0,\"INFO\":0,\"WARN\":0},\"records\":[]}");
    }
}
