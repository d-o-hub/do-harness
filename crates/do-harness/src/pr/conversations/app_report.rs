//! Classifying an app's status comment as an informational report (#517).
//!
//! CI vendors post top-level comments — a quality-gate badge, a benchmark table,
//! a coverage summary. Those are *reports about* the merge, not requests to the
//! author, but the inventory could not tell the two apart: a comment was
//! actionable unless it matched one of a few hardcoded English phrasings, so any
//! repository using a vendor outside that list was permanently blocked.
//!
//! The classification here is deliberately structural. A comment is an
//! `app-report` only when **both** conditions hold:
//!
//! 1. **Attributable** — the authoring app also publishes a check run on this
//!    head. The login ↔ check-run slug link is the identity proof, and the
//!    comparison is a whole-slug match, so `app/sonar` cannot borrow
//!    `sonarqubecloud`'s credibility.
//! 2. **No failure vocabulary** — the body carries no failure signal. This veto
//!    is absolute and is checked first, so an attributable app reporting a real
//!    failure stays actionable.
//!
//! Phrasing only selects the *direction* of the verdict; it can never make an
//! unattributable comment informational, and an unrecognised phrasing degrades
//! to "not an app report" rather than to a false pass.
//!
//! # What this never does
//!
//! An `app-report` is informational: it cannot resolve a review thread, cannot
//! dispose of an item, and never mutates anything. Representative code-review
//! apps are named too (see [`REVIEW_APPS`]) and are always treated as
//! actionable, because their report is a review the author must read.

use std::collections::BTreeSet;

/// File that adds app slugs to the attributable set, one per line.
///
/// The check-run link already covers "the app that reports checks on this
/// head". This list exists because that cannot name an app acting on a
/// *different* head: a reused pull request whose current head no longer carries
/// the vendor's run would otherwise lose the classification it had before.
pub const APP_ALLOW_FILE: &str = "app-report-allow.txt";

/// Directory holding the auditable CI policy, relative to the repository root.
const POLICY_DIR: &str = ".git/pr-triage";

/// Apps whose comment is a code review, never an informational report.
///
/// Sonar posts a quality gate; `CodeRabbit` and friends post review findings. The
/// attribution test alone would accept the second kind, because they do publish
/// check runs — and that would turn a bot's change request into an
/// informational note. Both sets are compared as whole slugs.
pub const REVIEW_APPS: [&str; 12] = [
    "coderabbitai",
    "copilot-pull-request-reviewer",
    "cursor",
    "deepsource-io",
    "danger",
    "gitar-bot",
    "graphite-app",
    "greptile-apps",
    "qodo-merge-pro",
    "sourcery-ai",
    "sweep-ai",
    "what-the-diff",
];

/// Substrings that mark a comment as carrying a failure signal.
///
/// Substrings, not words: a vendor writes `**Failed**` inside a table cell and
/// `do not pass` as a phrase. `risk` is absent on purpose — `Coverage risk` is
/// routine report language and blocking it would defeat the classification.
pub const FAILURE_SIGNALS: [&str; 10] = [
    "fail",
    "blocked",
    "vulnerab",
    "error",
    "critical",
    "does not pass",
    "not pass",
    "unsuccessful",
    "regression",
    "breaking",
];

/// Phrases that select the passing direction of a report.
///
/// Recorded as evidence, never as the decision: attribution already decided
/// whether the comment can be informational.
pub const PASS_PHRASES: [&str; 8] = [
    "quality gate passed",
    "quality gate: passed",
    "up to standards",
    "0 issues",
    "no issues found",
    "all checks passed",
    "all modified and coverable lines are covered",
    "benchmark results",
];

/// Why an app-authored comment was or was not treated as an informational
/// report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppReport {
    /// Informational: attributable and carrying no failure signal.
    Informational {
        /// The app slug that attributed the comment (evidence).
        slug: String,
        /// How attribution was established.
        attribution: Attribution,
        /// The passing phrase that was recognised, when one was.
        pass_phrase: Option<String>,
    },
    /// Actionable, with the reason the classification refused.
    Actionable {
        /// The app slug that attributed the comment, when it was attributable.
        slug: Option<String>,
        /// Why it stayed actionable.
        reason: &'static str,
    },
}

impl AppReport {
    /// The classification name recorded in the report and the ledger.
    #[must_use]
    pub fn classification(&self) -> &'static str {
        match self {
            Self::Informational { .. } => "app-report",
            Self::Actionable { .. } => "actionable",
        }
    }

    /// The single-line evidence string recorded alongside the classification.
    #[must_use]
    pub fn evidence(&self) -> String {
        match self {
            Self::Informational {
                slug,
                attribution,
                pass_phrase,
            } => {
                let mut text = format!(
                    "attributable app '{slug}' ({}), no failure vocabulary",
                    attribution.as_str()
                );
                if let Some(phrase) = pass_phrase {
                    text.push_str(", pass phrase '");
                    text.push_str(phrase);
                    text.push('\'');
                }
                text
            }
            Self::Actionable { slug, reason } => match slug {
                Some(slug) => format!("attributable app '{slug}', {reason}"),
                None => (*reason).to_owned(),
            },
        }
    }
}

