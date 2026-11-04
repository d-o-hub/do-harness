//! Reading and rendering the typed CI verdict.

use std::path::Path;

use anyhow::{Context, Result};

use super::{
    CheckBlocker, ChecksReport, MAX_HEAD_ATTEMPTS, PolicySummary, Verdict, classify, load_policy,
};
use crate::CliError;
use crate::pr::gh;
use crate::report::Format;

/// Evaluates the checks for a known head SHA.
///
/// # Errors
///
/// Returns an error when the policy, check runs, or statuses cannot be read.
pub fn evaluate_for_head(
    root: &Path,
    pr: u64,
    head_sha: &str,
    conflicts: bool,
) -> Result<ChecksReport> {
    let policy = load_policy(root)?;
    let runs = gh::check_runs_page(root, head_sha).context("could not read check runs")?;
    let statuses =
        gh::commit_statuses_page(root, head_sha).context("could not read commit statuses")?;

    if usize::try_from(runs.total_count) != Ok(runs.check_runs.len()) {
        return Ok(unknown_report(
            pr,
            head_sha,
            "check_runs_truncated",
            &format!(
                "check-runs API reported total_count={} but {} run(s) were read",
                runs.total_count,
                runs.check_runs.len()
            ),
        ));
    }
    if usize::try_from(statuses.total_count) != Ok(statuses.statuses.len()) {
        return Ok(unknown_report(
            pr,
            head_sha,
            "statuses_truncated",
            &format!(
                "status API reported total_count={} but {} status(es) were read",
                statuses.total_count,
                statuses.statuses.len()
            ),
        ));
    }

    Ok(classify(
        pr,
        head_sha,
        runs.check_runs,
        statuses.statuses,
        &policy,
        conflicts,
    ))
}

/// Evaluates a PR's checks, binding the verdict to a stable head SHA.
///
/// # Errors
///
/// Returns an error when the PR head or the check state cannot be read.
pub fn evaluate(root: &Path, pr: u64) -> Result<ChecksReport> {
    for attempt in 0..MAX_HEAD_ATTEMPTS {
        let head = gh::head_sha(root, pr)?;
        let report = evaluate_for_head(root, pr, &head, false)?;
        let recheck = gh::head_sha(root, pr)?;
        if recheck == head {
            return Ok(report);
        }
        if attempt + 1 == MAX_HEAD_ATTEMPTS {
            return Ok(unknown_report(
                pr,
                &head,
                "head_moved",
                &format!("head moved from {head} to {recheck} during inspection"),
            ));
        }
    }
    Ok(unknown_report(
        pr,
        "",
        "head_unstable",
        "could not obtain a stable head SHA",
    ))
}

fn unknown_report(pr: u64, head_sha: &str, code: &str, message: &str) -> ChecksReport {
    ChecksReport {
        pr,
        head_sha: head_sha.to_owned(),
        verdict: Verdict::Unknown,
        checks: Vec::new(),
        blockers: vec![CheckBlocker {
            code: code.to_owned(),
            message: message.to_owned(),
        }],
        policy: PolicySummary::default(),
    }
}

/// Runs `pr checks` and prints the report.
///
/// # Errors
///
/// Returns [`CliError::Running`] (exit 3) when the state cannot be read and
/// [`CliError::Verify`] (exit 1) for any non-pass verdict.
pub fn run(root: &Path, pr: u64, format: Format) -> Result<(), CliError> {
    let report = match evaluate(root, pr) {
        Ok(report) => report,
        Err(err) => {
            let report = unknown_report(pr, "", "api_unreadable", &format!("{err:#}"));
            print_report(&report, format);
            return Err(CliError::Running(err));
        }
    };
    print_report(&report, format);
    if report.verdict == Verdict::Pass {
        Ok(())
    } else {
        Err(CliError::Verify(anyhow::anyhow!(
            "PR {pr} CI verdict is {}",
            report.verdict.as_str()
        )))
    }
}

fn print_report(report: &ChecksReport, format: Format) {
    match format {
        Format::Json => match serde_json::to_string_pretty(report) {
            Ok(json) => println!("{json}"),
            Err(err) => eprintln!("failed to render CI report: {err}"),
        },
        Format::Text => {
            println!("pr {}: CI verdict: {}", report.pr, report.verdict.as_str());
            println!("  head: {}", report.head_sha);
            for entry in &report.checks {
                println!(
                    "  {} {} {} [{}] {}",
                    entry.verdict.as_str(),
                    entry.kind,
                    entry.name,
                    entry.source,
                    entry.state
                );
            }
            for blocker in &report.blockers {
                println!("  BLOCKER {}: {}", blocker.code, blocker.message);
            }
        }
    }
}
