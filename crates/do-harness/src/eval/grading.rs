//! Evaluation JSON contracts, structure gating, and assertion grading.
//!
//! Two executors produce residue for the same fixture: the deterministic
//! `evals/walkthrough.sh` (one sandbox per skill) and, with
//! `eval --agent-cmd`, an external agent command (one sandbox per case; see
//! [`super::agent`]). Both grade through [`grade_skill`], so the assertion
//! DSL and dimension tallies stay identical across modes.
//!
//! Cases carry an optional `dim` (one of the five [`do_harness_types::EvalDim`]
//! wire names, defaulting to `effectiveness`) so results break down per
//! dimension instead of hiding behind the aggregate pass rate.

use std::io;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use do_harness_types::{EvalDim, EvalMode};

use crate::eval_assert::AssertionGrade;
use crate::eval_walk::WalkRun;

use super::fixture::{EvalKind, fixture_diagnostics};
use super::gate::{GateVerdict, run_structure_gate};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkillEvals {
    #[allow(dead_code)]
    pub(super) skill_name: String,
    pub(super) evals: Vec<EvalCase>,
}

#[derive(Debug, Clone, serde::Deserialize)]
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
    /// Dataset intent of this case; see [`EvalKind`].
    #[serde(default)]
    pub(super) kind: EvalKind,
}

pub(super) struct SkillReport {
    /// Execution mode this report was produced in.
    pub(super) mode: EvalMode,
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
    /// Executor wall time in seconds (walkthrough or agent runs).
    pub(super) walk_secs: f64,
    /// Dataset-quality findings; empty when the fixture is well-formed.
    pub(super) fixture_warnings: Vec<String>,
}

/// Per-dimension tally with the baseline merged in.
#[derive(Debug, Clone)]
pub(super) struct DimReport {
    pub(super) dim: EvalDim,
    pub(super) graded: u32,
    pub(super) passed: u32,
    pub(super) without_passed: Option<u32>,
}

/// Result of the Tier-1 structure gate plus fixture parsing.
pub(super) enum GateOutcome {
    /// Structure gate failed; eval is skipped (fail-closed).
    Failed(SkillReport),
    /// The skill ships no evals: nothing to grade.
    NoEvals(SkillReport),
    /// The fixture does not parse: nothing to grade.
    InvalidEvals(SkillReport),
    /// Ready to execute with the parsed fixture.
    Ready {
        structure: String,
        evals: SkillEvals,
    },
}

/// Runs `quick_validate.py` against `dir`, then parses `evals/evals.json`.
///
/// Both executors share this so gate failures, missing fixtures, and invalid
/// fixtures report identically.
pub(super) async fn gate_and_parse(
    dir: &Path,
    name: &str,
    gate_script: &Path,
) -> Result<GateOutcome> {
    let (verdict, gate_msg) = {
        let dir = dir.to_path_buf();
        let gate_script = gate_script.to_path_buf();
        tokio::task::spawn_blocking(move || run_structure_gate(&dir, &gate_script))
            .await
            .with_context(|| format!("structure gate task failed for '{name}'"))?
    };
    if verdict == GateVerdict::Fail {
        return Ok(GateOutcome::Failed(SkillReport {
            gate_failed: true,
            line: format!("{name}: structure=invalid: {gate_msg} evals=skipped"),
            ..empty_report()
        }));
    }
    let structure = match verdict {
        GateVerdict::Pass => "ok".to_owned(),
        GateVerdict::Unavailable => format!("unknown (gate unavailable: {gate_msg})"),
        GateVerdict::Fail => unreachable!("handled above"),
    };

    let evals_path = dir.join("evals/evals.json");
    let content = match tokio::fs::read_to_string(&evals_path).await {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let mut report = empty_report();
            report.line = format!("{name}: structure={structure} evals=none");
            return Ok(GateOutcome::NoEvals(report));
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", evals_path.display()));
        }
    };
    match serde_json::from_str::<SkillEvals>(&content) {
        Ok(evals) => Ok(GateOutcome::Ready { structure, evals }),
        Err(err) => {
            let mut report = empty_report();
            report.line = format!("{name}: structure={structure} evals-invalid: {err}");
            Ok(GateOutcome::InvalidEvals(report))
        }
    }
}

/// Reads and parses a skill's `evals/evals.json`; `None` when absent.
pub(super) async fn load_evals(dir: &Path) -> Result<Option<SkillEvals>> {
    let evals_path = dir.join("evals/evals.json");
    let content = match tokio::fs::read_to_string(&evals_path).await {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", evals_path.display()));
        }
    };
    serde_json::from_str::<SkillEvals>(&content)
        .map(Some)
        .with_context(|| format!("failed to parse {}", evals_path.display()))
}

