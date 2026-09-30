//! lcov report parsing and path resolution.
//!
//! Only the records a patch-coverage verdict needs are interpreted: `SF:`
//! (source file), `DA:<line>,<hits>` (line coverage) and `end_of_record`.
//! Every other record (`FN`, `FNDA`, `BRDA`, `LF`, `LH`, `TN`, …) is ignored,
//! so a report from any lcov front end parses without configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Line coverage for one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileCoverage {
    /// Lines the report carries a record for.
    pub recorded: BTreeSet<u32>,
    /// Lines the report marks as executed zero times.
    pub uncovered: BTreeSet<u32>,
}

impl FileCoverage {
    /// Whether the report records the line.
    #[must_use]
    pub fn records(&self, line: u32) -> bool {
        self.recorded.contains(&line)
    }

    /// Whether the report records the line as uncovered.
    #[must_use]
    pub fn misses(&self, line: u32) -> bool {
        self.uncovered.contains(&line)
    }
}

/// Parsed lcov report, keyed by the `SF:` path exactly as written.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Coverage per source file, in report order.
    pub files: BTreeMap<String, FileCoverage>,
    /// Records that could not be interpreted.
    pub warnings: Vec<String>,
}

/// Parses lcov text.
///
/// A `DA:` record without a preceding `SF:` is a warning, not a failure: the
/// report is still usable and the unreadable record stays unproven.
#[must_use]
pub fn parse(text: &str) -> Report {
    let mut report = Report::default();
    let mut current: Option<String> = None;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim_end();
        if let Some(rest) = line.strip_prefix("SF:") {
            let path = rest.trim();
            if path.is_empty() {
                report
                    .warnings
                    .push(format!("lcov line {}: empty SF record", index + 1));
                current = None;
            } else {
                current = Some(path.to_owned());
                report.files.entry(path.to_owned()).or_default();
            }
            continue;
        }
        if line == "end_of_record" {
            current = None;
            continue;
        }
        let Some(record) = line.strip_prefix("DA:") else {
            continue;
        };
        let Some((line_no, hits)) = record.split_once(',') else {
            report
                .warnings
                .push(format!("lcov line {}: malformed DA record", index + 1));
            continue;
        };
        let (Ok(line_no), Ok(hits)) = (
            line_no.trim().parse::<u32>(),
            hits.trim().trim_end_matches(',').parse::<u64>(),
        ) else {
            report
                .warnings
                .push(format!("lcov line {}: malformed DA record", index + 1));
            continue;
        };
        let Some(path) = current.as_ref() else {
            report.warnings.push(format!(
                "lcov line {}: DA record outside a file section",
                index + 1
            ));
            continue;
        };
        let coverage = report.files.entry(path.clone()).or_default();
        coverage.recorded.insert(line_no);
        if hits == 0 {
            coverage.uncovered.insert(line_no);
        }
    }
    report
}

/// Resolves an `SF:` path to a repository-relative path that exists under `root`.
///
/// A CI checkout prefix is the normal case (`/home/runner/work/<repo>/<repo>/src/lib.rs`),
/// so resolution is the longest suffix of the report path that names an existing
/// file: no configuration is needed, and an explicit `strip_prefix` (matching a
/// component boundary) is applied first when the caller knows the prefix.
#[must_use]
pub fn resolve_path(raw: &str, strip_prefix: Option<&str>, root: &Path) -> Option<String> {
    let normalized = raw.replace('\\', "/");
    let stripped = match strip_prefix {
        Some(prefix) => {
            let prefix = prefix.replace('\\', "/");
            let prefix = prefix.trim_end_matches('/');
            normalized.strip_prefix(prefix).map_or_else(
                || normalized.clone(),
                |rest| rest.trim_start_matches('/').to_owned(),
            )
        }
        None => normalized.clone(),
    };

    let candidate = stripped.trim_start_matches("./");
    if candidate.is_empty() {
        return None;
    }
    if root.join(candidate).is_file() {
        return Some(candidate.to_owned());
    }
    let mut parts: Vec<&str> = candidate.split('/').collect();
    while parts.len() > 1 {
        parts.remove(0);
        let suffix = parts.join("/");
        if root.join(&suffix).is_file() {
            return Some(suffix);
        }
    }
    None
}
