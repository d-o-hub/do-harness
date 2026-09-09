//! Skill-eval runner for `do-harness eval`.

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
use crate::report::Format;

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillEvals {
    #[allow(dead_code)]
    skill_name: String,
    evals: Vec<EvalCase>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EvalCase {
    #[allow(dead_code)]
    id: i64,
    prompt: String,
    expected_output: String,
    #[allow(dead_code)]
    files: Vec<String>,
    assertions: Vec<String>,
}

struct SkillReport {
    gate_failed: bool,
    line: String,
    pass_rate: Option<f64>,
    graded: u32,
    passed: u32,
    prompt: Option<String>,
    expected_outcome: Option<String>,
}

/// Runs the skill-eval benchmark for skills under `.agents/skills`.
#[allow(clippy::too_many_lines, clippy::fn_params_excessive_bools)]
pub async fn run_eval(
    root: &Path,
    skill: Option<&str>,
    bless: bool,
    list_skills: bool,
    fail_fast: bool,
    dry_run: bool,
    format: Format,
) -> Result<()> {
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

        let sandbox = Sandbox::for_skill(root, &entry, &name)?;
        let report = check_skill(
            sandbox.root(),
            sandbox.root().join(".agents/skills").join(&name).as_path(),
            &name,
            &sandbox.gate_script(),
        )
        .await?;
        drop(sandbox);

        if format == Format::Json {
            reports_json.push(serde_json::json!({
                "name": name,
                "line": report.line,
                "pass_rate": report.pass_rate,
                "graded": report.graded,
                "passed": report.passed,
                "gate_failed": report.gate_failed,
            }));
        } else {
            println!("{}", report.line);
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
        (0, _) => {}
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateVerdict {
    Pass,
    Fail,
    Unavailable,
}

fn run_structure_gate(dir: &Path, gate_script: &Path) -> (GateVerdict, String) {
    if !gate_script.is_file() {
        return (
            GateVerdict::Unavailable,
            format!("quick_validate.py not found at {}", gate_script.display()),
        );
    }
    let Ok(output) = Command::new("python3").arg(gate_script).arg(dir).output() else {
        eprintln!("warning: python3 is missing or unavailable; structure gate skipped");
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

struct GradeOutcome {
    passed: u32,
    graded: u32,
    pass_rate: Option<f64>,
    prompt: Option<String>,
    expected_outcome: Option<String>,
}

async fn grade_skill(evals: &SkillEvals, root: &Path, walk: &WalkRun) -> Result<GradeOutcome> {
    let mut passed = 0u32;
    let mut graded = 0u32;
    let mut prompt = None;
    let mut expected_outcome = None;

    for case in &evals.evals {
        for spec in &case.assertions {
            if !crate::eval_assert::is_graded(spec) {
                continue;
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