/// How a comment's author was tied to the head's check runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribution {
    /// `<slug>[bot]` where `slug` publishes a check run on this head.
    CheckRun,
    /// The slug is listed in `app-report-allow.txt`.
    Allowlist,
}

impl Attribution {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CheckRun => "check-run author",
            Self::Allowlist => "allowlisted app",
        }
    }
}

/// The app slugs a set of classified check entries was published by.
///
/// Legacy commit statuses report `source = "status"`, which is not an app and
/// must never attribute a comment.
#[must_use]
pub fn check_run_slugs<'a, I>(sources: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = &'a str>,
{
    sources
        .into_iter()
        .filter(|source| !source.is_empty() && *source != "status")
        .map(str::to_owned)
        .collect()
}

/// Reads the policy-overridable app allowlist.
///
/// A missing file is an empty list. A malformed file is not an error here: the
/// cost of ignoring an addendum is that a comment stays actionable, which is the
/// safe direction, and blocking a snapshot on a comment file would be worse.
#[must_use]
pub fn load_app_allowlist(root: &std::path::Path) -> BTreeSet<String> {
    let path = root.join(POLICY_DIR).join(APP_ALLOW_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeSet::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Strips the `[bot]` suffix and any `app/` prefix from a login.
///
/// Returns `None` for a login that is not an App form at all, so a human account
/// can never be attributed by naming itself after an app.
#[must_use]
pub fn app_slug(login: &str) -> Option<String> {
    let login = login.trim();
    let login = login.strip_prefix("app/").unwrap_or(login);
    let slug = login.strip_suffix("[bot]")?;
    if slug.is_empty() {
        return None;
    }
    Some(slug.to_owned())
}

/// Whether the body carries a failure signal, and which one.
#[must_use]
pub fn failure_signal(body: &str) -> Option<&'static str> {
    let lower = body.to_ascii_lowercase();
    FAILURE_SIGNALS
        .iter()
        .find(|signal| lower.contains(*signal))
        .copied()
}

/// The recognised passing phrase, if the body carries one.
#[must_use]
pub fn pass_phrase(body: &str) -> Option<&'static str> {
    let lower = body.to_ascii_lowercase();
    PASS_PHRASES
        .iter()
        .find(|phrase| lower.contains(*phrase))
        .copied()
}

/// Classifies one top-level comment against the head's check-run apps.
///
/// The failure vocabulary is checked before attribution is consulted, so the
/// returned reason for a failing attributable app is the failure, not the
/// attribution.
#[must_use]
pub fn classify(
    login: &str,
    body: &str,
    check_run_slugs: &BTreeSet<String>,
    allowlist: &BTreeSet<String>,
) -> AppReport {
    let slug = app_slug(login);

    // A representative review app is a review whatever it says.
    if let Some(slug) = &slug {
        if REVIEW_APPS.contains(&slug.as_str()) {
            return AppReport::Actionable {
                slug: Some(slug.clone()),
                reason: "a code-review app's comment is a review, never an informational report",
            };
        }
    }

    // Failure vocabulary wins outright, attributable or not.
    if let Some(signal) = failure_signal(body) {
        let slug = slug.filter(|slug| check_run_slugs.contains(slug) || allowlist.contains(slug));
        return AppReport::Actionable {
            slug,
            reason: match signal {
                "fail" => "the body reports a failure",
                "blocked" => "the body reports a blocked state",
                "vulnerab" => "the body reports a vulnerability",
                "error" => "the body reports an error",
                "critical" => "the body reports a critical finding",
                "regression" => "the body reports a regression",
                "breaking" => "the body reports a breaking change",
                _ => "the body reports that checks did not pass",
            },
        };
    }

    let Some(slug) = slug else {
        // Not an App form at all: a human, or a bot without the suffix.
        return AppReport::Actionable {
            slug: None,
            reason: "the author is not an attributable app",
        };
    };

    let attribution = if check_run_slugs.contains(&slug) {
        Some(Attribution::CheckRun)
    } else if allowlist.contains(&slug) {
        Some(Attribution::Allowlist)
    } else {
        None
    };

    match attribution {
        Some(attribution) => AppReport::Informational {
            slug,
            attribution,
            pass_phrase: pass_phrase(body).map(str::to_owned),
        },
        None => AppReport::Actionable {
            slug: Some(slug),
            reason: "the app publishes no check run on this head and is not allowlisted",
        },
    }
}

#[cfg(test)]
#[path = "app_report_tests.rs"]
mod tests;
