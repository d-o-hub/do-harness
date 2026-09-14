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
/// cases mix it with other harness machinery, and negative cases are
/// out-of-scope requests where the skill must stay unloaded. The last one is
/// enforced: a skill with no negative case cannot measure Discoverability.
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
    }
    out
}
