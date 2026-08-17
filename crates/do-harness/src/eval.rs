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
/// and a single `skill_evals` row is persisted with `pass_rate` = passed
/// graded assertions / graded assertions. When `skill` is set, only that
/// skill is evaluated; a missing `SKILL.md` is an error.
///
/// # Errors
///
/// Returns an error when a requested skill is not found under
/// `.agents/skills`, when the state database cannot be initialized or
/// written, when a `db:` assertion cannot reach the database, or when any
/// evaluated skill fails the structure gate.
pub async fn run_eval(root: &Path, skill: Option<&str>) -> Result<()> {
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
                    token_efficiency: None,
                    pass_rate: Some(pass_rate),
                },
            )
            .await?;
        }
        if report.gate_failed {
            invalid.push(name);
        }
    }

    if invalid.is_empty() {
        Ok(())
    } else {
        bail!("structure gate failed for skill(s): {}", invalid.join(", "))
    }
}

/// A hermetic sandbox that mirrors a skill plus skill-creator into a temp dir.
///
/// The temp root becomes the workspace root for the walkthrough and every
/// graded assertion, so residue lands under the temp dir and the caller's
/// repository stays untouched.
struct Sandbox {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Sandbox {
    /// Copies `skill_dir` (SKILL.md + evals) and skill-creator scripts into a
    /// fresh temp root shaped like a harness workspace.
    fn for_skill(real_root: &Path, skill_dir: &Path, name: &str) -> Result<Sandbox> {
        let dir = tempfile::tempdir().context("failed to create eval sandbox")?;
        let root = dir.path().to_path_buf();
        let skills_root = root.join(".agents").join("skills");
        let dest_skill = skills_root.join(name);
        let gate_src = real_root
            .join(".agents")
            .join("skills")
            .join("skill-creator")
            .join("scripts");
        if gate_src.is_dir() {
            let dest_scripts = skills_root.join("skill-creator").join("scripts");
            fs::create_dir_all(&dest_scripts)
                .with_context(|| format!("failed to create {}", dest_scripts.display()))?;
            for entry in fs::read_dir(&gate_src)
                .with_context(|| format!("failed to read {}", gate_src.display()))?
            {
                let entry = entry?;
                let name = entry.file_name();
                fs::copy(entry.path(), dest_scripts.join(&name)).with_context(|| {
                    format!("failed to copy {} into sandbox", entry.path().display())
                })?;
            }
        }
        copy_dir(skill_dir, &dest_skill)?;
        Ok(Sandbox { _dir: dir, root })
    }

    /// Path of the hermetic workspace root.
    fn root(&self) -> &Path {
        &self.root
    }

    /// Path of the copied skill-creator gate script within the sandbox.
    fn gate_script(&self) -> PathBuf {
        self.root
            .join(".agents")
            .join("skills")
            .join("skill-creator")
            .join("scripts")
            .join("quick_validate.py")
    }
}

/// Recursively copies `src` into `dest`.
fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).with_context(|| format!("failed to create {}", dest.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} -> {}", from.display(), to.display())
            })?;
        }
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
        prompt: None,
        expected_outcome: None,
    };

    let (verdict, gate_msg) = run_structure_gate(dir, gate_script);
    if verdict == GateVerdict::Fail {
        return Ok(SkillReport {
            gate_failed: true,
            line: format!("{name}: structure=invalid: {gate_msg} evals=skipped"),
            pass_rate: None,
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
            graded = graded.saturating_add(1);
            let grade: AssertionGrade = if walk.present && !walk.success {
                AssertionGrade {
                    passed: false,
                    reason: "walkthrough.sh exited non-zero".to_owned(),
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
