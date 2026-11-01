//! Human-readable rendering of a review report.

use crate::pr::agent_view::{AgentReviewEnvelope, Representation};
use crate::pr::diff::Change;
use crate::pr::review::{Reduction, ReviewReport};

pub(super) fn print_agent_review(report: &ReviewReport, envelope: &AgentReviewEnvelope) {
    print_review(report);
    let rep = match envelope.representation {
        Representation::Residual => "residual",
        Representation::RawDiff => "raw_diff",
    };
    println!(
        "view agent ({rep}): review_required={} exempt={} skipped={} ({} bytes, ratio {:.3})",
        envelope.review_required_units.len(),
        envelope.exempt_count,
        envelope.skipped_count,
        envelope.measurement.emitted_bytes,
        envelope.measurement.ratio,
    );
}

pub(super) fn print_review(report: &ReviewReport) {
    let cache = if report.cached { "cached" } else { "fresh" };
    let policy = if report.policy.present {
        "present"
    } else {
        "absent"
    };
    match report.pr {
        Some(number) => println!("pr {number}: review ({cache}, policy {policy})"),
        None => println!(
            "{}...{}: review ({cache}, policy {policy})",
            report.base, report.head
        ),
    }
    println!("merge-base {} head {}", report.merge_base, report.head);
    println!(
        "residual: {} unit(s), exempt: {} unit(s), skipped: {} unit(s)",
        report.residual.len(),
        report.exempt.len(),
        report.skipped.len()
    );
    println!(
        "input: {} -> {} bytes ({:.3}, {})",
        report.measurement.t_raw,
        report.measurement.t_res,
        report.measurement.ratio,
        reduction_label(report.measurement.verdict)
    );
    for unit in &report.exempt {
        let anchor = unit.header.as_deref().unwrap_or("-");
        println!(
            "  exempt {} [{}] {}",
            unit.id,
            change_label(unit.change),
            anchor
        );
    }
    for unit in &report.skipped {
        let anchor = unit.header.as_deref().unwrap_or("-");
        println!(
            "  skipped {} [{}] {}",
            unit.id,
            change_label(unit.change),
            anchor
        );
    }
    for claim in &report.false_proven {
        println!("  revoked {}: {}", claim.unit_id, claim.reason);
    }
    for unit in &report.residual {
        let anchor = unit.header.as_deref().unwrap_or("-");
        println!("  {} [{}] {}", unit.id, change_label(unit.change), anchor);
        for line in &unit.lines {
            println!("      {line}");
        }
    }
    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }
}

fn change_label(change: Change) -> &'static str {
    match change {
        Change::Added => "added",
        Change::Modified => "modified",
        Change::Deleted => "deleted",
        Change::Renamed => "renamed",
    }
}

fn reduction_label(reduction: Reduction) -> &'static str {
    match reduction {
        Reduction::Reduced => "reduced",
        Reduction::NoGo => "no-go",
    }
}
