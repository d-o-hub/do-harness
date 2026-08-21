//! Skill-eval runner for `do-harness eval`.
//!
//! The structure gate is delegated to skill-creator's `quick_validate.py` —
//! the canonical check is never duplicated in Rust. Skills passing the gate
//! have their `evals/evals.json` fixtures parsed, their optional
//! `evals/walkthrough.sh` executed once, and their prefixed (graded)
//! assertions executed deterministically against the workspace root. One
//! `skill_evals` row is persisted per skill carrying the fraction of graded
//! assertions that passed (`pass_rate`), plus the first graded case's prompt
//! and expected outcome.

use crate::eval_sandbox::Sandbox;

#[cfg(test)]
mod eval_tests;
#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::eval_assert::AssertionGrade;
use crate::eval_walk::WalkRun;

/// Canonical `evals/evals.json` fixture schema.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillEvals {
    /// Fixture skill name; the persisted row uses the directory name.
    #[allow(dead_code)] // schema-required key, not consumed by the runner
    skill_name: String,
    /// Individual evaluation cases.
    evals: Vec<EvalCase>,
}

/// A single evaluation fixture case.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EvalCase {
    /// Stable fixture identifier.
    #[allow(dead_code)] // schema-required key, not consumed by the runner
    id: i64,
    /// Prompt handed to the evaluated skill.
    prompt: String,
    /// Expected outcome of the prompt.
    expected_output: String,
    /// Files the case reads or writes.
    #[allow(dead_code)] // schema-required key, not consumed by the runner
    files: Vec<String>,
    /// Assertions the case must satisfy (prefixed ones are graded).
    assertions: Vec<String>,
}

/// Result of evaluating a single skill directory.
struct SkillReport {
    /// Whether the skill failed the structure gate.
    gate_failed: bool,
    /// Per-skill summary line printed to stdout.
    line: String,
    /// `pass_rate` to persist; `None` when nothing should be persisted.
    pass_rate: Option<f64>,
    /// Number of graded (prefixed) assertions evaluated.
    graded: u32,
    /// Number of graded assertions that passed.
    passed: u32,
    /// Prompt of the first graded case, for richer persistence.
    prompt: Option<String>,
    /// Expected outcome of the first graded case.
    expected_outcome: Option<String>,
}

/// Runs the skill-eval benchmark for every skill under `root/.agents/skills`.
///
/// Each skill directory containing a `SKILL.md` is validated with
/// skill-creator's `quick_validate.py`. When the gate passes and the skill
/// ships `evals/evals.json`, its graded (prefixed) assertions are executed
/// and persisted: one latest-row `skill_evals` entry plus an append-only
/// `skill_eval_runs` record (the improvement trend).
///
/// Graders are tamper-evident: when a blessed baseline exists and the
/// on-disk `walkthrough.sh` / `evals.json` hashes drift, the skill fails
/// until reviewed and re-blessed. A skill's blessed bar floor also fails it
/// when the pass rate drops below `best_ever - tolerance`, even with green
/// assertions.
///
/// With `bless`, a fully green run re-baselines the graders and raises the
/// bar floor to `best_ever - tolerance`.
///
/// # Errors
///
/// Returns an error when a requested skill is not found under
/// `.agents/skills`, when the state database cannot be initialized or
/// written, when a `db:` assertion cannot reach the database, or when any
/// evaluated skill fails the structure gate, grader-drifts, or misses its
/// blessed bar.
pub async fn run_eval(root: &Path, skill: Option<&str>, bless: bool) -> Result<()> {
    let skills_root = root.join(".agents/skills");
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
    for entry in entries {
        let name = entry
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("invalid skill directory name: {}", entry.display()))?
            .to_owned();

        let hashes = crate::eval_integrity::grader_hashes(&skills_root.join(&name))
            .context(format!("failed to hash graders for skill '{name}'"))?;
        let baseline = do_harness_db::get_grader_baseline(&conn, &name).await?;
        if let Some(baseline) = &baseline {
            if !hashes.matches_baseline(baseline) {
                println!(
                    "{name}: grader-DRIFT: graders changed since last bless; review the diff \
                     then run `do-harness eval --bless --skill {name}`"
                );
                invalid.push(name);
                continue;
            }
        }

        // Evaluate the skill in a hermetic temp root so walkthroughs and
        // assertions cannot dirty the caller's repository tree.
        let sandbox = Sandbox::for_skill(root, &entry, &name)?;
        let report = check_skill(
            sandbox.root(),
            sandbox.root().join(".agents/skills").join(&name).as_path(),
            &name,
            &sandbox.gate_script(),
        )
        .await?;
        drop(sandbox);
        println!("{}", report.line);
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
            do_harness_db::insert_skill_eval_run(
                &conn,
                &do_harness_db::NewSkillEvalRun {
                    skill_name: &name,
                    graded: i64::from(report.graded),
                    passed: i64::from(report.passed),
                    pass_rate: Some(pass_rate),
                },
            )
            .await?;
        }

        if bless {
            bless_skill(&conn, &name, &report, &hashes).await?;
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

        if report.gate_failed && !invalid.contains(&name) {
            invalid.push(name);
        }
    }

    if invalid.is_empty() {
        Ok(())
    } else {
        bail!("eval failed for skill(s): {}", invalid.join(", "))
    }
}

