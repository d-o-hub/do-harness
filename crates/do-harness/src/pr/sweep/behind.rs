//! Recovering a `BEHIND` branch when `update-branch` is refused (#518).
//!
//! With `strict_required_status_checks_policy: true` every merge makes the next
//! pull request `BEHIND`, and `PUT /pulls/{n}/update-branch` needs write access
//! to the head branch. A Dependabot branch is owned by `dependabot[bot]`, so that
//! request is refused and a sweep could merge exactly one dependency pull
//! request before stopping on the second.
//!
//! `GitHub` documents a vendor command for exactly this case
//! ([Dependabot pull request comment commands](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-pull-request-comment-commands)):
//!
//! > `@dependabot rebase` — Rebases the pull request.
//!
//! It is asynchronous and it force-pushes, so the head SHA changes and every
//! prior receipt is invalid. [`update`] owns the ordered chain: the API update
//! first, the vendor command only when the API refuses, and only for a
//! dependency bot. The caller revalidates from scratch afterwards, which is what
//! makes the changed head safe rather than merely new.
//!
//! # What this deliberately never does
//!
//! A human or external author is never commented at and never pushed. The
//! comment body is a fixed vendor command this module owns — never text derived
//! from the pull request — posted at most once per head, only under `--execute`,
//! and only for an author the bot set recognises. There is no `--admin`, no
//! `--auto`, and no push: when the fallback cannot help, the outcome is
//! [`BEHIND_UNUPDATABLE`].

use std::path::Path;

use crate::pr::gh::{self, PrReadinessView};
use crate::pr::readiness::AuthorizationReason;
use crate::pr::sweep::report::BranchUpdate;
use crate::pr::sweep::state::PrAudit;
use crate::pr::sweep::step::{BEHIND_UNUPDATABLE, Owner};

/// Result of waiting for a head change after a branch update.
enum HeadPoll {
    /// The head moved; carries the refreshed view.
    Changed(Box<PrReadinessView>),
    /// The head did not move within the bound.
    Unchanged,
    /// The view could not be re-read.
    ReadFailed,
}

