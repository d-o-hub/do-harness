//! Orchestrates a full `do-harness eval` run.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use do_harness_types::EvalMode;

use crate::eval_sandbox::Sandbox;
use crate::report::Format;

use super::agent::{AgentSpec, check_skill_agent};
use super::bless::bless_skill;
use super::fixture::fixture_diagnostics;
use super::grading::{
    GateOutcome, check_skill, check_skill_without, gate_and_parse, load_evals, report_from_outcome,
    skill_words,
};

/// Options for one `do-harness eval` invocation.
///
/// Deliberately a flat options bag (like `VerifyOpts`); the boolean fields
/// are independent CLI switches, not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct EvalOpts<'a> {
    /// Restrict evaluation to this skill directory name.
    pub skill: Option<&'a str>,
    /// Re-baseline graders and ratchet floors on a fully green run.
    pub bless: bool,
    /// List available skills and exit.
    pub list_skills: bool,
    /// Halt on the first failing skill.
    pub fail_fast: bool,
    /// Skip execution (report only).
    pub dry_run: bool,
    /// Output format.
    pub format: Format,
    /// Approver identity recorded with `bless`.
    pub approver: Option<&'a str>,
    /// Skip the without-skill baseline run.
    pub no_lift: bool,
    /// External agent command run once per case instead of the walkthrough.
    pub agent_cmd: Option<&'a str>,
    /// Kill an agent run after this many seconds.
    pub agent_timeout_secs: u64,
    /// Fail skills whose fixture has dataset-quality gaps.
    pub strict_fixtures: bool,
}

