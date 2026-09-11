//! Human and machine-readable reporting for `do-harness init`.

use super::{BaselineState, CandidateStatus, InitReport};
use std::path::Path;

/// Prints the init report in the requested format.
///
/// Text mode is a bootstrap narrative (detected → candidates → generated →
/// initial verification); JSON mode serializes the report for tooling.
pub fn print_report(report: &InitReport, root: &Path, format: crate::report::Format) {
    use crate::report::Format;

    if format == Format::Json {
        match serde_json::to_writer_pretty(std::io::stdout(), report) {
            Ok(()) => println!(),
            Err(err) => eprintln!("error: failed to serialize report: {err}"),
        }
        return;
    }

    println!("Initialized do-harness workspace in {}", root.display());
    println!("Detected:");
    if report.detected.is_empty() {
        println!("  (no repository markers)");
    }
    for finding in &report.detected {
        println!("  {finding}");
    }
    println!("Candidate development signals:");
    if report.candidates.is_empty() {
        println!("  (none; generic pack ships zero sensors)");
    }
    for candidate in &report.candidates {
        let status = match candidate.status {
            CandidateStatus::Pass => "PASS",
            CandidateStatus::Degraded => "DEGRADED",
            CandidateStatus::Missing => "MISSING",
        };
        let suffix = if candidate.included { "" } else { " (omitted)" };
        if candidate.detail.is_empty() {
            println!("  {:<10} {status}{suffix}", candidate.name);
        } else {
            println!(
                "  {:<10} {status}{suffix} — {}",
                candidate.name, candidate.detail
            );
        }
    }
    println!("Generated:");
    for path in &report.written {
        println!("  wrote {path}");
    }
    for path in &report.skipped {
        println!("  skipped {path} (exists; re-run with --force to overwrite)");
    }
    if report.seeded > 0 {
        println!("  seeded {} invariants", report.seeded);
    }
    if let Some(baseline) = &report.baseline {
        let state = match baseline.state {
            BaselineState::Green => "GREEN",
            BaselineState::Red => "RED",
            BaselineState::Vacuous => "VACUOUS (no sensors; not evidence)",
        };
        println!("Initial verification:");
        println!("  {state}");
        if !baseline.failed.is_empty() {
            println!("  failed: {}", baseline.failed.join(", "));
        }
        for failure in &baseline.failures {
            println!("  --- {} ---", failure.name);
            for line in failure.detail.lines() {
                println!("      {line}");
            }
        }
    }
    println!();
    println!("Next steps:");
    println!("  do-harness hook install   # wire git hooks");
    println!("  do-harness list           # show the configured sensors");
    println!("  do-harness status --set verification   # evidence freshness");
}