/// Waits for the head to change, observing immediately and never before the
/// first read.
fn wait_for_head_change(root: &Path, number: u64, old_head: &str, wait_seconds: u64) -> HeadPoll {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait_seconds);
    loop {
        match gh::readiness_view(root, number) {
            Ok(view) => {
                if view.head_ref_oid != old_head {
                    return HeadPoll::Changed(Box::new(view));
                }
            }
            Err(_) => return HeadPoll::ReadFailed,
        }
        if std::time::Instant::now() >= deadline {
            return HeadPoll::Unchanged;
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

/// The documented rebase command, and the only comment body this module posts.
pub(super) const DEPENDENCY_BOT_REBASE_COMMAND: &str = "@dependabot rebase";

/// Upper bound on waiting for a head change after a branch update.
pub(super) const MAX_UPDATE_WAIT_SECONDS: u64 = 1200;

/// What the branch-update step decided for one `BEHIND` pull request.
pub(super) enum Outcome {
    /// The head moved; the caller revalidates from scratch.
    Updated {
        /// The refreshed view, already read.
        view: Box<PrReadinessView>,
        /// Which mechanism moved the head.
        via: BranchUpdate,
    },
    /// `update-branch` was accepted but the head never moved.
    ///
    /// The API agreed and nothing happened, so what is wrong is unknowable from
    /// here — that is `unknown`, not a branch state this tool can name.
    Stalled {
        /// The mechanism that stalled.
        via: BranchUpdate,
    },
    /// The branch is still behind and this tool cannot move it.
    Unupdatable {
        /// The mechanism that was used before giving up.
        via: BranchUpdate,
        /// The blocker to report.
        reason: AuthorizationReason,
    },
    /// The PR could not be re-read after the request.
    ReadFailed,
}

/// One `BEHIND` pull request, as the recovery chain needs it.
///
/// Grouped rather than passed as loose arguments so the chain's steps take the
/// same four or five parameters and a call cannot transpose two of them.
struct Target<'a> {
    root: &'a Path,
    number: u64,
    old_head: &'a str,
    author: &'a str,
    owner: Owner,
}

/// Runs the ordered recovery chain for one `BEHIND` pull request.
///
/// 1. `PUT /pulls/{n}/update-branch`, pinned to the captured head;
/// 2. on refusal, and only for a dependency bot, post
///    [`DEPENDENCY_BOT_REBASE_COMMAND`];
/// 3. wait for the head to move, bounded by `wait_seconds`;
/// 4. return so the caller revalidates from scratch.
///
/// `owner` is passed rather than assumed so the eligibility rule stays honest at
/// a call site that only ever reaches here for an owned PR.
pub(super) fn update(
    root: &Path,
    number: u64,
    old_head: &str,
    author: &str,
    owner: Owner,
    audit: &mut PrAudit,
    wait_seconds: u64,
) -> Outcome {
    let target = Target {
        root,
        number,
        old_head,
        author,
        owner,
    };
    audit.update_requested = true;
    audit.update_expected_head = Some(old_head.to_owned());
    match gh::update_branch(root, number, old_head) {
        Ok(()) => await_head(&target, wait_seconds, BranchUpdate::UpdateBranch, None),
        Err(error) => refuse(&target, audit, &error, wait_seconds),
    }
}

/// The second link: ask the dependency bot to rebase, then wait.
fn refuse(
    target: &Target<'_>,
    audit: &mut PrAudit,
    error: &anyhow::Error,
    wait_seconds: u64,
) -> Outcome {
    // Once per head: a repeat request for the same head cannot help — the bot
    // either already rebased (the head would have moved) or will not (the 30-day
    // rule, or extra commits on the branch). A *new* head earns a new request.
    let already_asked = audit.rebase_comment_head.as_deref() == Some(target.old_head);
    if !rebase_fallback_eligible(target.owner, target.author) || already_asked {
        let message = manual_message(target.author, already_asked, error);
        return Outcome::Unupdatable {
            via: BranchUpdate::None,
            reason: AuthorizationReason::new(BEHIND_UNUPDATABLE, message),
        };
    }
    if let Err(post_error) =
        gh::post_issue_comment(target.root, target.number, DEPENDENCY_BOT_REBASE_COMMAND)
    {
        return Outcome::Unupdatable {
            via: BranchUpdate::None,
            reason: AuthorizationReason::new(
                BEHIND_UNUPDATABLE,
                format!(
                    "the branch is behind its base and the rebase request could not be posted \
                     ({post_error:#}); {} must update the branch",
                    target.author
                ),
            ),
        };
    }
    audit.rebase_comment_posted = true;
    audit.rebase_comment_head = Some(target.old_head.to_owned());
    await_head(
        target,
        wait_seconds,
        BranchUpdate::DependabotRebase,
        Some(wait_seconds),
    )
}

/// Waits for the head to move.
///
/// `rebase_wait` carries the bound only on the vendor-command path, because a
/// rebase that never lands is a branch this tool cannot move (`blocked`), while
/// an API update that was accepted and never landed is an unexplained stall
/// (`unknown`). Collapsing the two would either overstate certainty about the
/// rebase or understate it about the API.
fn await_head(
    target: &Target<'_>,
    wait_seconds: u64,
    via: BranchUpdate,
    rebase_wait: Option<u64>,
) -> Outcome {
    match wait_for_head_change(target.root, target.number, target.old_head, wait_seconds) {
        HeadPoll::Changed(view) => Outcome::Updated { view, via },
        HeadPoll::ReadFailed => Outcome::ReadFailed,
        HeadPoll::Unchanged => match rebase_wait {
            Some(bound) => Outcome::Unupdatable {
                via,
                reason: AuthorizationReason::new(
                    BEHIND_UNUPDATABLE,
                    format!(
                        "the branch is behind its base and the dependency bot did not rebase it \
                         within {bound}s; re-run after the bot rebases, or update the branch \
                         yourself in an authorized work session"
                    ),
                ),
            },
            None => Outcome::Stalled { via },
        },
    }
}

/// The blocker for a branch only its author can move.
fn manual_message(author: &str, already_asked: bool, error: &anyhow::Error) -> String {
    let who = if author.trim().is_empty() {
        "the author"
    } else {
        author
    };
    if already_asked {
        format!(
            "the branch is behind its base and was already asked to rebase at this head; \
             {who} must rebase or merge the base"
        )
    } else {
        format!(
            "the branch is behind its base and `update-branch` was refused ({error:#}); \
             {who} must rebase or merge the base"
        )
    }
}

/// Whether the dependency-bot rebase fallback may be used for this author.
///
/// Both conditions are required, and neither is a heuristic about intent:
///
/// * the author must be *owned* — an external or unknown identity is never
///   touched, because the comment would ask a stranger's bot to rewrite a
///   branch on our behalf;
/// * the login must be in the dependency-bot set, because `@dependabot rebase`
///   is a command only that service understands. Posting it at an arbitrary
///   `[bot]` would be noise at best.
#[must_use]
pub(super) fn rebase_fallback_eligible(owner: Owner, login: &str) -> bool {
    owner == Owner::Owned && is_dependency_bot(login)
}

/// Whether `login` is Dependabot.
///
/// Narrower than the general bot predicate on purpose: our own automation bots
/// do not answer this command, so only Dependabot logins qualify. The `app/`
/// prefix is stripped first, because `gh pr list --json author` reports
/// `app/dependabot` for an App-authored pull request.
#[must_use]
pub(super) fn is_dependency_bot(login: &str) -> bool {
    let login = login.trim();
    let name = login.strip_prefix("app/").unwrap_or(login);
    let name = name.strip_suffix("[bot]").unwrap_or(name);
    name.eq_ignore_ascii_case("dependabot")
}

#[cfg(test)]
#[path = "behind_tests.rs"]
mod tests;
