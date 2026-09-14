//! Every run grades twice: once with the skill installed and once with the
//! guidance payload (`SKILL.md` + `references/`) stripped. The delta is the
//! Skill Lift in points. Cases carry an optional `dim` (one of the five
//! [`do_harness_types::EvalDim`] wire names, defaulting to `effectiveness`)
//! so lift also breaks down per dimension instead of hiding behind the
//! aggregate pass rate.
//!
//! Self-referential walkthroughs — those that invoke the skill body as the
//! subject under test (e.g. validating the skill copy itself, or distilling
//! into the skill itself) — fail the without-run by construction and report
//! lift `+1.00`. That is honest (every check depends on the skill existing)
//! but uninformative about guidance value; read their per-dimension rows,
//! not the headline, when comparing skills.

use std::io;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use do_harness_types::EvalDim;

use crate::eval_assert::AssertionGrade;
use crate::eval_walk::WalkRun;

use super::gate::{GateVerdict, run_structure_gate};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkillEvals {
    #[allow(dead_code)]
    pub(super) skill_name: String,
    pub(super) evals: Vec<EvalCase>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvalCase {
    #[allow(dead_code)]
    pub(super) id: i64,
    pub(super) prompt: String,
    pub(super) expected_output: String,
    #[allow(dead_code)]
    pub(super) files: Vec<String>,
    pub(super) assertions: Vec<String>,
    /// Scored dimension; untagged cases count as workflow effectiveness,
    /// preserving the pre-dimension meaning of the aggregate pass rate.
    #[serde(default)]
    pub(super) dim: EvalDim,
}

/// Per-dimension tally within one grading pass.
#[derive(Debug, Clone)]
pub(super) struct DimOutcome {
    pub(super) dim: EvalDim,
    pub(super) graded: u32,
    pub(super) passed: u32,
}

pub(super) struct SkillReport {
    pub(super) gate_failed: bool,
    pub(super) line: String,
    pub(super) pass_rate: Option<f64>,
    pub(super) graded: u32,
    pub(super) passed: u32,
    pub(super) prompt: Option<String>,
    pub(super) expected_outcome: Option<String>,
    /// With-skill minus without-skill pass rate in points; `None` when lift
    /// was not measured or either side graded nothing.
    pub(super) lift: Option<f64>,
    pub(super) without_pass_rate: Option<f64>,
    pub(super) without_graded: u32,
    pub(super) without_passed: u32,
    /// Per-dimension tallies with without-run passes merged in.
    pub(super) dims: Vec<DimReport>,
    /// Words in the installed `SKILL.md` plus `references/`.
    pub(super) skill_words: i64,
    /// Walkthrough wall time in seconds (with-skill run).
    pub(super) walk_secs: f64,
}

/// Per-dimension tally with the baseline merged in.
#[derive(Debug, Clone)]
pub(super) struct DimReport {
    pub(super) dim: EvalDim,
    pub(super) graded: u32,
    pub(super) passed: u32,
    pub(super) without_passed: Option<u32>,
}

pub(super) async fn check_skill(
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
        lift: None,
        without_pass_rate: None,
        without_graded: 0,
        without_passed: 0,
        dims: Vec::new(),
        skill_words: 0,
        walk_secs: 0.0,
    };

    let (verdict, gate_msg) = {
        let dir = dir.to_path_buf();
        let gate_script = gate_script.to_path_buf();
        tokio::task::spawn_blocking(move || run_structure_gate(&dir, &gate_script))
            .await
            .with_context(|| format!("structure gate task failed for '{name}'"))?
    };
    if verdict == GateVerdict::Fail {
        return Ok(SkillReport {
            gate_failed: true,
            line: format!("{name}: structure=invalid: {gate_msg} evals=skipped"),
            ..empty()
        });
    }
    let structure = match verdict {
        GateVerdict::Pass => "ok".to_owned(),
        GateVerdict::Unavailable => format!("unknown (gate unavailable: {gate_msg})"),
        GateVerdict::Fail => unreachable!("handled above"),
    };

    let outcome = grade_evals_in(dir, root, name).await?;
    let Some(outcome) = outcome else {
        let mut report = empty();
        report.line = format!("{name}: structure={structure} evals=none");
        return Ok(report);
    };
    let words = skill_words(dir);
    let mut report = report_from_outcome(name, &structure, outcome, words);
    report.lift = None;
    Ok(report)
}

/// Grades the same evals with the guidance payload stripped (no structure
/// gate: `SKILL.md` is absent by design). The caller pairs this baseline
/// with the with-skill report to compute Skill Lift.
pub(super) async fn check_skill_without(root: &Path, dir: &Path) -> Result<GradeOutcome> {
    let evals_path = dir.join("evals/evals.json");
    let content = tokio::fs::read_to_string(&evals_path)
        .await
        .with_context(|| format!("failed to read {}", evals_path.display()))?;
    let parsed: SkillEvals = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", evals_path.display()))?;
    let walk = {
        let dir = dir.to_path_buf();
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || crate::eval_walk::run_walkthrough(&dir, &root))
            .await
            .with_context(|| "without-skill walkthrough task failed")?
    };
    grade_skill(&parsed, root, &walk).await
}

