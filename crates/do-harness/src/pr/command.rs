//! CLI-facing PR analysis: target resolution, verdict rendering, exit codes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::CliError;
use crate::changes::git_command;
use crate::cli::PrAction;
use crate::report::Format;

use super::diff::Change;
use super::gh;
use super::no_effect::{self, Effect};
use super::review::{self, Reduction, ReviewReport};
use super::waivers;

/// What to analyze.
#[derive(Debug, Clone)]
pub enum Target {
    /// A pull request resolved through `gh`.
    Pr(u64),
    /// Explicit local revisions.
    Range {
        /// Base revision.
        base: String,
        /// Head revision.
        head: String,
    },
}

#[derive(Debug, Serialize)]
struct Report {
    schema_version: u32,
    mode: &'static str,
    pr: Option<u64>,
    base: String,
    head: String,
    effective_change: bool,
    method: &'static str,
}

/// Resolves the workspace root for `pr`: harness root first, git work-tree
/// fallback, so the command works in any repository.
///
/// # Errors
///
/// Returns an error when the explicit root is not a directory or neither a
/// harness root nor a git work tree can be found.
pub fn resolve_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if !path.is_dir() {
            bail!("root is not a directory: {}", path.display());
        }
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    if let Ok(root) = do_harness_db::find_harness_root(&cwd) {
        return Ok(root);
    }
    let output = crate::changes::git_command(&cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to run git rev-parse --show-toplevel")?;
    if !output.status.success() {
        bail!(
            "no harness root or git work tree found under {}",
            cwd.display()
        );
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        bail!("git rev-parse --show-toplevel returned no path");
    }
    Ok(PathBuf::from(path))
}

/// Dispatches a `pr` action.
///
/// # Errors
///
/// Returns [`CliError::Usage`] for invalid argument combinations and
/// [`CliError::Verify`] when the verdict cannot be determined; an analysis
/// error is never reported as "no effect".
pub fn run(root: &Path, action: PrAction) -> Result<(), CliError> {
    match action {
        PrAction::NoEffect {
            pr,
            base,
            head,
            format,
        } => {
            let target = target_from(pr, base, head)?;
            run_no_effect(root, target, format).map_err(CliError::Verify)
        }
        PrAction::Review {
            pr,
            base,
            head,
            recompute,
            format,
        } => {
            let target = target_from(pr, base, head)?;
            run_review(root, &target, recompute, format).map_err(CliError::Verify)
        }
        PrAction::Ready { pr, format } => super::readiness::run(root, pr, format),
        PrAction::Waivers {
            pr,
            base,
            head,
            patch,
            lcov,
            since,
            strip_prefix,
            format,
        } => run_waivers(
            root,
            &WaiversArgs {
                pr,
                base,
                head,
                patch,
                lcov,
                since,
                strip_prefix,
                format,
            },
        ),
        PrAction::External(args) => {
            if let Some(first) = args.first() {
                if let Ok(pr) = first.parse::<u64>() {
                    let format = if args
                        .iter()
                        .any(|a| a == "--json" || a == "json" || a == "--format=json")
                    {
                        Format::Json
                    } else {
                        Format::Text
                    };
                    return super::readiness::run(root, pr, format);
                }
            }
            Err(CliError::Usage(anyhow::anyhow!(
                "unknown pr subcommand or invalid PR number: {}",
                args.join(" ")
            )))
        }
    }
}

/// Resolves exclusive PR or revision-range targeting.
fn target_from(
    pr: Option<u64>,
    base: Option<String>,
    head: Option<String>,
) -> Result<Target, CliError> {
    match (pr, base, head) {
        (Some(number), None, None) => Ok(Target::Pr(number)),
        (None, Some(base), Some(head)) => Ok(Target::Range { base, head }),
        _ => Err(CliError::Usage(anyhow::anyhow!(
            "provide a PR number or both --base and --head"
        ))),
    }
}

fn run_no_effect(root: &Path, target: Target, format: Format) -> Result<()> {
    let report = analyze(root, target)?;
    if matches!(format, Format::Json) {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let verdict = if report.effective_change {
            "has-effect"
        } else {
            "no-effect"
        };
        match report.pr {
            Some(number) => println!(
                "pr {number}: {verdict} (base {} head {}, {})",
                report.base, report.head, report.method
            ),
            None => println!(
                "{verdict} (base {} head {}, {})",
                report.base, report.head, report.method
            ),
        }
    }
    Ok(())
}

fn run_review(root: &Path, target: &Target, recompute: bool, format: Format) -> Result<()> {
    let report = review::analyze(root, target, recompute)?;
    if matches!(format, Format::Json) {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_review(&report);
    }
    Ok(())
}

