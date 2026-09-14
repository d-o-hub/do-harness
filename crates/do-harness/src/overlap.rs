//! `do-harness overlap`: Tier-2 distinctiveness advisory for the skill corpus.
//!
//! Ranks every skill pair by cosine similarity over word-frequency vectors
//! built from `SKILL.md` plus `references/**/*.md` (the guidance payload an
//! agent actually loads). Pairs at or above `--threshold` print as WARN with
//! their top shared terms; the command stays advisory and always exits 0 so
//! topical vocabulary shared across a domain never fails a gate. Calibrated
//! default (`0.45`) flags the genuine overlaps in this repo while the
//! harness-hub vocabulary (`0.25`–`0.36`) stays informational.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Result, bail};

use crate::report::Format;

/// Default cosine threshold: flags genuine duplication, not hub vocabulary.
pub const DEFAULT_THRESHOLD: f64 = 0.45;

/// How many shared terms to show per pair.
const SHARED_TERMS: usize = 5;

/// Minimal stopword list: domain glue that would otherwise dominate every
/// pair. Deliberately small — topical overlap must stay visible.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "have", "are", "was", "were", "will",
    "would", "should", "could", "there", "their", "them", "they", "you", "your", "our", "can",
    "not", "but", "all", "any", "per", "via", "into", "over", "under", "about", "between", "each",
    "other", "more", "most", "such", "than", "then", "also", "when", "use", "used", "using",
    "based",
];

/// One ranked skill pair.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlapPair {
    /// First skill name (alphabetically).
    pub a: String,
    /// Second skill name.
    pub b: String,
    /// Cosine similarity of the frequency vectors, `0.0`–`1.0`.
    pub similarity: f64,
    /// Top shared terms by minimum frequency, ties alphabetical.
    pub shared: Vec<String>,
}

/// Ranks every skill pair in `skills_root` by descending similarity.
pub fn corpus_pairs(skills_root: &Path) -> Result<Vec<OverlapPair>> {
    let mut docs: BTreeMap<String, HashMap<String, u32>> = BTreeMap::new();
    let entries = std::fs::read_dir(skills_root)
        .map_err(|err| anyhow::anyhow!("cannot read {}: {err}", skills_root.display()))?;
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() || !dir.join("SKILL.md").is_file() {
            continue;
        }
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        docs.insert(name.to_owned(), skill_terms(&dir));
    }
    let names: Vec<&str> = docs.keys().map(String::as_str).collect();
    let mut pairs = Vec::new();
    for (i, a) in names.iter().enumerate() {
        for b in &names[i + 1..] {
            let (similarity, shared) = compare(&docs[*a], &docs[*b]);
            pairs.push(OverlapPair {
                a: (*a).to_owned(),
                b: (*b).to_owned(),
                similarity,
                shared,
            });
        }
    }
    pairs.sort_by(|x, y| {
        y.similarity
            .partial_cmp(&x.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(pairs)
}

/// Frequency vector over a skill's guidance payload.
fn skill_terms(dir: &Path) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut files = vec![dir.join("SKILL.md")];
    let mut stack = vec![dir.join("references")];
    while let Some(top) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&top) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                files.push(path);
            }
        }
    }
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        for token in tokenize(&text) {
            *counts.entry(token).or_insert(0) += 1;
        }
    }
    counts
}

/// Lowercase alphanumeric tokens without glue words or stubs.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|raw| {
            let token = raw.to_ascii_lowercase();
            (token.len() > 2 && !STOPWORDS.contains(&token.as_str())).then_some(token)
        })
        .collect()
}