/// Blesses a fully green run: re-baselines the graders' hashes and raises the
/// bar floor to `best_ever - tolerance`. A run that is not fully green is not
/// blessable — blessing is the human sign-off that the current graders and
/// results are honest.
async fn bless_skill(
    conn: &do_harness_db::Connection,
    name: &str,
    report: &SkillReport,
    hashes: &crate::eval_integrity::GraderHashes,
) -> Result<()> {
    if report.gate_failed {
        bail!("cannot bless skill '{name}': structure gate failed; fix it and rerun with --bless");
    }
    match (report.graded, report.passed) {
        (0, _) => {
            // No graded assertions: baselining graders is still meaningful
            // (it pins walkthrough.sh + evals.json), but no bar is set.
        }
        (graded, passed) if passed < graded => {
            bail!(
                "cannot bless skill '{name}': {passed}/{graded} assertions green; only fully green runs are blessable"
            );
        }
        _ => {}
    }
    do_harness_db::bless_grader_baseline(conn, name, &hashes.walkthrough_sha, &hashes.specs_sha)
        .await?;
    if report.graded > 0 {
        let best = do_harness_db::max_pass_rate(conn, name).await?;
        if let Some(floor) = crate::eval_integrity::GraderHashes::bar_floor(best) {
            if do_harness_db::raise_skill_bar(conn, name, floor).await? {
                println!("{name}: blessed; bar floor raised to {floor:.2}");
            } else {
                println!("{name}: blessed; bar floor unchanged");
            }
        }
    } else {
        println!("{name}: blessed; no graded assertions so no bar was set");
    }
    Ok(())
}

/// Lists skill directories under `skills_root` that contain a `SKILL.md`,
/// sorted by name.
fn discover_skills(skills_root: &Path) -> Vec<PathBuf> {
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

/// Structure-gate outcome for a skill directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateVerdict {
    /// `quick_validate.py` passed.
    Pass,
    /// The skill is structurally invalid.
    Fail,
    /// The gate could not run: script or `python3` missing. Not a skill
    /// defect; consumer workspaces without skill-creator hit this.
    Unavailable,
}

/// Runs the canonical structure gate for `dir`.
///
/// Returns the verdict plus a message: the last line of the gate's combined
/// stdout and stderr when it ran, or a diagnostic when it could not.
fn run_structure_gate(dir: &Path, gate_script: &Path) -> (GateVerdict, String) {
    if !gate_script.is_file() {
        return (
            GateVerdict::Unavailable,
            format!("quick_validate.py not found at {}", gate_script.display()),
        );
    }
    let Ok(output) = Command::new("python3").arg(gate_script).arg(dir).output() else {
        return (
            GateVerdict::Unavailable,
            "gate could not be executed (python3 missing)".to_owned(),
        );
    };
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    let message = combined
        .lines()
        .last()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let verdict = if output.status.success() {
        GateVerdict::Pass
    } else {
        GateVerdict::Fail
    };
    (verdict, message)
}

