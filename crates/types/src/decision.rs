//! Machine-readable architecture decision headers.
//!
//! A [`DecisionHeader`] replaces free-form ADR prose with three minimal,
//! verifiable fields: the invariant itself, a one-sentence rationale, and the
//! sensor that verifies compliance.

use serde::{Deserialize, Serialize};

/// A single executable architecture invariant.
///
/// Fields follow the `Invariant` / `Rationale` / `Sensor` contract:
/// the constraint, why it exists, and how compliance is verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionHeader {
    /// The hard constraint (e.g., "libSQL for state, max 500 LOC per file").
    pub invariant: String,
    /// One-sentence reason the invariant exists.
    pub rationale: String,
    /// The test or command that verifies compliance.
    pub sensor: String,
    /// Broad category of the invariant (e.g., "architecture", "storage").
    pub category: String,
}

impl DecisionHeader {
    /// Constructs a new decision header from its three core fields.
    #[must_use]
    pub const fn new(
        invariant: String,
        rationale: String,
        sensor: String,
        category: String,
    ) -> Self {
        Self {
            invariant,
            rationale,
            sensor,
            category,
        }
    }
}

/// Accepted top-level shapes for `plans/invariants.json`.
///
/// Accepts either a bare `DecisionHeader` array or an object with a top-level
/// `invariants` array (extra object keys such as `$comment` are ignored so
/// policy tooling can annotate the file). Each header itself stays strict
/// (`deny_unknown_fields`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
struct InvariantsWrapper {
    invariants: Vec<DecisionHeader>,
}

/// Parses `plans/invariants.json` in either accepted shape.
///
/// # Errors
///
/// Returns an error describing the expected schema when the input matches
/// neither a `DecisionHeader` array nor an `{"invariants": [...]}` object.
pub fn parse_invariants_json(json: &str) -> Result<Vec<DecisionHeader>, String> {
    if let Ok(headers) = serde_json::from_str::<Vec<DecisionHeader>>(json) {
        return Ok(headers);
    }
    if let Ok(wrapper) = serde_json::from_str::<InvariantsWrapper>(json) {
        return Ok(wrapper.invariants);
    }
    Err(
        "invalid invariants.json: expected a JSON array of DecisionHeader objects \
         ({invariant, rationale, sensor, category}) or an object with a top-level \
         \"invariants\" array; see plans/invariants.json"
            .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{DecisionHeader, parse_invariants_json};

    fn header() -> DecisionHeader {
        DecisionHeader::new(
            "invariant".to_owned(),
            "rationale".to_owned(),
            "sensor".to_owned(),
            "category".to_owned(),
        )
    }

    #[test]
    fn parses_bare_array() {
        let json = serde_json::to_string(&vec![header()]).expect("serialize");
        assert_eq!(parse_invariants_json(&json).expect("parse"), vec![header()]);
    }

    #[test]
    fn parses_wrapper_object_with_comment() {
        let json = serde_json::json!({
            "$comment": "policy tooling annotation",
            "invariants": [header()],
        })
        .to_string();
        assert_eq!(parse_invariants_json(&json).expect("parse"), vec![header()]);
    }

    #[test]
    fn rejects_headers_with_unknown_fields() {
        let json = r#"[{"invariant":"i","rationale":"r","sensor":"s","category":"c","bogus":1}]"#;
        assert!(parse_invariants_json(json).is_err());
    }

    #[test]
    fn error_mentions_expected_schema() {
        let err = parse_invariants_json("{}").expect_err("must fail");
        assert!(err.contains("DecisionHeader"), "{err}");
        assert!(err.contains("invariants"), "{err}");
    }
}
