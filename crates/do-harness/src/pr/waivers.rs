//! Patch-coverage residue classification for `pr waivers`.
//!
//! A patch-coverage report mixes genuinely missing tests with residue no plain
//! test can cover, and a reviewer otherwise classifies it by hand on every PR.
//! This module reads a measured lcov report plus the patch set and answers one
//! question per uncovered changed line: waivable by class, or needs a test?
//!
//! The classes come from measured residue in `d-o-hub/rust-self-learning-memory`
//! (see `crates/do-harness/tests/fixtures/waivers/README.md`): logging-macro
//! field expressions (a subscriber evaluates them), fallbacks a prior guard
//! already proved unreachable, and `#[cfg(feature = "…")]` items the measured
//! pipeline never compiled. Anything else is reported as needing a test — the
//! classifier never invents a waiver for a line it cannot justify.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use super::diff;
use super::lcov;
use super::{waiver_classify, waiver_scan};

pub use super::waiver_comment::{
    CodecovStatus, WaiverAuditRecord, is_authorized_role, parse_candidate_waiver, render_markdown,
};

/// Why an uncovered changed line needs no new test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaiverClass {
    /// A logging macro's field expression.
    MacroField,
    /// A fallback a prior guard already proved unreachable.
    GuardedArm,
    /// Inside a `#[cfg(feature = "…")]` item the measured pipeline never compiled.
    FeatureGated,
    /// Everything else: a test is the fix.
    Missing,
}

impl WaiverClass {
    /// Stable kebab-case label, matching the JSON serialization.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::MacroField => "macro-field",
            Self::GuardedArm => "guarded-arm",
            Self::FeatureGated => "feature-gated",
            Self::Missing => "missing",
        }
    }

    /// Whether the class justifies a waiver instead of a test.
    #[must_use]
    pub fn waivable(self) -> bool {
        !matches!(self, Self::Missing)
    }
}

/// One uncovered changed line and its verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineVerdict {
    /// Line number in the head revision.
    pub line: u32,
    /// Classification.
    pub class: WaiverClass,
    /// Why the class applies, in reviewer-facing words.
    pub evidence: String,
}

/// Verdicts for one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileVerdict {
    /// Repository-relative path.
    pub path: String,
    /// Verdicts sorted by line.
    pub lines: Vec<LineVerdict>,
}

/// A changed line that the previous report missed and the current one covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoveredLine {
    /// Repository-relative path.
    pub path: String,
    /// Line number in the head revision.
    pub line: u32,
}

/// Classified patch-coverage residue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// Verdicts per file, sorted by path.
    pub files: Vec<FileVerdict>,
    /// Per-class line totals, keyed by [`WaiverClass::label`].
    pub counts: BTreeMap<String, usize>,
    /// Changed lines that became covered since the previous report.
    pub covered_since: Vec<CoveredLine>,
    /// `SF:` paths that name no file under the root.
    pub unresolved_files: Vec<String>,
    /// Non-fatal diagnostics.
    pub warnings: Vec<String>,
}

impl Report {
    /// Total classified lines.
    #[must_use]
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    /// Lines the classifier marks as needing a test.
    #[must_use]
    pub fn missing(&self) -> usize {
        self.count(WaiverClass::Missing)
    }

    /// Lines the classifier marks as waivable.
    #[must_use]
    pub fn waivable(&self) -> usize {
        self.total() - self.missing()
    }

    /// Lines classified as `class`.
    #[must_use]
    pub fn count(&self, class: WaiverClass) -> usize {
        self.counts.get(class.label()).copied().unwrap_or(0)
    }
}

/// Everything `analyze` reads.
pub struct Inputs<'a> {
    /// Unified-diff text of the patch set.
    pub patch: &'a str,
    /// Measured lcov report.
    pub lcov: &'a str,
    /// Previous lcov report, when "covered since" should be reported.
    pub since: Option<&'a str>,
    /// Workspace root the head sources and lcov paths resolve against.
    pub root: &'a Path,
    /// Prefix stripped from `SF:` paths before resolution.
    pub strip_prefix: Option<&'a str>,
}

