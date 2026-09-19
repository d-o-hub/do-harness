//! Deterministic skill ranking and the optional semantic selector.
//!
//! Ranking is one pure function over catalog metadata so it can be benchmarked
//! or replaced without touching the catalog or the CLI. The selector is an
//! optional external executable that may only *reorder* already-chosen
//! candidates: it never sees a skill body, never introduces a path, and cannot
//! execute a skill or grant a permission.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::catalog::SkillMetadata;

/// Environment variable naming the optional selector executable.
pub const SELECTOR_ENV: &str = "DO_HARNESS_SKILL_SELECTOR";

/// Environment variable bounding the selector's wall time, in seconds.
pub const SELECTOR_TIMEOUT_ENV: &str = "DO_HARNESS_SKILL_SELECTOR_TIMEOUT";

/// Selector timeout when the environment does not set one.
pub const SELECTOR_TIMEOUT_SECS: f64 = 10.0;

/// Selector wire-format version.
pub const SELECTOR_SCHEMA_VERSION: u32 = 1;

/// Shortest query/haystack token kept; drops single characters and punctuation.
const MIN_TOKEN_LEN: usize = 2;

/// Score weight added when the skill name appears in the query.
const NAME_HIT_WEIGHT: f64 = 1.0;

/// One ranked candidate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scored {
    /// Skill name.
    pub name: String,
    /// Skill description.
    pub description: String,
    /// Short description, when the frontmatter carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,
    /// Repository-relative `SKILL.md` path.
    pub path: String,
    /// Rank score in `[0.0, 2.0]`, rounded to three decimals.
    pub score: f64,
}

impl Scored {
    /// Builds a scored candidate from catalog metadata.
    #[must_use]
    pub fn from_metadata(metadata: &SkillMetadata, score: f64) -> Scored {
        Scored {
            name: metadata.name.clone(),
            description: metadata.description.clone(),
            short_description: metadata.short_description.clone(),
            path: metadata.path.clone(),
            score,
        }
    }
}

/// Ranks `entries` against `query`, returning at most `limit` candidates.
///
/// The algorithm is deliberately cheap and model-free: split both sides into
/// lowercased alphanumeric tokens, score the fraction of query tokens present
/// in the skill's metadata, and add a full weight when the skill name appears
/// as a whole query token. Ties break on `(name, path)` so the order never
/// depends on filesystem iteration.
#[must_use]
pub fn rank(entries: &[SkillMetadata], query: &str, limit: usize) -> Vec<Scored> {
    if limit == 0 {
        return Vec::new();
    }
    let query_tokens = tokenize(query);
    let mut scored: Vec<Scored> = entries
        .iter()
        .map(|metadata| Scored::from_metadata(metadata, score(metadata, &query_tokens)))
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.path.cmp(&b.path))
    });
    scored.truncate(limit);
    scored
}

/// Scores one skill against the already-tokenized query.
fn score(metadata: &SkillMetadata, query_tokens: &[String]) -> f64 {
    let overlap = if query_tokens.is_empty() {
        0.0
    } else {
        let haystack: BTreeSet<String> = tokenize(&haystack(metadata)).into_iter().collect();
        let matched = query_tokens
            .iter()
            .filter(|token| haystack.contains(*token))
            .count();
        #[allow(clippy::cast_precision_loss)]
        let ratio = matched as f64 / query_tokens.len() as f64;
        ratio
    };
    let name_hit = name_terms(&metadata.name)
        .iter()
        .any(|term| query_tokens.contains(term));
    let total = overlap + if name_hit { NAME_HIT_WEIGHT } else { 0.0 };
    (total * 1000.0).round() / 1000.0
}

/// The text a query is matched against: name plus both descriptions.
fn haystack(metadata: &SkillMetadata) -> String {
    let mut text = String::with_capacity(metadata.description.len() + 64);
    text.push_str(&metadata.name);
    text.push(' ');
    if let Some(short) = &metadata.short_description {
        text.push_str(short);
        text.push(' ');
    }
    text.push_str(&metadata.description);
    text
}

/// Lowercases, splits on non-alphanumerics, drops short tokens, and dedupes
/// preserving first-seen order. No stopword list and no stemming: the corpus is
/// small and a hidden word list would make ranking harder to reason about.
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for token in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.chars().count() >= MIN_TOKEN_LEN)
    {
        let lowered = token.to_lowercase();
        if seen.insert(lowered.clone()) {
            out.push(lowered);
        }
    }
    out
}

/// Tokens of a skill name, which is usually hyphen-separated.
fn name_terms(name: &str) -> Vec<String> {
    tokenize(name)
}

/// Outcome of the optional semantic selection step.
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    /// Deterministic order stands (no selector configured, or it was rejected).
    Deterministic,
    /// The selector reordered candidates; the named set is a candidate subset.
    Selected(Vec<String>),
}

/// Optional selector configuration, resolved from the environment by
/// [`SelectorConfig::from_env`] and passed explicitly to [`select`].
///
/// The explicit form keeps selection a pure function of its inputs, so tests
/// exercise it without mutating process-global environment state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectorConfig {
    /// Selector executable path; `None` disables semantic selection.
    pub bin: Option<String>,
    /// Wall-clock bound for one selector invocation.
    pub timeout: Duration,
}

