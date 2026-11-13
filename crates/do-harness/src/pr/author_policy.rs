//! Who a sweep is allowed to merge (#519).
//!
//! Merging was restricted to own/bot/admin-authored pull requests with no way to
//! relax that. It is now a declared policy with the safe value as the compiled-in
//! default:
//!
//! * [`AuthorPolicy::Owned`] — only an author `classify_author` calls
//!   [`Owner::Owned`]. This is today's behaviour and the default.
//! * [`AuthorPolicy::OwnedOrExternalGreen`] — an *external* author's pull
//!   request may merge, but only when the authorization snapshot carries zero
//!   blockers. "Green" is not a separate judgement here: it is exactly the
//!   readiness verdict, so whatever gates an owned merge gates an external one,
//!   including the required-check rules from #515/#516 and the conversation
//!   dispositions from #517.
//!
//! # What the relaxed policy does not relax
//!
//! The extra permission is one-way and minimal: an external branch is read and
//! merged, never written. No `update-branch`, no comment, no push, no thread
//! resolution, no closure — under either policy. A blocker on an external
//! pull request is still `skipped: external` with the findings reported, because
//! the tool cannot fix a branch it does not own.
//!
//! Draft and `hold`/`wip`/`do-not-merge` remain hard stops for every author: they
//! are the author's own decisions, and relaxing who may merge is not a licence to
//! override them.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::pr::sweep::step::Owner;

/// The `[pr]` table of `do-harness.toml`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrConfig {
    /// Who a `pr sweep` may merge. Absent means [`AuthorPolicy::Owned`].
    ///
    /// Typed rather than a string so a misspelling is a configuration error at
    /// load time instead of a silent fallback to the default: a policy that
    /// widens what automation may merge must never be *guessed*.
    #[serde(default)]
    pub author_policy: AuthorPolicy,
}

/// Who a sweep may merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum AuthorPolicy {
    /// Only own/bot/admin-authored pull requests (the default).
    #[default]
    #[value(name = "owned")]
    Owned,
    /// Also an external author's pull request, but only when it is fully green.
    #[value(name = "owned-or-external-green")]
    OwnedOrExternalGreen,
}

impl AuthorPolicy {
    /// The stable token recorded in the report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::OwnedOrExternalGreen => "owned-or-external-green",
        }
    }

    /// Whether a pull request by this author may be merged at all.
    ///
    /// [`Owner::UnknownIdentity`] is never mergeable: "we could not tell whose
    /// branch this is" is not evidence that it is safe to merge. The relaxed
    /// policy widens `External` only.
    #[must_use]
    pub fn may_merge(self, owner: Owner) -> bool {
        match (self, owner) {
            (_, Owner::Owned) | (Self::OwnedOrExternalGreen, Owner::External) => true,
            (Self::Owned, Owner::External) | (_, Owner::UnknownIdentity) => false,
        }
    }
}

/// The policy the requested [`AuthorPolicy`] resolves to.
///
/// Precedence is flag, then repository configuration, then the compiled-in
/// default, so an invocation can narrow or widen what the repository declared
/// without editing a file — and a repository can set the default once.
///
/// # Errors
///
/// Returns an error when `do-harness.toml` exists but cannot be read or parsed;
/// a malformed value is never silently replaced by the default.
pub fn resolve(root: &Path, flag: Option<AuthorPolicy>) -> Result<AuthorPolicy> {
    if let Some(policy) = flag {
        return Ok(policy);
    }
    Ok(crate::config::load_sync(root)?.pr.author_policy)
}

#[cfg(test)]
#[path = "author_policy_tests.rs"]
mod tests;
