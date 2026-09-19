//! Outcome aggregation for graded eval passes.

use std::path::Path;

use anyhow::Result;
use do_harness_types::EvalDim;

use crate::eval_assert::AssertionGrade;
use crate::eval_walk::WalkRun;

use super::super::fixture::EvalKind;
use super::{EvalCase, SkillEvals};

/// Per-dimension tally within one grading pass.
#[derive(Debug, Clone)]
pub(in crate::eval) struct DimOutcome {
    pub(in crate::eval) dim: EvalDim,
    pub(in crate::eval) graded: u32,
    pub(in crate::eval) passed: u32,
}

/// Verdict and grader reason for one graded assertion.
#[derive(Debug, Clone, serde::Serialize)]
pub(in crate::eval) struct AssertionOutcome {
    /// The assertion spec as written in the fixture.
    pub(in crate::eval) spec: String,
    /// Whether the assertion passed.
    pub(in crate::eval) passed: bool,
    /// Grader explanation; names the missing needle or path on a miss.
    pub(in crate::eval) reason: String,
}

/// One graded case with its identity, dimension, and assertion detail.
#[derive(Debug, Clone, serde::Serialize)]
pub(in crate::eval) struct CaseOutcome {
    /// Fixture case id.
    pub(in crate::eval) id: i64,
    /// Dataset intent from the fixture.
    pub(in crate::eval) kind: EvalKind,
    /// Scored dimension.
    pub(in crate::eval) dim: EvalDim,
    /// Graded assertions in this case.
    pub(in crate::eval) graded: u32,
    /// Graded assertions that passed.
    pub(in crate::eval) passed: u32,
    /// Per-assertion verdicts in fixture order.
    pub(in crate::eval) assertions: Vec<AssertionOutcome>,
}

pub(in crate::eval) struct GradeOutcome {
    pub(in crate::eval) passed: u32,
    pub(in crate::eval) graded: u32,
    pub(in crate::eval) pass_rate: Option<f64>,
    pub(in crate::eval) prompt: Option<String>,
    pub(in crate::eval) expected_outcome: Option<String>,
    pub(in crate::eval) dims: Vec<DimOutcome>,
    /// Per-case detail backing the text and JSON report; cases without graded
    /// assertions are omitted.
    pub(in crate::eval) cases: Vec<CaseOutcome>,
    /// Executor wall time for this pass, in seconds.
    pub(in crate::eval) walk_secs: f64,
}

impl GradeOutcome {
    /// Empty accumulator for per-case agent grading.
    pub(in crate::eval) fn empty() -> Self {
        GradeOutcome {
            passed: 0,
            graded: 0,
            pass_rate: None,
            prompt: None,
            expected_outcome: None,
            dims: Vec::new(),
            cases: Vec::new(),
            walk_secs: 0.0,
        }
    }

    /// Folds one graded case into the accumulator and recomputes the rate.
    pub(in crate::eval) fn merge_case(&mut self, case: GradeOutcome) {
        self.passed += case.passed;
        self.graded += case.graded;
        if self.prompt.is_none() {
            self.prompt = case.prompt;
            self.expected_outcome = case.expected_outcome;
        }
        for add in case.dims {
            match self.dims.iter_mut().find(|d| d.dim == add.dim) {
                Some(existing) => {
                    existing.graded += add.graded;
                    existing.passed += add.passed;
                }
                None => self.dims.push(add),
            }
        }
        self.cases.extend(case.cases);
        self.dims.sort_by_key(|d| {
            EvalDim::all()
                .iter()
                .position(|candidate| *candidate == d.dim)
                .unwrap_or(usize::MAX)
        });
        self.pass_rate = (self.graded > 0).then(|| f64::from(self.passed) / f64::from(self.graded));
    }
}

/// Wraps `evals` into a single-case suite for per-case agent grading.
pub(in crate::eval) fn single_case(evals: &SkillEvals, case: &EvalCase) -> SkillEvals {
    SkillEvals {
        skill_name: evals.skill_name.clone(),
        evals: vec![case.clone()],
    }
}

/// Grades every graded assertion in `evals` against `root`'s residue.
///
/// A failed executor ([`WalkRun::success`] false) fails every graded
/// assertion with the executor's detail as the reason.
pub(in crate::eval) async fn grade_skill(
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
    let mut cases = Vec::new();

    for case in &evals.evals {
        let Some(slot) = EvalDim::all().iter().position(|d| *d == case.dim) else {
            continue;
        };
        let mut case_graded = 0u32;
        let mut case_passed = 0u32;
        let mut case_assertions = Vec::new();
        for spec in &case.assertions {
            if !crate::eval_assert::is_graded(spec) {
                continue;
            }
            if prompt.is_none() {
                prompt = Some(case.prompt.clone());
                expected_outcome = Some(case.expected_output.clone());
            }
            graded += 1;
            case_graded += 1;
            dim_graded[slot] += 1;
            let grade: AssertionGrade = if walk.present && !walk.success {
                let reason = walk
                    .detail
                    .clone()
                    .unwrap_or_else(|| "executor exited non-zero".to_owned());
                AssertionGrade {
                    passed: false,
                    reason,
                }
            } else {
                crate::eval_assert::grade(root, spec, walk).await?
            };
            if grade.passed {
                passed += 1;
                case_passed += 1;
                dim_passed[slot] += 1;
            }
            case_assertions.push(AssertionOutcome {
                spec: spec.clone(),
                passed: grade.passed,
                reason: grade.reason,
            });
        }
        if case_graded > 0 {
            cases.push(CaseOutcome {
                id: case.id,
                kind: case.kind,
                dim: case.dim,
                graded: case_graded,
                passed: case_passed,
                assertions: case_assertions,
            });
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
        cases,
        walk_secs: 0.0,
    })
}
