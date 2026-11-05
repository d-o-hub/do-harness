//! Eligibility for executing `pr update-base` (#468).
//!
//! An update is executed only for an authorized own/known-bot head that lives
//! in this repository. A fork head, a contributor's branch we are not allowed
//! to rewrite, or an identity we cannot prove is not updated.

use std::path::Path;

use crate::pr::gh::{self, PrReadinessView};

/// Result of the eligibility check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibilityOutcome {
    /// The head may be updated under `--execute`.
    Eligible,
    /// A confirmed condition forbids the update; `--execute` blocks.
    Blocked(&'static str),
    /// The identity or permission could not be proven; the state is `UNKNOWN`.
    Unknown(&'static str),
}

/// Whether `view`'s head may be updated.
///
/// Rules:
/// - the head must be a branch of this repository (not a fork), and
/// - the PR author must be the authenticated user or a known bot, and
/// - the authenticated user must hold a write/admin/maintain/owner role.
///
/// A missing head repository, identity, or permission is a fail-closed
/// `Unknown`; it never authorizes.
pub fn evaluate(
    root: &Path,
    view: &PrReadinessView,
    repo: &str,
    current_user: &str,
    author: &str,
) -> EligibilityOutcome {
    let Some(head_owner) = view
        .head_repository
        .as_ref()
        .map(|repository| repository.name_with_owner.clone())
    else {
        return EligibilityOutcome::Blocked("fork_head");
    };
    if head_owner != repo {
        return EligibilityOutcome::Blocked("fork_head");
    }
    if current_user.is_empty() || author.is_empty() {
        return EligibilityOutcome::Unknown("identity_unknown");
    }
    let Ok(permission) = gh::user_permission(root, current_user) else {
        return EligibilityOutcome::Unknown("permission_unknown");
    };
    if !crate::pr::waiver_comment::is_authorized_role(&permission) {
        return EligibilityOutcome::Unknown("permission_unknown");
    }
    if author == current_user || crate::pr::readiness::is_bot_login(author) {
        EligibilityOutcome::Eligible
    } else {
        EligibilityOutcome::Blocked("unauthorized")
    }
}
