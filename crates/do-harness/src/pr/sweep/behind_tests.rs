//! Unit table for the `BEHIND` recovery chain (#518).
//!
//! The eligibility rule and the report tokens are pure, so they are pinned here
//! without a fake `gh`. The end-to-end chain — refusal, one comment, head
//! change, revalidation — lives in `tests/pr_sweep_behind.rs`, because it needs
//! the real binary.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::pr::readiness::AuthorizationReason;
use crate::pr::sweep::report::BranchUpdate;
use crate::pr::sweep::step::{self, BEHIND_UNUPDATABLE, Owner};

use super::{
    DEPENDENCY_BOT_REBASE_COMMAND, is_dependency_bot, manual_message, rebase_fallback_eligible,
};

/// Every eligible author form.
///
/// `gh pr list --json author` reports `app/dependabot`, `gh api` reports
/// `dependabot[bot]`, and a payload may omit both forms. All three are the same
/// bot, and an owned PR by any of them may be asked to rebase.
#[test]
fn every_dependabot_login_form_is_recognised() {
    for login in [
        "dependabot",
        "dependabot[bot]",
        "app/dependabot",
        "  app/dependabot  ",
        "Dependabot[bot]",
    ] {
        assert!(is_dependency_bot(login), "{login} must be Dependabot");
    }
}

/// Anything else is not: the command is Dependabot's, not a general bot verb.
#[test]
fn other_authors_are_not_dependency_bots() {
    for login in [
        "renovate",
        "renovate[bot]",
        "app/renovate",
        "dependabot-helper",
        "dependabot-helper[bot]",
        "not-dependabot[bot]",
        "octocat",
        "github-actions[bot]",
        "",
        "[bot]",
        "app/",
    ] {
        assert!(!is_dependency_bot(login), "{login} must not be Dependabot");
    }
}

/// The fallback needs *both* an owned PR and the bot login.
#[test]
fn eligibility_requires_ownership_and_the_bot_login() {
    assert!(rebase_fallback_eligible(Owner::Owned, "app/dependabot"));
    assert!(rebase_fallback_eligible(Owner::Owned, "dependabot[bot]"));
    // A third party is never touched, bot or not.
    assert!(!rebase_fallback_eligible(Owner::External, "app/dependabot"));
    // An undetermined identity is never touched either: "maybe ours" is not ours.
    assert!(!rebase_fallback_eligible(
        Owner::UnknownIdentity,
        "app/dependabot"
    ));
    // An owned PR by our own automation is not the dependency bot.
    assert!(!rebase_fallback_eligible(
        Owner::Owned,
        "github-actions[bot]"
    ));
}

/// The posted body is a fixed vendor command, never derived text.
#[test]
fn the_rebase_command_is_the_documented_literal() {
    assert_eq!(DEPENDENCY_BOT_REBASE_COMMAND, "@dependabot rebase");
}

/// Report tokens are the contract a reader and the tests key on.
#[test]
fn branch_update_tokens_are_stable() {
    assert_eq!(BranchUpdate::UpdateBranch.as_str(), "update-branch");
    assert_eq!(BranchUpdate::DependabotRebase.as_str(), "dependabot-rebase");
    assert_eq!(BranchUpdate::None.as_str(), "none");
}

/// The JSON spelling matches `as_str`, so the report and the text path agree.
#[test]
fn branch_update_serialises_to_the_same_tokens() {
    for (via, token) in [
        (BranchUpdate::UpdateBranch, "update-branch"),
        (BranchUpdate::DependabotRebase, "dependabot-rebase"),
        (BranchUpdate::None, "none"),
    ] {
        let json = serde_json::to_string(&via).unwrap();
        assert_eq!(json, format!("\"{token}\""));
    }
}

/// A refused update names the author and the required action, so the blocker is
/// the remediation rather than a restatement of the symptom.
#[test]
fn the_manual_message_names_the_author_and_the_fix() {
    let error = anyhow::anyhow!("HTTP 422: head is not up to date");
    let message = manual_message("octocat", false, &error);
    assert!(message.contains("octocat"), "{message}");
    assert!(message.contains("rebase or merge the base"), "{message}");
    assert!(message.contains("update-branch"), "{message}");
}

/// An empty login degrades to a phrase, never to "'' must rebase".
#[test]
fn an_unknown_author_falls_back_to_a_phrase() {
    let error = anyhow::anyhow!("HTTP 403: forbidden");
    let message = manual_message("   ", false, &error);
    assert!(message.contains("the author"), "{message}");
    assert!(!message.contains("''"), "{message}");
}

/// A second attempt at the same head says so, because the reason it cannot work
/// is different: the bot was already asked.
#[test]
fn a_repeat_attempt_at_the_same_head_says_it_was_already_asked() {
    let error = anyhow::anyhow!("HTTP 422: head is not up to date");
    let message = manual_message("app/dependabot", true, &error);
    assert!(message.contains("already asked to rebase"), "{message}");
}

/// `behind_unupdatable` is a blocked outcome with the message as its next
/// action — not `unknown`, and not a bare outcome code.
#[test]
fn the_behind_blocker_carries_its_message_as_the_next_action() {
    let reason = AuthorizationReason::new(BEHIND_UNUPDATABLE, "octocat must rebase");
    assert_eq!(
        step::next_action(7, std::slice::from_ref(&reason)).as_deref(),
        Some("octocat must rebase")
    );
}

/// The code itself is `behind_unupdatable`: the sweep continues to the next PR
/// instead of halting, so a reader must be able to tell it from a timeout.
#[test]
fn the_behind_code_is_distinct_from_the_update_codes() {
    assert_eq!(BEHIND_UNUPDATABLE, "behind_unupdatable");
    assert_ne!(BEHIND_UNUPDATABLE, "update_timeout");
    assert_ne!(BEHIND_UNUPDATABLE, "update_refused");
    assert_ne!(BEHIND_UNUPDATABLE, "update_required");
}
