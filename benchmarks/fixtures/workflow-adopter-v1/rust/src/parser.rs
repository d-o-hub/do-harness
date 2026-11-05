//! Line parsing for `[INFO|WARN|ERROR] component: message` build log records.

/// Severity of a parsed record, ordered so `ERROR > WARN > INFO`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    pub fn parse(value: &str) -> Option<Severity> {
        match value {
            "INFO" => Some(Severity::Info),
            "WARN" => Some(Severity::Warn),
            "ERROR" => Some(Severity::Error),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Warn => "WARN",
            Severity::Error => "ERROR",
        }
    }
}

/// One parsed build-log record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub severity: Severity,
    pub component: String,
    pub message: String,
}

fn parse_line(line: &str) -> Option<Record> {
    let line = line.trim();
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let severity = Severity::parse(rest[..close].trim())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_records() {
        let records = parse("[ERROR] db: connection refused\n[WARN] db: slow query\n");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].severity, Severity::Error);
        assert_eq!(records[0].component, "db");
        assert_eq!(records[0].message, "connection refused");
        assert_eq!(records[1].severity, Severity::Warn);
    }

    #[test]
    fn skips_unrecognized_lines() {
        let records = parse("not a record\n[INFO] build: started\n[UNKNOWN] x: y\n");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].component, "build");
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        let records = parse("  [WARN]   cache : miss  \n");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].component, "cache");
        assert_eq!(records[0].message, "miss");
    }

    #[test]
    fn rejects_missing_component_or_message() {
        assert!(parse("[INFO] nothing-here\n").is_empty());
        assert!(parse("[INFO] : missing component\n").is_empty());
    }
}
