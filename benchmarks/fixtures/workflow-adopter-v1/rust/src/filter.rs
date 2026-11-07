//! Severity/component/message filtering that preserves record order.

use crate::cli::Filter;
use crate::parser::Record;

/// Retain records meeting the filter, in original input order.
pub fn apply(records: &[Record], filter: &Filter) -> Vec<Record> {
    records
        .iter()
        .filter(|record| {
            record.severity >= filter.min
                && filter
                    .component
                    .as_ref()
                    .is_none_or(|component| component == &record.component)
                && filter
                    .message
                    .as_ref()
                    .is_none_or(|message| {
                        record.message.to_lowercase().contains(&message.to_lowercase())
                    })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Filter;
    use crate::parser::{parse, Severity};

    fn fixture() -> Vec<Record> {
        parse("[INFO] build: started\n[WARN] db: slow query\n[ERROR] db: connection refused\n")
    }

    #[test]
    fn retains_all_at_default_infimum() {
        let filter = Filter {
            min: Severity::Info,
            component: None,
            message: None,
        };
        assert_eq!(apply(&fixture(), &filter).len(), 3);
    }

    #[test]
    fn minimum_severity_excludes_lower() {
        let filter = Filter {
            min: Severity::Error,
            component: None,
            message: None,
        };
        let retained = apply(&fixture(), &filter);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].severity, Severity::Error);
    }

    #[test]
    fn component_filter_matches_exactly() {
        let filter = Filter {
            min: Severity::Info,
            component: Some("db".to_owned()),
            message: None,
        };
        let retained = apply(&fixture(), &filter);
        assert!(retained.iter().all(|record| record.component == "db"));
    }

    #[test]
    fn message_substring_is_case_insensitive() {
        let filter = Filter {
            min: Severity::Info,
            component: None,
            message: Some("REFUSED".to_owned()),
        };
        let retained = apply(&fixture(), &filter);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].message, "connection refused");
    }
}