/// Runs the skill-eval benchmark for skills under `.agents/skills`.
#[allow(clippy::too_many_lines)]
pub async fn run_eval(root: &Path, opts: EvalOpts<'_>) -> Result<()> {
    let EvalOpts {
        skill,
        bless,
        list_skills,
        fail_fast,
        dry_run,
        format,
        approver,
        no_lift,
        agent_cmd,
        agent_timeout_secs,
        strict_fixtures,
    } = opts;
    let skills_root = root.join(".agents/skills");
    if list_skills {
        let skills = discover_skills(&skills_root);
        for s in &skills {
            if let Some(name) = s.file_name().and_then(|n| n.to_str()) {
                println!("{name}");
            }
        }
        return Ok(());
    }
    let approver = if bless {
        Some(resolve_approver(approver)?)
    } else {
        None
    };

    let entries = match skill {
        Some(name) => {
            let dir = skills_root.join(name);
            if !dir.join("SKILL.md").is_file() {
                bail!("skill '{name}' not found under .agents/skills");
            }
            vec![dir]
        }
        None => discover_skills(&skills_root),
    };

    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut invalid = Vec::new();
    let mut reports_json = Vec::new();

    let mode = if agent_cmd.is_some() {
        EvalMode::Agent
    } else {
        EvalMode::Deterministic
    };
    let agent_spec = agent_cmd.map(|command| AgentSpec {
        command: command.to_owned(),
        timeout: Duration::from_secs(agent_timeout_secs.max(1)),
    });

    for entry in entries {
        let name = entry
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("invalid skill directory name: {}", entry.display()))?
            .to_owned();

        let hashes = crate::eval_integrity::grader_hashes(&skills_root.join(&name))
            .await
            .context(format!("failed to hash graders for skill '{name}'"))?;
        let baseline = do_harness_db::get_grader_baseline(&conn, &name).await?;
        if let Some(baseline) = &baseline {
            if !hashes.matches_baseline(baseline) {
                if bless {
                    println!("{name}: grader-DRIFT: graders changed, re-blessing under --bless");
                } else {
                    println!(
                        "{name}: grader-DRIFT: graders changed since last bless; review the diff \
                         then run `do-harness eval --bless --skill {name}`"
                    );
                    invalid.push(name.clone());
                    if fail_fast {
                        break;
                    }
                    continue;
                }
            }
        }

        if dry_run {
            println!("{name}: dry run, skipped evaluation");
            continue;
        }

        let mut report = if let Some(spec) = &agent_spec {
            let gate_script = skills_root
                .join("skill-creator")
                .join("scripts")
                .join("quick_validate.py");
            match gate_and_parse(&entry, &name, &gate_script).await? {
                GateOutcome::Ready { structure, evals } => {
                    let outcome =
                        check_skill_agent(root, &entry, &name, &evals, spec, false).await?;
                    report_from_outcome(
                        &name,
                        &structure,
                        outcome,
                        skill_words(&entry),
                        fixture_diagnostics(&evals),
                    )
                }
                GateOutcome::Failed(report)
                | GateOutcome::NoEvals(report)
                | GateOutcome::InvalidEvals(report) => report,
            }
        } else {
            let sandbox = Sandbox::for_skill(root, &entry, &name)?;
            let report = check_skill(
                sandbox.root(),
                sandbox.root().join(".agents/skills").join(&name).as_path(),
                &name,
                &sandbox.gate_script(),
            )
            .await?;
            drop(sandbox);
            report
        };

        if !no_lift && !report.gate_failed && report.pass_rate.is_some() {
            let baseline = if let Some(spec) = &agent_spec {
                match load_evals(&entry).await? {
                    Some(evals) => check_skill_agent(root, &entry, &name, &evals, spec, true).await,
                    None => Err(anyhow::anyhow!("evals vanished for '{name}'")),
                }
            } else {
                let bare = Sandbox::for_skill(root, &entry, &name)?;
                bare.strip_guidance(&name)?;
                let bare_dir = bare.root().join(".agents/skills").join(&name);
                let result = check_skill_without(bare.root(), &bare_dir).await;
                drop(bare);
                result
            };
            match baseline {
                Ok(baseline) => {
                    report.without_graded = baseline.graded;
                    report.without_passed = baseline.passed;
                    report.without_pass_rate = baseline.pass_rate;
                    if let (Some(with), Some(without)) = (report.pass_rate, baseline.pass_rate) {
                        report.lift = Some(with - without);
                    }
                    let without_by_dim: std::collections::HashMap<_, _> = baseline
                        .dims
                        .into_iter()
                        .map(|d| (d.dim, d.passed))
                        .collect();
                    // A dimension missing from the baseline graded nothing
                    // there, so its baseline stays unmeasured (None).
                    for dim in &mut report.dims {
                        dim.without_passed = without_by_dim.get(&dim.dim).copied();
                    }
                }
                Err(err) => {
                    eprintln!("warning: without-skill baseline for '{name}' failed: {err:#}");
                }
            }
        }
        finish_line(&mut report, mode);

        if format == Format::Json {
            reports_json.push(report_json(&name, &report, mode));
        } else {
            println!("{}", report.line);
        }

        if !report.fixture_warnings.is_empty() {
            for warning in &report.fixture_warnings {
                if strict_fixtures {
                    println!("{name}: FIXTURE-MISS: {warning}");
                } else {
                    println!("{name}: fixture-WARN: {warning}");
                }
            }
            if strict_fixtures && !invalid.contains(&name) {
                invalid.push(name.clone());
            }
        }

        if let Some(pass_rate) = report.pass_rate {
            do_harness_db::insert_skill_eval(
                &conn,
                &do_harness_db::NewSkillEval {
                    skill_name: &name,
                    prompt: report.prompt.as_deref(),
                    expected_outcome: report.expected_outcome.as_deref(),
                    pass_rate: Some(pass_rate),
                },
            )
            .await?;
            let run_id = do_harness_db::insert_skill_eval_run(
                &conn,
                &do_harness_db::NewSkillEvalRun {
                    skill_name: &name,
                    mode,
                    graded: i64::from(report.graded),
                    passed: i64::from(report.passed),
                    pass_rate: Some(pass_rate),
                    without_pass_rate: report.without_pass_rate,
                    skill_words: Some(report.skill_words),
                    walk_secs: Some(report.walk_secs),
                },
            )
            .await?;
            let dim_rates: Vec<do_harness_db::NewSkillEvalDimRate<'_>> = report
                .dims
                .iter()
                .map(|d| do_harness_db::NewSkillEvalDimRate {
                    dim: d.dim.as_str(),
                    graded: i64::from(d.graded),
                    passed: i64::from(d.passed),
                    without_passed: d.without_passed.map(i64::from),
                })
                .collect();
            do_harness_db::insert_dim_rates(&conn, run_id, &dim_rates).await?;
        }

        if bless {
            let approver = approver.as_deref().unwrap_or("unknown");
            bless_skill(&conn, &name, &report, &hashes, approver).await?;
        } else if let Some(floor) = do_harness_db::get_skill_bar(&conn, &name).await? {
            if let Some(rate) = report.pass_rate {
                if rate < floor {
                    println!(
                        "{name}: BAR-MISS: pass_rate {rate:.2} below blessed floor {floor:.2}"
                    );
                    invalid.push(name.clone());
                }
            }
        }
        if !bless {
            if let Some(floor) = do_harness_db::get_lift_floor(&conn, &name).await? {
                if let Some(lift) = report.lift {
                    if lift < floor {
                        println!(
                            "{name}: LIFT-MISS: lift {lift:+.2} below blessed floor {floor:+.2}"
                        );
                        if !invalid.contains(&name) {
                            invalid.push(name.clone());
                        }
                    }
                }
            }
        }

        if report.gate_failed && !invalid.contains(&name) {
            invalid.push(name.clone());
        }

        if !invalid.is_empty() && fail_fast {
            break;
        }
    }

    if format == Format::Json && !reports_json.is_empty() {
        println!("{}", serde_json::to_string(&reports_json)?);
    }

    if invalid.is_empty() {
        Ok(())
    } else {
        bail!("eval failed for skill(s): {}", invalid.join(", "))
    }
}

