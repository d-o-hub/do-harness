//! Rendering for the patch-coverage waiver report.
//!
//! The text output is the artifact the command exists for: a comment a reviewer
//! can paste into a pull request, with the per-class evidence and the per-file
//! counts that make the waiver auditable.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::waivers::{LineVerdict, Report};

/// Status of Codecov coverage concern evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodecovStatus {
    /// No coverage gap reported in the latest Codecov report.
    NoConcern,
    /// Explicitly waived by an authorized reviewer for current head SHA.
    Waived,
    /// Coverage concern exists and is not validly waived for current head SHA.
    Actionable,
}

/// Digest of Codecov report status on the PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecovSummary {
    pub status: CodecovStatus,
    pub is_actionable: bool,
    pub waived: bool,
    pub patch_coverage: Option<f64>,
    pub missing_lines: Option<u64>,
    pub excerpt: String,
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiver_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiver_author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiver_finding_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiver_head_sha: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub audit_waivers: Vec<WaiverAuditRecord>,
}

/// Audit record of an evaluated waiver comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaiverAuditRecord {
    pub author: String,
    pub head_sha: String,
    pub finding_ref: String,
    pub reason: String,
    pub authorized: bool,
    pub is_stale: bool,
}

/// Candidate waiver parsed from a PR issue comment body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateWaiver {
    pub author: String,
    pub head_sha: Option<String>,
    pub finding_ref: Option<String>,
    pub reason: String,
}

/// Returns true if `role` grants repository write/admin authority to waive policy.
#[must_use]
pub fn is_authorized_role(role: &str) -> bool {
    let lower = role.to_ascii_lowercase();
    matches!(lower.as_str(), "admin" | "write" | "maintain" | "owner")
}

/// Parses explicit waiver details from a PR comment.
#[must_use]
pub fn parse_candidate_waiver(comment: &super::gh::Comment) -> Option<CandidateWaiver> {
    let user = comment.user.as_ref()?;
    let author = user.login.trim();
    if author.is_empty()
        || super::readiness::is_bot_login(author)
        || super::readiness::is_codecov_author(author)
    {
        return None;
    }

    let lowered = comment.body.to_ascii_lowercase();
    if !lowered.contains("waiver") && !lowered.contains("waive") {
        return None;
    }

    let head_sha = extract_head_sha(&comment.body);
    let finding_ref = extract_finding_ref(&comment.body);
    let reason = extract_reason(&comment.body);

    Some(CandidateWaiver {
        author: author.to_owned(),
        head_sha,
        finding_ref,
        reason,
    })
}

fn extract_head_sha(body: &str) -> Option<String> {
    let lowered = body.to_ascii_lowercase();
    for tag in [
        "head_sha:",
        "head_sha=",
        "head:",
        "head=",
        "commit:",
        "sha:",
    ] {
        if let Some(idx) = lowered.find(tag) {
            let rest = body[idx + tag.len()..].trim_start();
            let word = rest
                .split(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == ']')
                .next()
                .unwrap_or("");
            if is_hex_sha(word) {
                return Some(word.to_owned());
            }
        }
    }

    for token in body
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | ',' | ';' | ':'))
    {
        let clean = token.trim();
        if clean.len() == 40 && clean.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(clean.to_owned());
        }
    }

    for token in body.split_whitespace() {
        let clean = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if clean.len() >= 7 && clean.len() <= 40 && clean.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(clean.to_owned());
        }
    }

    None
}

fn extract_finding_ref(body: &str) -> Option<String> {
    let lowered = body.to_ascii_lowercase();
    for tag in ["finding:", "finding=", "finding_ref:", "finding_ref="] {
        if let Some(idx) = lowered.find(tag) {
            let rest = body[idx + tag.len()..].trim_start();
            let word = rest
                .split(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == ']')
                .next()
                .unwrap_or("");
            if !word.is_empty() {
                return Some(word.to_owned());
            }
        }
    }

    let candidates = [
        "codecov",
        "patch coverage",
        "missing coverage",
        "patch-coverage",
        "coverage",
        "macro-field",
        "guarded-arm",
        "feature-gated",
        "missing",
    ];

    for candidate in candidates {
        if lowered.contains(candidate) {
            return Some((*candidate).to_owned());
        }
    }

    None
}