/// Empty report used as the base for early exits.
fn empty_report() -> SkillReport {
    SkillReport {
        mode: EvalMode::Deterministic,
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
        fixture_warnings: Vec::new(),
    }
}

/// Deterministic path: gate, parse, run the walkthrough once, grade every
/// case against its residue.
pub(super) async fn check_skill(
    root: &Path,
    dir: &Path,
    name: &str,
    gate_script: &Path,
) -> Result<SkillReport> {
    let (structure, evals) = match gate_and_parse(dir, name, gate_script).await? {
        GateOutcome::Failed(report)
        | GateOutcome::NoEvals(report)
        | GateOutcome::InvalidEvals(report) => return Ok(report),
        GateOutcome::Ready { structure, evals } => (structure, evals),
    };

    let started = Instant::now();
    let walk = {
        let dir = dir.to_path_buf();
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || crate::eval_walk::run_walkthrough(&dir, &root))
            .await
            .with_context(|| format!("walkthrough task failed for '{name}'"))?
    };
    let mut outcome = grade_skill(&evals, root, &walk).await?;
    outcome.walk_secs = started.elapsed().as_secs_f64();
    let warnings = fixture_diagnostics(&evals);
    let mut report = report_from_outcome(name, &structure, outcome, skill_words(dir), warnings);
    report.lift = None;
    Ok(report)
}

/// Grades the same evals with the guidance payload stripped (no structure
/// gate: `SKILL.md` is absent by design). The caller pairs this baseline
/// with the with-skill report to compute Skill Lift.
pub(super) async fn check_skill_without(root: &Path, dir: &Path) -> Result<GradeOutcome> {
    let evals = load_evals(dir)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no evals at {}", dir.display()))?;
    let started = Instant::now();
    let walk = {
        let dir = dir.to_path_buf();
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || crate::eval_walk::run_walkthrough(&dir, &root))
            .await
            .with_context(|| "without-skill walkthrough task failed")?
    };
    let mut outcome = grade_skill(&evals, root, &walk).await?;
    outcome.walk_secs = started.elapsed().as_secs_f64();
    Ok(outcome)
}

/// Builds the report line and `SkillReport` from a graded outcome.
pub(super) fn report_from_outcome(
    name: &str,
    structure: &str,
    outcome: GradeOutcome,
    words: i64,
    fixture_warnings: Vec<String>,
) -> SkillReport {
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
        mode: EvalMode::Deterministic,
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
        walk_secs: outcome.walk_secs,
        fixture_warnings,
    }
}

/// Words in `SKILL.md` plus every `references/**/*.md`: the context the
/// skill costs whenever it loads.
pub(super) fn skill_words(dir: &Path) -> i64 {
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

/// Per-dimension tally within one grading pass.
#[derive(Debug, Clone)]
pub(super) struct DimOutcome {
    pub(super) dim: EvalDim,
    pub(super) graded: u32,
    pub(super) passed: u32,
}

pub(super) struct GradeOutcome {
    pub(super) passed: u32,
    pub(super) graded: u32,
    pub(super) pass_rate: Option<f64>,
    pub(super) prompt: Option<String>,
    pub(super) expected_outcome: Option<String>,
    pub(super) dims: Vec<DimOutcome>,
    /// Executor wall time for this pass, in seconds.
    pub(super) walk_secs: f64,
}

impl GradeOutcome {
    /// Empty accumulator for per-case agent grading.
    pub(super) fn empty() -> Self {
        GradeOutcome {
            passed: 0,
            graded: 0,
            pass_rate: None,
            prompt: None,
            expected_outcome: None,
            dims: Vec::new(),
            walk_secs: 0.0,
        }
    }

    /// Folds one graded case into the accumulator and recomputes the rate.
    pub(super) fn merge_case(&mut self, case: GradeOutcome) {
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
pub(super) fn single_case(evals: &SkillEvals, case: &EvalCase) -> SkillEvals {
    SkillEvals {
        skill_name: evals.skill_name.clone(),
        evals: vec![case.clone()],
    }
}

/// Grades every graded assertion in `evals` against `root`'s residue.
///
/// A failed executor ([`WalkRun::success`] false) fails every graded
/// assertion with the executor's detail as the reason.
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
        walk_secs: 0.0,
    })
}