/// Parses `evals.json`, runs the walkthrough timed, and grades. Returns
/// `None` when the skill ships no evals, or an `evals-invalid` outcome when
/// the fixture does not parse (unknown dims included).
async fn grade_evals_in(dir: &Path, root: &Path, name: &str) -> Result<Option<TimedOutcome>> {
    let evals_path = dir.join("evals/evals.json");
    let content = match tokio::fs::read_to_string(&evals_path).await {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", evals_path.display()));
        }
    };

    let parsed = match serde_json::from_str::<SkillEvals>(&content) {
        Ok(parsed) => parsed,
        Err(err) => {
            return Ok(Some(TimedOutcome {
                outcome: GradeOutcome {
                    passed: 0,
                    graded: 0,
                    pass_rate: None,
                    prompt: None,
                    expected_outcome: None,
                    dims: Vec::new(),
                    invalid: Some(format!("evals-invalid: {err}")),
                },
                walk_secs: 0.0,
            }));
        }
    };

    let started = Instant::now();
    let walk = {
        let dir = dir.to_path_buf();
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || crate::eval_walk::run_walkthrough(&dir, &root))
            .await
            .with_context(|| format!("walkthrough task failed for '{name}'"))?
    };
    let walk_secs = started.elapsed().as_secs_f64();
    let outcome = grade_skill(&parsed, root, &walk).await?;
    Ok(Some(TimedOutcome { outcome, walk_secs }))
}

struct TimedOutcome {
    outcome: GradeOutcome,
    walk_secs: f64,
}

fn report_from_outcome(
    name: &str,
    structure: &str,
    timed: TimedOutcome,
    words: i64,
) -> SkillReport {
    let outcome = timed.outcome;
    if let Some(invalid) = outcome.invalid.as_deref() {
        return SkillReport {
            gate_failed: false,
            line: format!("{name}: structure={structure} {invalid}"),
            pass_rate: None,
            graded: 0,
            passed: 0,
            prompt: None,
            expected_outcome: None,
            lift: None,
            without_pass_rate: None,
            without_graded: 0,
            without_passed: 0,
            dims: Vec::new(),
            skill_words: words,
            walk_secs: timed.walk_secs,
        };
    }
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
    SkillReport {
        gate_failed: false,
        line,
        pass_rate: outcome.pass_rate,
        graded: outcome.graded,
        passed: outcome.passed,
        prompt: outcome.prompt,
        expected_outcome: outcome.expected_outcome,
        lift: None,
        without_pass_rate: None,
        without_graded: 0,
        without_passed: 0,
        dims: outcome
            .dims
            .into_iter()
            .map(|d| DimReport {
                dim: d.dim,
                graded: d.graded,
                passed: d.passed,
                without_passed: None,
            })
            .collect(),
        skill_words: words,
        walk_secs: timed.walk_secs,
    }
}

/// Words in `SKILL.md` plus every `references/**/*.md`: the context the
/// skill costs whenever it loads.
fn skill_words(dir: &Path) -> i64 {
    let mut words = 0i64;
    let body = std::fs::read_to_string(dir.join("SKILL.md")).unwrap_or_default();
    words =
        words.saturating_add(i64::try_from(body.split_whitespace().count()).unwrap_or(i64::MAX));
    let refs = dir.join("references");
    let mut stack = vec![refs];
    while let Some(top) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&top) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                words = words.saturating_add(
                    i64::try_from(text.split_whitespace().count()).unwrap_or(i64::MAX),
                );
            }
        }
    }
    words
}

pub(super) struct GradeOutcome {
    pub(super) passed: u32,
    pub(super) graded: u32,
    pub(super) pass_rate: Option<f64>,
    pub(super) prompt: Option<String>,
    pub(super) expected_outcome: Option<String>,
    pub(super) dims: Vec<DimOutcome>,
    /// Set when the fixture itself is unparsable; grading is skipped.
    pub(super) invalid: Option<String>,
}

pub(super) async fn grade_skill(
    evals: &SkillEvals,
    root: &Path,
    walk: &WalkRun,
) -> Result<GradeOutcome> {
    let mut passed = 0u32;
    let mut graded = 0u32;
    let mut prompt = None;
    let mut expected_outcome = None;
    // Fixed-size tally indexed by canonical dimension order.
    let mut dim_graded = [0u32; 5];
    let mut dim_passed = [0u32; 5];

    for case in &evals.evals {
        let Some(slot) = EvalDim::all().iter().position(|d| *d == case.dim) else {
            continue;
        };
        for spec in &case.assertions {
            if !crate::eval_assert::is_graded(spec) {
                continue;
            }
            if prompt.is_none() {
                prompt = Some(case.prompt.clone());
                expected_outcome = Some(case.expected_output.clone());
            }
            graded += 1;
            dim_graded[slot] += 1;
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
                dim_passed[slot] += 1;
            }
        }
    }

    let pass_rate = (graded > 0).then(|| f64::from(passed) / f64::from(graded));
    let dims = EvalDim::all()
        .into_iter()
        .enumerate()
        .filter(|(i, _)| dim_graded[*i] > 0)
        .map(|(i, dim)| DimOutcome {
            dim,
            graded: dim_graded[i],
            passed: dim_passed[i],
        })
        .collect();
    Ok(GradeOutcome {
        passed,
        graded,
        pass_rate,
        prompt,
        expected_outcome,
        dims,
        invalid: None,
    })
}
