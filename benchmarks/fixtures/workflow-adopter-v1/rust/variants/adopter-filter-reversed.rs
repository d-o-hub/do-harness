//! BUGGY VARIANT (scenario: multi-file filter fix): reversal bug.
//! Retained set is correct but record order is reversed. Used only by the
//! adopter corpus as a planted defect; never part of the healthy fixture.

use crate::cli::Filter;
use crate::parser::Record;

/// Retain records meeting the filter, but in REVERSED order (planted bug).
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
        .rev()
        .collect()
}
