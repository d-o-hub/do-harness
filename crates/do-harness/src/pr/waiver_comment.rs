//! Rendering for the patch-coverage waiver report.
//!
//! The text output is the artifact the command exists for: a comment a reviewer
//! can paste into a pull request, with the per-class evidence and the per-file
//! counts that make the waiver auditable.

use std::fmt::Write as _;

use super::waivers::{LineVerdict, Report};

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
