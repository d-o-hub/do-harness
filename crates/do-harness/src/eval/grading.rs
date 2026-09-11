//! Evaluation JSON contracts and assertion grading.

use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};

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
}

pub(super) struct SkillReport {
    pub(super) gate_failed: bool,
    pub(super) line: String,
    pub(super) pass_rate: Option<f64>,
    pub(super) graded: u32,
    pub(super) passed: u32,
    pub(super) prompt: Option<String>,
    pub(super) expected_outcome: Option<String>,
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

pub(super) struct GradeOutcome {
    pub(super) passed: u32,
    pub(super) graded: u32,
    pub(super) pass_rate: Option<f64>,
    pub(super) prompt: Option<String>,
    pub(super) expected_outcome: Option<String>,
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
