//! `do-harness skills`: progressive-disclosure skill selection.
//!
//! Every stage is bounded and offline by default: the catalog reads frontmatter
//! only, ranking is a pure function over that metadata, and the optional
//! selector may reorder candidates but never load a body, introduce a path, or
//! execute a skill.

use std::path::Path;

use anyhow::{Result, bail};

use crate::cli::SkillsAction;
use crate::report::Format;

mod cache;
pub mod catalog;
pub mod suggest;

#[cfg(test)]
mod tests;

/// Dispatches a `skills` action.
///
/// # Errors
///
/// Returns an error when the skill root cannot be listed, or the request is a
/// usage error (`--limit 0`).
pub fn run(root: &Path, action: SkillsAction) -> Result<()> {
    match action {
        SkillsAction::Suggest {
            query,
            limit,
            format,
        } => suggest_command(root, &query, limit, format),
    }
}

/// Ranks catalog skills for `query` and prints the result.
fn suggest_command(root: &Path, query: &str, limit: usize, format: Format) -> Result<()> {
    if limit == 0 {
        bail!("invalid --limit 0: at least one candidate must be requested");
    }
    let catalog = catalog::scan(root)?;
    let query_terms = suggest::tokenize(query);
    let candidates = suggest::rank(&catalog.skills, query, limit);

    // The selector can only reorder what the deterministic stage already chose,
    // and the deterministic stage is always reorderable into itself.
    let config = suggest::SelectorConfig::from_env();
    let (selection, mut warnings) = suggest::select(root, query, &candidates, &config);
    warnings.splice(0..0, catalog.warnings);
    let candidates = match &selection {
        suggest::Selection::Deterministic => candidates,
        suggest::Selection::Selected(names) => suggest::reorder(candidates, names),
    };

    match format {
        Format::Json => println!(
            "{}",
            serde_json::json!({
                "schema_version": 1,
                "query_terms": query_terms.len(),
                "catalog_size": catalog.skills.len(),
                "candidates": candidates,
                "warnings": warnings,
            })
        ),
        Format::Text => {
            if candidates.is_empty() {
                println!("no skill candidates");
            }
            for candidate in &candidates {
                let label = candidate
                    .short_description
                    .clone()
                    .unwrap_or_else(|| first_chars(&candidate.description, 80));
                println!("{:.3} {} — {}", candidate.score, candidate.name, label);
            }
        }
    }
    Ok(())
}

/// First `max` characters of `text`, on a character boundary.
fn first_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
