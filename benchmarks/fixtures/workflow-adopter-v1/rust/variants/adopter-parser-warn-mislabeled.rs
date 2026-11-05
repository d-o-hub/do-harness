//! BUGGY VARIANT (scenario: source edit after green): WARN mislabeled as ERROR.
//! A source edit that silently changes preserved behavior; a verification gate
//! must re-run and catch the planted regression instead of trusting stale green.

use crate::parser::{Record, Severity};

fn parse_line(line: &str) -> Option<Record> {
    let line = line.trim();
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let mut severity = Severity::parse(rest[..close].trim())?;
    // Planted regression: WARN records are mislabeled as ERROR.
    if severity == Severity::Warn {
        severity = Severity::Error;
    }
    let after = rest[close + 1..].trim();
    let (component, message) = after.split_once(':')?;
    let component = component.trim();
    let message = message.trim();
    if component.is_empty() || message.is_empty() {
        return None;
    }
    Some(Record {
        severity,
        component: component.to_owned(),
        message: message.to_owned(),
    })
}

/// Parse a log body into records, skipping lines that do not match.
pub fn parse(text: &str) -> Vec<Record> {
    text.lines().filter_map(parse_line).collect()
}
