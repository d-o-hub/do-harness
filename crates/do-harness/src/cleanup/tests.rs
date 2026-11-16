//! Pure parsing and classification checks for the cleanup verdict engine.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::discover::parse;
use super::{ALL_REASONS, Reason};

#[test]
fn porcelain_blocks_carry_branch_lock_and_prune_state() {
    let entries = parse(
        "worktree /repo\nHEAD aaaa\nbranch refs/heads/main\n\n\
         worktree /repo/wt\nHEAD bbbb\nbranch refs/heads/topic\nlocked network share\n\n\
         worktree /repo/gone\nHEAD cccc\ndetached\nprunable gitdir file points to non-existent location\n",
    )
    .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].branch.as_deref(), Some("main"));
    assert_eq!(entries[1].branch.as_deref(), Some("topic"));
    assert_eq!(entries[1].locked.as_deref(), Some("network share"));
    assert!(entries[2].detached);
    assert!(entries[2].prunable.is_some());
}

#[test]
fn a_block_without_a_head_is_rejected() {
    assert!(parse("worktree /repo\nbranch refs/heads/main\n").is_err());
}

#[test]
fn every_reason_round_trips_and_only_failures_are_errors() {
    for reason in ALL_REASONS {
        assert_eq!(Reason::from_code(reason.as_str()), Some(reason));
    }
    assert_eq!(Reason::from_code("not-a-reason"), None);
    for reason in [
        Reason::Dirty,
        Reason::Locked,
        Reason::Kept,
        Reason::Unmerged,
    ] {
        assert!(
            !reason.is_error_class(),
            "{} is a safety skip",
            reason.as_str()
        );
    }
    for reason in [Reason::Unproven, Reason::StaleBase, Reason::DeleteFailed] {
        assert!(reason.is_error_class(), "{} is an error", reason.as_str());
    }
}