fn print_review(report: &ReviewReport) {
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

fn analyze(root: &Path, target: Target) -> Result<Report> {
    match target {
        Target::Range { base, head } => {
            let effect = no_effect::effective_change(root, &base, &head)
                .with_context(|| format!("cannot compare {base}...{head}"))?;
            Ok(Report {
                schema_version: 1,
                mode: "range",
                pr: None,
                base,
                head,
                effective_change: effect.has_effect(),
                method: "git",
            })
        }
        Target::Pr(number) => {
            let view = gh::view(root, number)?;
            let number = view.number;
            let base = view.base_ref_name;
            let head = view.head_ref_oid;
            if let Some(effect) = local_effect(root, &base, &head) {
                return Ok(Report {
                    schema_version: 1,
                    mode: "pr",
                    pr: Some(number),
                    base,
                    head,
                    effective_change: effect.has_effect(),
                    method: "git",
                });
            }
            let changed = gh::compare_file_count(root, &base, &head)
                .with_context(|| format!("cannot determine effective change for PR {number}"))?;
            Ok(Report {
                schema_version: 1,
                mode: "pr",
                pr: Some(number),
                base,
                head,
                effective_change: changed > 0,
                method: "compare-api",
            })
        }
    }
}

fn local_effect(root: &Path, base: &str, head: &str) -> Option<Effect> {
    if !no_effect::rev_exists(root, head) {
        return None;
    }
    for candidate in no_effect::base_refs(base) {
        if no_effect::rev_exists(root, &candidate) {
            if let Ok(effect) = no_effect::effective_change(root, &candidate, head) {
                return Some(effect);
            }
        }
    }
    None
}

/// `pr waivers` inputs as parsed from the CLI.
pub struct WaiversArgs {
    /// Pull request number; resolves base and head through `gh`.
    pub pr: Option<u64>,
    /// Base revision (local mode).
    pub base: Option<String>,
    /// Head revision (local mode).
    pub head: Option<String>,
    /// Patch file instead of a revision range.
    pub patch: Option<PathBuf>,
    /// Measured lcov report.
    pub lcov: PathBuf,
    /// Previous lcov report.
    pub since: Option<PathBuf>,
    /// Prefix stripped from `SF:` paths.
    pub strip_prefix: Option<String>,
    /// Output format.
    pub format: Format,
}

/// Classifies patch-coverage residue and prints the review comment.
///
/// An unreadable input file is a usage error (exit 2); a patch that cannot be
/// computed is an analysis failure (exit 1). A completed analysis exits 0 and
/// leaves the verdict to the caller: `counts.missing` in the JSON report is the
/// number of changed lines a test still has to cover.
fn run_waivers(root: &Path, args: &WaiversArgs) -> Result<(), CliError> {
    let patch = if let Some(path) = &args.patch {
        read_input(path, "patch")?
    } else {
        let target = target_from(args.pr, args.base.clone(), args.head.clone())?;
        patch_text(root, &target).map_err(CliError::Verify)?
    };
    let lcov = read_input(&args.lcov, "lcov report")?;
    let since = args
        .since
        .as_ref()
        .map(|path| read_input(path, "previous lcov report"))
        .transpose()?;
    let report = waivers::analyze(&waivers::Inputs {
        patch: &patch,
        lcov: &lcov,
        since: since.as_deref(),
        root,
        strip_prefix: args.strip_prefix.as_deref(),
    });
    if matches!(args.format, Format::Json) {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| CliError::Verify(anyhow::anyhow!("cannot serialize report: {err}")))?;
        println!("{json}");
    } else {
        print!("{}", waivers::render_markdown(&report));
    }
    Ok(())
}

/// Reads a CLI input file, reporting an unreadable file as a usage error.
fn read_input(path: &Path, what: &str) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(|err| {
        CliError::Usage(anyhow::anyhow!(
            "cannot read {what} {}: {err}",
            path.display()
        ))
    })
}

/// Unified-diff text for a target.
fn patch_text(root: &Path, target: &Target) -> Result<String> {
    match target {
        Target::Range { base, head } => {
            let merge_base = no_effect::merge_base(root, base, head)
                .with_context(|| format!("cannot resolve the merge base of {base} and {head}"))?;
            local_diff(root, &merge_base, head)
        }
        Target::Pr(number) => gh::diff(root, *number),
    }
}

/// `git diff` text between two revisions, with the flags the review input uses.
fn local_diff(root: &Path, base: &str, head: &str) -> Result<String> {
    let output = git_command(root)
        .args([
            "diff",
            "--no-color",
            "--find-renames",
            "--unified=3",
            base,
            head,
            "--",
        ])
        .output()
        .context("failed to run git diff")?;
    if !output.status.success() {
        bail!(
            "git diff {base} {head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
