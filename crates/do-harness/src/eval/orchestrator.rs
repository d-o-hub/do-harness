//! Orchestrates a full `do-harness eval` run.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::eval_sandbox::Sandbox;
use crate::report::Format;

use super::bless::bless_skill;
use super::grading::check_skill;

/// Runs the skill-eval benchmark for skills under `.agents/skills`.
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools
)]
pub async fn run_eval(
    root: &Path,
    skill: Option<&str>,
    bless: bool,
    list_skills: bool,
    fail_fast: bool,
    dry_run: bool,
    format: Format,
    approver: Option<&str>,
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
