//! Fixture: the feature-gated shape PR #1042 waived (see ../README.md).

pub mod heuristics;

/// Tier the cascade picked.
pub enum Tier {
    /// Cheap keyword tier.
    Keyword,
    /// Semantic rerank tier.
    Semantic,
}

/// Ranking the cascade produced.
pub struct Ranking {
    /// Ordered candidate IDs.
    pub ids: Vec<String>,
}

#[cfg(feature = "csm")]
pub mod semantic_rerank {
    //! `csm`-gated semantic rerank wiring.

    use super::{Ranking, Tier};

    /// Reranks the top tier through the semantic judge.
    ///
    /// # Errors
    ///
    /// Returns the tier when the judge cannot be reached.
    pub fn rerank_tiers(ids: &[String], enabled: bool) -> Result<Ranking, Tier> {
        if !enabled {
            return Err(Tier::Keyword);
        }
        let mut ordered = ids.to_vec();
        ordered.sort();
        Ok(Ranking { ids: ordered })
    }
}