/// Cosine similarity plus top shared terms by minimum frequency.
fn compare(a: &HashMap<String, u32>, b: &HashMap<String, u32>) -> (f64, Vec<String>) {
    let mut dot = 0f64;
    let mut shared: Vec<(&str, u32)> = Vec::new();
    for (term, ca) in a {
        if let Some(cb) = b.get(term) {
            dot += f64::from(*ca) * f64::from(*cb);
            shared.push((term.as_str(), (*ca).min(*cb)));
        }
    }
    let norm = |counts: &HashMap<String, u32>| {
        f64::sqrt(counts.values().map(|v| f64::from(*v) * f64::from(*v)).sum())
    };
    let (na, nb) = (norm(a), norm(b));
    let similarity = if na > 0.0 && nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    };
    shared.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(y.0)));
    let terms = shared
        .into_iter()
        .take(SHARED_TERMS)
        .map(|(term, _)| term.to_owned())
        .collect();
    (similarity, terms)
}

/// Runs the overlap advisory over `.agents/skills`.
///
/// # Errors
///
/// Returns an error when the threshold is outside `[0, 1]` or the corpus
/// cannot be read. Flagged pairs never error: the command is advisory.
pub fn run_overlap(root: &Path, threshold: f64, format: Format) -> Result<()> {
    if !(0.0..=1.0).contains(&threshold) {
        bail!("invalid --threshold {threshold}: expected 0.0 to 1.0");
    }
    let pairs = corpus_pairs(&root.join(".agents/skills"))?;
    let flagged = pairs
        .iter()
        .filter(|pair| pair.similarity >= threshold)
        .count();
    match format {
        Format::Text => {
            for pair in &pairs {
                println!(
                    "{} <-> {} sim={:.3} shared=[{}]",
                    pair.a,
                    pair.b,
                    pair.similarity,
                    pair.shared.join(", ")
                );
            }
            if flagged > 0 {
                println!("WARN: {flagged} pair(s) at or above threshold {threshold:.2}");
            } else {
                println!(
                    "OK: no pair at or above threshold {threshold:.2} ({} checked)",
                    pairs.len()
                );
            }
        }
        Format::Json => println!(
            "{}",
            serde_json::json!({
                "threshold": threshold,
                "flagged": flagged,
                "pairs": pairs.iter().map(|pair| serde_json::json!({
                    "a": pair.a,
                    "b": pair.b,
                    "similarity": pair.similarity,
                    "shared": pair.shared,
                })).collect::<Vec<_>>(),
            })
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn skill_at(root: &Path, name: &str, body: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("references")).unwrap();
        std::fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn near_duplicate_skills_rank_first() {
        let root = tempfile::tempdir().unwrap();
        skill_at(
            root.path(),
            "alpha",
            "# Alpha\nSpike scratchpad hypothesis verification cleanup transition.",
        );
        skill_at(
            root.path(),
            "beta",
            "# Beta\nSpike scratchpad hypothesis verification cleanup transition extra.",
        );
        skill_at(
            root.path(),
            "gamma",
            "# Gamma\nUnrelated haiku prose ocean waves.",
        );

        let pairs = corpus_pairs(root.path()).unwrap();
        assert_eq!(pairs.len(), 3);
        assert_eq!(
            (pairs[0].a.as_str(), pairs[0].b.as_str()),
            ("alpha", "beta")
        );
        assert!(pairs[0].similarity > 0.8, "sim={}", pairs[0].similarity);
        assert!(pairs[0].shared.contains(&"spike".to_owned()));
        assert!(pairs[2].similarity < pairs[0].similarity);
    }

    #[test]
    fn empty_and_missing_corpus_yields_no_pairs() {
        let root = tempfile::tempdir().unwrap();
        assert!(corpus_pairs(root.path()).unwrap().is_empty());
        skill_at(root.path(), "solo", "# Solo\nOnly skill present here.");
        assert!(corpus_pairs(root.path()).unwrap().is_empty());
    }

    #[test]
    fn tokenize_drops_glue_and_stubs() {
        let tokens = tokenize("The spike IS it, a1 ok spike!");
        assert!(!tokens.contains(&"the".to_owned()));
        assert!(!tokens.contains(&"is".to_owned()));
        assert!(!tokens.contains(&"it".to_owned()));
        assert!(tokens.contains(&"spike".to_owned()));
    }
}
