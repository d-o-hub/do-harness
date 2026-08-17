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
