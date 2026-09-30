//! Classification of PR comments for the loop measures.

use crate::pr::gh::Comment;

/// Words that name a patch-coverage waiver class in a review comment.
///
/// The labels match [`crate::pr::waivers::WaiverClass`] so a waiver count here
/// and a `pr waivers` verdict describe the same class.
const WAIVER_KEYWORDS: [(&str, &[&str]); 3] = [
    (
        "macro-field",
        &[
            "macro-field",
            "field expression",
            "field span",
            "tracing::",
            "log::",
            "subscriber",
        ],
    ),
    (
        "guarded-arm",
        &[
            "guarded-arm",
            "unreachable",
            "defensive arm",
            "cannot execute",
            "cannot happen",
            "let-else",
        ],
    ),
    (
        "feature-gated",
        &["feature-gated", "cfg(feature", "no ci job enables"],
    ),
];

/// Markers that make a human comment actionable rather than informational.
const ACTION_MARKERS: [&str; 12] = [
    "must ", "should ", "please ", "needs ", "needed", "fix ", "add ", "remove ", "rename ",
    "rerun ", "revert ", "waiv",
];

/// Waiver classes named by one comment body.
pub(super) fn waiver_classes(body: &str) -> Vec<(&'static str, usize)> {
    let lowered = body.to_lowercase();
    WAIVER_KEYWORDS
        .iter()
        .filter(|(_, keywords)| keywords.iter().any(|keyword| lowered.contains(keyword)))
        .map(|(class, _)| (*class, 1))
        .collect()
}

/// Whether a comment asks for work rather than reporting state.
///
/// A bot login is informational; a human comment is actionable when its body
/// carries an action marker, informational otherwise. The rule is deliberately
/// lexical (and documented in `docs/cli.md`) so the measure is reproducible.
pub(super) fn is_actionable(comment: &Comment) -> bool {
    let Some(login) = comment
        .user
        .as_ref()
        .map(|author| author.login.as_str())
        .filter(|login| !login.is_empty())
    else {
        return false;
    };
    if crate::pr::readiness::is_bot_login(login) {
        return false;
    }
    let lowered = comment.body.to_lowercase();
    ACTION_MARKERS.iter().any(|marker| lowered.contains(marker))
}
