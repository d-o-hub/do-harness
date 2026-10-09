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
    for tag in ["head_sha:", "head_sha=", "head:", "head=", "commit:", "sha:"] {
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

    for token in body.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | ',' | ';' | ':')) {
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
    (len >= 7 && len <= 40) && s.chars().all(|c| c.is_ascii_hexdigit())
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