fn extract_reason(body: &str) -> String {
    let lowered = body.to_ascii_lowercase();
    if let Some(idx) = lowered.find("):") {
        let rest = body[idx + 2..].trim();
        if !rest.is_empty() {
            let first_line = rest.lines().next().unwrap_or(rest).trim();
            return first_line.to_owned();
        }
    }
    for prefix in ["waiver:", "waive:", "reason:"] {
        if let Some(idx) = lowered.find(prefix) {
            let rest = body[idx + prefix.len()..].trim();
            if !rest.is_empty() {
                let first_line = rest.lines().next().unwrap_or(rest).trim();
                return first_line.to_owned();
            }
        }
    }
    body.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn is_hex_sha(s: &str) -> bool {
    let len = s.len();
    (7..=40).contains(&len) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Evaluates Codecov reports and candidate waivers for PR readiness.
#[allow(clippy::too_many_lines)]
pub fn evaluate_codecov(
    root: &std::path::Path,
    comments: &[super::gh::Comment],
    head_ref_oid: &str,
    blockers: &mut Vec<String>,
    actionable_comments: &mut Vec<super::readiness::ActionableComment>,
) -> Option<CodecovSummary> {
    let latest_codecov = comments.iter().rev().find(|c| {
        let author = c.user.as_ref().map_or("", |u| u.login.as_str());
        super::readiness::is_codecov_author(author)
    })?;

    let author = latest_codecov
        .user
        .as_ref()
        .map_or("codecov", |u| u.login.as_str());
    let patch_cov = parse_patch_coverage(&latest_codecov.body);
    let missing_lines = parse_missing_lines(&latest_codecov.body);
    let excerpt = first_line(&latest_codecov.body, 80);

    let has_gap = latest_codecov.body.contains("missing coverage")
        || latest_codecov.body.contains("Patch coverage is 0")
        || latest_codecov.body.contains("Decreases by")
        || missing_lines.is_some_and(|m| m > 0);

    if has_gap {
        let mut audit_waivers = Vec::new();
        let mut valid_waiver = None;

        for c in comments {
            if let Some(candidate) = parse_candidate_waiver(c) {
                let head_sha = candidate.head_sha.clone().unwrap_or_default();
                let is_stale = head_sha.is_empty()
                    || (!head_ref_oid.is_empty()
                        && !head_ref_oid
                            .to_ascii_lowercase()
                            .starts_with(&head_sha.to_ascii_lowercase())
                        && !head_sha
                            .to_ascii_lowercase()
                            .starts_with(&head_ref_oid.to_ascii_lowercase()));

                let perm = super::gh::user_permission(root, &candidate.author).unwrap_or_default();
                let authorized = is_authorized_role(&perm);

                let finding_ref = candidate.finding_ref.clone().unwrap_or_default();
                let finding_lower = finding_ref.to_ascii_lowercase();
                let finding_matches = finding_lower.contains("codecov")
                    || finding_lower.contains("coverage")
                    || finding_lower.contains("patch")
                    || finding_lower.contains("missing")
                    || finding_lower.contains("macro-field")
                    || finding_lower.contains("guarded-arm")
                    || finding_lower.contains("feature-gated");

                audit_waivers.push(WaiverAuditRecord {
                    author: candidate.author.clone(),
                    head_sha: head_sha.clone(),
                    finding_ref: finding_ref.clone(),
                    reason: candidate.reason.clone(),
                    authorized,
                    is_stale,
                });

                if finding_matches
                    && !is_stale
                    && authorized
                    && !candidate.reason.trim().is_empty()
                    && valid_waiver.is_none()
                {
                    valid_waiver = Some((candidate, head_sha, finding_ref));
                }
            }
        }

        if let Some((waiver, matched_sha, matched_finding)) = valid_waiver {
            Some(CodecovSummary {
                status: CodecovStatus::Waived,
                is_actionable: false,
                waived: true,
                patch_coverage: patch_cov,
                missing_lines,
                excerpt,
                url: latest_codecov.html_url.clone(),
                waiver_reason: Some(waiver.reason),
                waiver_author: Some(waiver.author),
                waiver_finding_ref: Some(matched_finding),
                waiver_head_sha: Some(matched_sha),
                audit_waivers,
            })
        } else {
            let cov_text = patch_cov.map_or_else(
                || "uncovered lines".to_owned(),
                |p| format!("patch coverage {p:.2}%"),
            );
            blockers.push(format!(
                "unanswered Codecov comment reporting missing coverage ({cov_text})"
            ));
            actionable_comments.push(super::readiness::ActionableComment {
                id: latest_codecov.id,
                author: author.to_owned(),
                excerpt: excerpt.clone(),
                url: latest_codecov.html_url.clone(),
            });

            Some(CodecovSummary {
                status: CodecovStatus::Actionable,
                is_actionable: true,
                waived: false,
                patch_coverage: patch_cov,
                missing_lines,
                excerpt,
                url: latest_codecov.html_url.clone(),
                waiver_reason: None,
                waiver_author: None,
                waiver_finding_ref: None,
                waiver_head_sha: None,
                audit_waivers,
            })
        }
    } else {
        Some(CodecovSummary {
            status: CodecovStatus::NoConcern,
            is_actionable: false,
            waived: false,
            patch_coverage: patch_cov,
            missing_lines,
            excerpt,
            url: latest_codecov.html_url.clone(),
            waiver_reason: None,
            waiver_author: None,
            waiver_finding_ref: None,
            waiver_head_sha: None,
            audit_waivers: Vec::new(),
        })
    }
}

pub fn first_line(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.chars().count() > max {
        let mut excerpt: String = line.chars().take(max).collect();
        excerpt.push_str("...");
        excerpt
    } else {
        line.to_owned()
    }
}

pub fn parse_patch_coverage(body: &str) -> Option<f64> {
    let lower = body.to_ascii_lowercase();
    let idx = lower.find("patch coverage")?;
    let rest = &body[idx..];
    let pct_idx = rest.find('%')?;
    let words = &rest[..pct_idx];
    words.split_whitespace().last()?.parse::<f64>().ok()
}

pub fn parse_missing_lines(body: &str) -> Option<u64> {
    let lower = body.to_ascii_lowercase();
    let idx = lower.find("lines in your changes missing coverage")?;
    let prefix = &body[..idx];
    let count_word = prefix.split_whitespace().last()?;
    count_word.parse::<u64>().ok()
}

/// Renders the report as the paste-ready review comment.
#[must_use]
pub fn render_markdown(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("### Patch-coverage residue\n\n");
    if report.total() == 0 {
        out.push_str(
            "No uncovered changed lines in this patch set: every measured changed line is covered.\n",
        );
    } else {
        let _ = writeln!(
            out,
            "{} uncovered changed line(s) across {} file(s): {} waivable by class, {} need a test.\n",
            report.total(),
            report.files.len(),
            report.waivable(),
            report.missing(),
        );
        push_table(&mut out, report, true);
        push_table(&mut out, report, false);
    }

    if !report.covered_since.is_empty() {
        out.push_str(
            "\n**Covered since the previous report**\n\n| file | line(s) |\n| --- | --- |\n",
        );
        for line in &report.covered_since {
            let _ = writeln!(out, "| `{}` | {} |", line.path, line.line);
        }
    }

    if report.total() > 0 {
        out.push_str("\n**Per file**\n\n| file | waivable | needs a test |\n| --- | --- | --- |\n");
        for file in &report.files {
            let waivable = file
                .lines
                .iter()
                .filter(|line| line.class.waivable())
                .count();
            let missing = file.lines.len() - waivable;
            let _ = writeln!(out, "| `{}` | {} | {} |", file.path, waivable, missing);
        }
    }

    if !report.unresolved_files.is_empty() {
        out.push_str("\n**Unresolved `SF:` paths** (no file under the root):\n");
        for path in &report.unresolved_files {
            let _ = writeln!(out, "- `{path}`");
        }
    }
    for warning in &report.warnings {
        let _ = writeln!(out, "\n> warning: {warning}");
    }
    out
}

/// Appends the waivable or the missing-lines table.
fn push_table(out: &mut String, report: &Report, waivable: bool) {
    let rows: Vec<(&str, &LineVerdict)> = report
        .files
        .iter()
        .flat_map(|file| {
            file.lines
                .iter()
                .map(move |line| (file.path.as_str(), line))
        })
        .filter(|(_, line)| line.class.waivable() == waivable)
        .collect();
    if rows.is_empty() {
        return;
    }
    let title = if waivable {
        "\n**Waivable by class**\n\n"
    } else {
        "\n**Needs a test**\n\n"
    };
    out.push_str(title);
    out.push_str("| file | line(s) | class | evidence |\n| --- | --- | --- | --- |\n");
    let mut index = 0;
    while index < rows.len() {
        let (file, row) = rows[index];
        let mut end = index;
        while end + 1 < rows.len() {
            let (next_file, next) = rows[end + 1];
            if next_file != file
                || next.line != rows[end].1.line + 1
                || next.class != rows[end].1.class
                || next.evidence != rows[end].1.evidence
            {
                break;
            }
            end += 1;
        }
        let span = if end > index {
            format!("{}-{}", row.line, rows[end].1.line)
        } else {
            row.line.to_string()
        };
        let _ = writeln!(
            out,
            "| `{file}` | {span} | {} | {} |",
            row.class.label(),
            row.evidence
        );
        index = end + 1;
    }
}