/// Classifies the patch-set residue of one report.
#[must_use]
pub fn analyze(inputs: &Inputs<'_>) -> Report {
    let mut report = Report::default();
    let parsed = diff::parse(inputs.patch);
    report.warnings.extend(parsed.warnings.iter().cloned());
    let added = added_lines(&parsed);

    let coverage = lcov::parse(inputs.lcov);
    report.warnings.extend(coverage.warnings.iter().cloned());
    let by_path = resolve_files(
        &coverage,
        inputs.strip_prefix,
        inputs.root,
        &mut report.unresolved_files,
    );

    let sources = read_sources(&added, inputs.root, &mut report.warnings);

    let since = inputs.since.map(lcov::parse);
    let since_by_path = since.as_ref().map(|since| {
        let mut unresolved = Vec::new();
        let resolved = resolve_files(since, inputs.strip_prefix, inputs.root, &mut unresolved);
        for path in unresolved {
            report
                .warnings
                .push(format!("--since path unresolved under the root: {path}"));
        }
        resolved
    });

    let mut candidates: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    let regions: BTreeMap<String, Vec<waiver_scan::FeatureRegion>> = sources
        .iter()
        .map(|(path, source)| (path.clone(), waiver_scan::cfg_feature_regions(source)))
        .collect();
    for (path, lines) in &added {
        if let Some(coverage) = by_path.get(path) {
            for line in lines {
                if coverage.misses(*line) {
                    candidates.entry(path.clone()).or_default().insert(*line);
                }
            }
        }
        // Feature-gated items are absent from the report when the measured
        // pipeline never compiled them, so the patch set is the only evidence
        // that they exist at all. A gated line the report *does* cover needs no
        // waiver, so only unmeasured or uncovered ones are candidates.
        let Some(file_regions) = regions.get(path).filter(|regions| !regions.is_empty()) else {
            continue;
        };
        for line in lines {
            if waiver_scan::features_at(file_regions, *line).is_none() {
                continue;
            }
            let covered = by_path
                .get(path)
                .is_some_and(|coverage| coverage.records(*line) && !coverage.misses(*line));
            if !covered {
                candidates.entry(path.clone()).or_default().insert(*line);
            }
        }
    }

    for (path, lines) in &candidates {
        let Some(source) = sources.get(path) else {
            continue;
        };
        let no_regions = Vec::new();
        let file_regions = regions.get(path).unwrap_or(&no_regions);
        let let_elses = waiver_scan::let_else_spans(source);
        let macros = waiver_scan::macro_spans(source, &waiver_classify::LOG_MACROS);
        let arms = waiver_scan::match_arms(source);
        let verdicts: Vec<LineVerdict> = lines
            .iter()
            .map(|line| {
                waiver_classify::classify_line(
                    source,
                    *line,
                    file_regions,
                    &let_elses,
                    &macros,
                    &arms,
                )
            })
            .collect();
        report.files.push(FileVerdict {
            path: path.clone(),
            lines: verdicts,
        });
    }
    report
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));

    for file in &report.files {
        for verdict in &file.lines {
            *report
                .counts
                .entry(verdict.class.label().to_owned())
                .or_default() += 1;
        }
    }
    report.counts.retain(|_, count| *count > 0);

    if let Some(since_by_path) = since_by_path {
        report.covered_since = covered_since(&added, &by_path, &since_by_path);
    }

    report
}

/// Added line numbers per file, in head-revision coordinates.
fn added_lines(parsed: &diff::Parsed) -> BTreeMap<String, BTreeSet<u32>> {
    let mut added: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for unit in &parsed.units {
        if unit.change == diff::Change::Deleted {
            continue;
        }
        let mut line = u32::try_from(unit.new_start).unwrap_or(0);
        for payload in &unit.lines {
            match payload.as_bytes().first() {
                Some(b'+') => {
                    added.entry(unit.path.clone()).or_default().insert(line);
                    line = line.saturating_add(1);
                }
                Some(b' ') => line = line.saturating_add(1),
                _ => {}
            }
        }
    }
    added
}

/// Maps coverage records onto repository-relative paths.
fn resolve_files(
    coverage: &lcov::Report,
    strip_prefix: Option<&str>,
    root: &Path,
    unresolved: &mut Vec<String>,
) -> BTreeMap<String, lcov::FileCoverage> {
    let mut by_path: BTreeMap<String, lcov::FileCoverage> = BTreeMap::new();
    for (raw, file) in &coverage.files {
        let Some(path) = lcov::resolve_path(raw, strip_prefix, root) else {
            unresolved.push(raw.clone());
            continue;
        };
        let entry = by_path.entry(path).or_default();
        entry.recorded.extend(&file.recorded);
        entry.uncovered.extend(&file.uncovered);
    }
    unresolved.sort();
    unresolved.dedup();
    by_path
}

/// Reads the head source of every file with added lines.
fn read_sources(
    added: &BTreeMap<String, BTreeSet<u32>>,
    root: &Path,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, Vec<String>> {
    let mut sources = BTreeMap::new();
    for path in added.keys() {
        match std::fs::read_to_string(root.join(path)) {
            Ok(text) => {
                sources.insert(path.clone(), text.lines().map(str::to_owned).collect());
            }
            Err(err) => warnings.push(format!("{path}: head source unreadable ({err})")),
        }
    }
    sources
}

/// Changed lines the previous report missed and the current one covers.
fn covered_since(
    added: &BTreeMap<String, BTreeSet<u32>>,
    current: &BTreeMap<String, lcov::FileCoverage>,
    previous: &BTreeMap<String, lcov::FileCoverage>,
) -> Vec<CoveredLine> {
    let mut covered = Vec::new();
    for (path, lines) in added {
        let (Some(this_time), Some(last_time)) = (current.get(path), previous.get(path)) else {
            continue;
        };
        for line in lines {
            if last_time.misses(*line) && !this_time.misses(*line) && this_time.records(*line) {
                covered.push(CoveredLine {
                    path: path.clone(),
                    line: *line,
                });
            }
        }
    }
    covered.sort_by(|left, right| left.path.cmp(&right.path).then(left.line.cmp(&right.line)));
    covered
}
