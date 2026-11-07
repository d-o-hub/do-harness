//! BUGGY VARIANT (scenario: alternating failures): inverted severity gate.
//! Lower severities are retained and higher severities dropped, the opposite of
//! the documented contract. Used only by the adopter corpus as a planted defect.

use crate::cli::Filter;
use crate::parser::Record;

/// Retain records whose severity is BELOW the minimum (planted inversion bug).
pub fn apply(records: &[Record], filter: &Filter) -> Vec<Record> {
    records
        .iter()
        .filter(|record| {
            record.severity < filter.min
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