/// Validates the structure gate and, when it does not fail, parses and scores
/// the skill's `evals/evals.json` fixtures with deterministic assertions.
async fn check_skill(
    root: &Path,
    dir: &Path,
    name: &str,
    gate_script: &Path,
) -> Result<SkillReport> {
    let empty = || SkillReport {
        gate_failed: false,
        line: String::new(),
        pass_rate: None,
        graded: 0,
        passed: 0,
        prompt: None,
        expected_outcome: None,
    };

    let (verdict, gate_msg) = run_structure_gate(dir, gate_script);
    if verdict == GateVerdict::Fail {
        return Ok(SkillReport {
            gate_failed: true,
            line: format!("{name}: structure=invalid: {gate_msg} evals=skipped"),
            pass_rate: None,
            graded: 0,
            passed: 0,
            prompt: None,
            expected_outcome: None,
        });
    }
    let structure = match verdict {
        GateVerdict::Pass => "ok".to_owned(),
        GateVerdict::Unavailable => format!("unknown (gate unavailable: {gate_msg})"),
        GateVerdict::Fail => unreachable!("handled above"),
    };

    let evals_path = dir.join("evals/evals.json");
    let content = match fs::read_to_string(&evals_path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let mut report = empty();
            report.line = format!("{name}: structure={structure} evals=none");
            return Ok(report);
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", evals_path.display()));
        }
    };

    let parsed = match serde_json::from_str::<SkillEvals>(&content) {
        Ok(parsed) => parsed,
        Err(err) => {
            let mut report = empty();
            report.line = format!("{name}: structure={structure} evals-invalid: {err}");
            return Ok(report);
        }
    };

    let walk = crate::eval_walk::run_walkthrough(dir, root);
    let outcome = grade_skill(&parsed, root, &walk).await?;

    let line = match outcome.pass_rate {
        Some(rate) => format!(
            "{name}: structure={structure} evals={}/{} pass_rate={rate:.2}",
            outcome.passed, outcome.graded
        ),
        None => format!(
            "{name}: structure={structure} evals={}/{}",
            outcome.passed, outcome.graded
        ),
    };
    Ok(SkillReport {
        gate_failed: false,
        line,
        pass_rate: outcome.pass_rate,
        graded: outcome.graded,
        passed: outcome.passed,
        prompt: outcome.prompt,
        expected_outcome: outcome.expected_outcome,
    })
}

/// Grading summary returned by [`grade_skill`].
struct GradeOutcome {
    /// Number of graded (prefixed) assertions that passed.
    passed: u32,
    /// Total number of graded (prefixed) assertions.
    graded: u32,
    /// `passed / graded`, or `None` when there is nothing to grade.
    pass_rate: Option<f64>,
    /// Prompt of the first case contributing a graded assertion.
    prompt: Option<String>,
    /// Expected outcome of the first case contributing a graded assertion.
    expected_outcome: Option<String>,
}

/// Executes every prefixed assertion in `evals` against `root`.
async fn grade_skill(evals: &SkillEvals, root: &Path, walk: &WalkRun) -> Result<GradeOutcome> {
    let mut passed = 0u32;
    let mut graded = 0u32;
    let mut prompt = None;
    let mut expected_outcome = None;

    for case in &evals.evals {
        for spec in &case.assertions {
            if !crate::eval_assert::is_graded(spec) {
                continue; // documentation, excluded from pass_rate
            }
            if prompt.is_none() {
                prompt = Some(case.prompt.clone());
                expected_outcome = Some(case.expected_output.clone());
            }
            graded += 1;
            let grade: AssertionGrade = if walk.present && !walk.success {
                let reason = walk
                    .detail
                    .clone()
                    .unwrap_or_else(|| "walkthrough.sh exited non-zero".to_owned());
                AssertionGrade {
                    passed: false,
                    reason,
                }
            } else {
                crate::eval_assert::grade(root, spec, walk).await?
            };
            if grade.passed {
                passed += 1;
            }
        }
    }

    let pass_rate = (graded > 0).then(|| f64::from(passed) / f64::from(graded));
    Ok(GradeOutcome {
        passed,
        graded,
        pass_rate,
        prompt,
        expected_outcome,
    })
}
