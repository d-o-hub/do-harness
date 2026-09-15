//! Dataset taxonomy and quality diagnostics for skill eval fixtures.
//!
//! Thin datasets made the eval gate vacuous once: a skill with one assertion
//! and no out-of-scope case can score 1.00 while measuring nothing. The case
//! `kind` makes dataset intent explicit, and [`fixture_diagnostics`] names
//! the gaps so `eval --strict-fixtures` can refuse them.

use super::grading::SkillEvals;

/// Dataset intent of one eval case.
///
/// The taxonomy follows the evaluation-model split: explicit cases name the
/// skill or its task, implicit cases need it without naming it, contextual
/// cases mix it with other harness machinery, negative cases are out-of-scope
/// requests where the skill must stay unloaded, and gotchas cases carry
/// *negative knowledge* — a known trap where the run must observably avoid the
/// wrong action. The last one is where unsolved, non-green lessons live: a
/// gotchas case is provable even though no positive fix ever passed, because
/// its assertions are negative (`absent:` / `not-contains:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum EvalKind {
    /// Direct request naming the skill or its task.
    #[default]
    Explicit,
    /// Task needs the skill but does not name it.
    Implicit,
    /// Multi-step task mixing the skill with other harness machinery.
    Contextual,
    /// Out-of-scope request: the skill must not load or act.
    Negative,
    /// Known trap: the run must observably avoid a recorded wrong action.
    Gotchas,
}

/// Dataset-quality findings for a parsed fixture.
///
/// Thin datasets made the eval gate vacuous once: a skill with one assertion
/// and no out-of-scope case can score 1.00 while measuring nothing. These
/// diagnostics name the gaps; `eval --strict-fixtures` turns them into gate
/// failures so the corpus cannot silently regress.
pub(super) fn fixture_diagnostics(evals: &SkillEvals) -> Vec<String> {
    let mut out = Vec::new();
    if evals.evals.is_empty() {
        out.push("no eval cases".to_owned());
        return out;
    }
    if !evals
        .evals
        .iter()
        .any(|case| case.kind == EvalKind::Negative)
    {
        out.push("no negative (out-of-scope) case".to_owned());
    }
    for case in &evals.evals {
        if !case
            .assertions
            .iter()
            .any(|spec| crate::eval_assert::is_graded(spec))
        {
            out.push(format!("case {} has no graded assertions", case.id));
        }
        // A gotchas case exists to prove the wrong action was NOT taken, so it
        // must carry at least one negative assertion; a positive-only "gotchas"
        // case measures nothing the other kinds do not.
        if case.kind == EvalKind::Gotchas && !case.assertions.iter().any(|spec| is_negative(spec)) {
            out.push(format!(
                "case {} is kind=gotchas but has no negative (absent:/not-contains:) assertion",
                case.id
            ));
        }
    }
    // Self-answering fixtures: when no assertion reads the skill's own guidance
    // (SKILL.md or references/), the deterministic walkthrough scores the same
    // with and without the skill, so Skill Lift is structurally zero and the
    // fixture cannot measure whether the guidance is what produced the result.
    // Typical cause: every assertion targets residue the walkthrough itself
    // wrote.
    let guidance_root = format!(".agents/skills/{}/", evals.skill_name);
    let reads_guidance = evals.evals.iter().any(|case| {
        case.assertions.iter().any(|spec| {
            let rest = spec
                .strip_prefix("contains:")
                .or_else(|| spec.strip_prefix("not-contains:"));
            let target = match rest {
                Some(rest) => rest.split_once('|').map_or(rest, |(path, _)| path),
                None => spec.as_str(),
            };
            target.starts_with(&guidance_root)
        })
    });
    if !reads_guidance {
        out.push(
            "no assertion reads the skill's own guidance (SKILL.md or references/): \
             the fixture is self-answering and cannot measure Skill Lift"
                .to_owned(),
        );
    }
    out
}

/// Whether a graded assertion expresses a negative expectation.
///
/// `absent:` proves an artifact was not created; `not-contains:` proves an
/// artifact that must exist omits a forbidden action or value.
fn is_negative(spec: &str) -> bool {
    spec.starts_with("absent:") || spec.starts_with("not-contains:")
}