/// Appends lift, cost proxies, and the eval mode to the human-readable
/// report line once the baseline has been measured.
fn finish_line(report: &mut super::grading::SkillReport, mode: EvalMode) {
    if report.gate_failed || report.pass_rate.is_none() {
        return;
    }
    let lift = report
        .lift
        .map_or_else(|| "n/a".to_owned(), |lift| format!("{lift:+.2}"));
    let fixture = if report.fixture_warnings.is_empty() {
        "ok"
    } else {
        "warn"
    };
    report.line = format!(
        "{} lift={lift} words={} walk={:.1}s mode={mode} fixture={fixture}",
        report.line, report.skill_words, report.walk_secs
    );
}

/// Machine-readable eval report including mode, lift, dimensions, and cost.
fn report_json(
    name: &str,
    report: &super::grading::SkillReport,
    mode: EvalMode,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "mode": mode.as_str(),
        "line": report.line,
        "pass_rate": report.pass_rate,
        "graded": report.graded,
        "passed": report.passed,
        "gate_failed": report.gate_failed,
        "lift": report.lift,
        "without_pass_rate": report.without_pass_rate,
        "without_graded": report.without_graded,
        "without_passed": report.without_passed,
        "skill_words": report.skill_words,
        "walk_secs": report.walk_secs,
        "fixture_warnings": report.fixture_warnings,
        "dims": report.dims.iter().map(|d| serde_json::json!({
            "dim": d.dim.as_str(),
            "graded": d.graded,
            "passed": d.passed,
            "without_passed": d.without_passed,
        })).collect::<Vec<_>>(),
    })
}

/// Resolves the bless approver: explicit flag, `DO_HARNESS_APPROVER`, then the
/// git user email. An anonymous bless is rejected (fail-closed).
fn resolve_approver(explicit: Option<&str>) -> Result<String> {
    if let Some(value) = explicit.filter(|value| !value.trim().is_empty()) {
        return Ok(value.trim().to_owned());
    }
    if let Ok(value) = std::env::var("DO_HARNESS_APPROVER") {
        if !value.trim().is_empty() {
            return Ok(value.trim().to_owned());
        }
    }
    if let Ok(output) = crate::changes::git_command(Path::new("."))
        .args(["config", "user.email"])
        .output()
    {
        if output.status.success() {
            let email = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !email.is_empty() {
                return Ok(email);
            }
        }
    }
    bail!("--bless requires an approver: pass --approver <name> or set DO_HARNESS_APPROVER")
}

/// Returns skill directories that contain a `SKILL.md`, sorted by path.
#[must_use]
pub(super) fn discover_skills(skills_root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let Ok(entries) = fs::read_dir(skills_root) else {
        return dirs;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if dir.is_dir() && dir.join("SKILL.md").is_file() {
            dirs.push(dir);
        }
    }
    dirs.sort();
    dirs
}