impl SelectorConfig {
    /// Reads the selector configuration from the environment.
    #[must_use]
    pub fn from_env() -> SelectorConfig {
        let bin = std::env::var(SELECTOR_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let secs = std::env::var(SELECTOR_TIMEOUT_ENV)
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(SELECTOR_TIMEOUT_SECS);
        SelectorConfig {
            bin,
            timeout: Duration::from_secs_f64(secs),
        }
    }
}

/// Applies the optional selector to `candidates`, falling back with a warning.
///
/// Every rejection is a fallback, never a partial application: the
/// deterministic ranking is always a valid answer, so a missing, broken,
/// slow, or untrusted selector degrades the result rather than failing the
/// command. The selector may only name candidates it was offered.
#[must_use]
pub fn select(
    root: &Path,
    query: &str,
    candidates: &[Scored],
    config: &SelectorConfig,
) -> (Selection, Vec<String>) {
    let mut warnings = Vec::new();
    let Some(bin) = config.bin.clone() else {
        return (Selection::Deterministic, warnings);
    };
    if !Path::new(&bin).is_file() {
        warnings.push(format!("{SELECTOR_ENV} is not a file: {bin}"));
        return (Selection::Deterministic, warnings);
    }
    let timeout = config.timeout;

    let payload = serde_json::json!({
        "schema_version": SELECTOR_SCHEMA_VERSION,
        "query": query,
        "candidates": candidates.iter().map(|candidate| serde_json::json!({
            "name": candidate.name,
            "description": candidate.description,
            "score": candidate.score,
        })).collect::<Vec<_>>(),
    });
    let input = payload.to_string();

    let mut child = match crate::shell::retry_executable_busy(|| {
        Command::new(&bin)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }) {
        Ok(child) => child,
        Err(err) => {
            warnings.push(format!("cannot spawn {SELECTOR_ENV}={bin}: {err}"));
            return (Selection::Deterministic, warnings);
        }
    };
    let write_result = child
        .stdin
        .take()
        .context("selector stdin unavailable")
        .and_then(|mut stdin| {
            stdin
                .write_all(input.as_bytes())
                .context("cannot write selector input")
        });
    if let Err(err) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        warnings.push(format!("selector input failed: {err}"));
        return (Selection::Deterministic, warnings);
    }

    let outcome = wait_with_timeout(child, timeout);
    match outcome {
        Err(reason) => {
            warnings.push(reason);
            (Selection::Deterministic, warnings)
        }
        Ok(stdout) => match validate(&stdout, candidates) {
            Ok(selected) => (Selection::Selected(selected), warnings),
            Err(reason) => {
                warnings.push(reason);
                (Selection::Deterministic, warnings)
            }
        },
    }
}

/// Waits for the selector, killing it once `timeout` elapses.
fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> Result<String, String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if !status.success() {
                    return Err(format!("selector exited with {status}"));
                }
                return Ok(stdout);
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "selector exceeded {SELECTOR_TIMEOUT_ENV} of {timeout:?}"
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => return Err(format!("selector wait failed: {err}")),
        }
    }
}

/// Selector response body.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    schema_version: u32,
    selected: Vec<String>,
    confidence: f64,
}

/// Validates selector output against the candidate set.
///
/// Every rejection reason is a fallback, never a partial application: an
/// out-of-set name, a duplicate, an over-long list, or an out-of-range
/// confidence all mean the deterministic ranking is used unchanged.
///
/// Exposed to the crate so the rejection matrix is testable without spawning a
/// process: the spawn path is platform-specific, the trust logic is not.
pub(crate) fn validate(stdout: &str, candidates: &[Scored]) -> Result<Vec<String>, String> {
    let response: Response = serde_json::from_str(stdout)
        .map_err(|err| format!("selector output is not JSON: {err}"))?;
    if response.schema_version != SELECTOR_SCHEMA_VERSION {
        return Err(format!(
            "selector schema_version {} is unsupported",
            response.schema_version
        ));
    }
    if !response.confidence.is_finite() || !(0.0..=1.0).contains(&response.confidence) {
        return Err(format!(
            "selector confidence {} is outside [0,1]",
            response.confidence
        ));
    }
    let allowed: BTreeSet<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
    let mut seen = BTreeSet::new();
    for name in &response.selected {
        if !allowed.contains(name.as_str()) {
            return Err(format!("selector chose '{name}' outside the candidate set"));
        }
        if !seen.insert(name.as_str()) {
            return Err(format!("selector repeated candidate '{name}'"));
        }
    }
    if response.selected.len() > candidates.len() {
        return Err(format!(
            "selector chose {} candidates from {} offered",
            response.selected.len(),
            candidates.len()
        ));
    }
    Ok(response.selected)
}

/// Reorders `candidates` to put `selected` first, preserving both relative
/// orders. Names absent from `selected` keep their deterministic position.
#[must_use]
pub fn reorder(candidates: Vec<Scored>, selected: &[String]) -> Vec<Scored> {
    let mut chosen: Vec<Scored> = Vec::with_capacity(candidates.len());
    let mut rest: Vec<Scored> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if selected.contains(&candidate.name) {
            chosen.push(candidate);
        } else {
            rest.push(candidate);
        }
    }
    // Within the chosen group, honor the selector's own order.
    chosen.sort_by_key(|candidate| {
        selected
            .iter()
            .position(|name| *name == candidate.name)
            .unwrap_or(usize::MAX)
    });
    chosen.extend(rest);
    chosen
}
