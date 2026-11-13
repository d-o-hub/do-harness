//! Unit table for the post-fetch snapshot check (#520).
//!
//! The end-to-end behaviour — a shallow clone becoming provable, a moved base
//! refusing the proof — is in `tests/pr_fetch.rs`, because it needs a real
//! remote. What lives here is the decision itself: given the view the proof
//! planned against and the view read back after the fetch, is the pair still the
//! one we set out to compare?

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::pr::gh::PrView;

use super::{Snapshot, compare_snapshots};

/// A view with recognisable object ids.
fn view(number: u64, base_oid: &str, head_oid: &str) -> PrView {
    PrView {
        number,
        base_ref_name: "main".to_owned(),
        base_ref_oid: base_oid.to_owned(),
        head_ref_oid: head_oid.to_owned(),
    }
}

const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE_2: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HEAD: &str = "cccccccccccccccccccccccccccccccccccccccc";
const HEAD_2: &str = "dddddddddddddddddddddddddddddddddddddddd";

/// The ordinary case: a fetch changed nothing about the pair.
#[test]
fn an_identical_view_is_unchanged() {
    assert_eq!(
        compare_snapshots(&view(1, BASE, HEAD), &view(1, BASE, HEAD)),
        Snapshot::Unchanged
    );
}

/// A base that moved during the fetch must be named, not silently accepted.
#[test]
fn a_moved_base_is_reported_with_both_commits() {
    assert_eq!(
        compare_snapshots(&view(1, BASE, HEAD), &view(1, BASE_2, HEAD)),
        Snapshot::BaseMoved {
            from: BASE.to_owned(),
            to: BASE_2.to_owned(),
        }
    );
}

/// A head that moved is the same class of failure: the proof describes an old
/// commit, so it proves nothing about the one that would be merged.
#[test]
fn a_moved_head_is_reported_with_both_commits() {
    assert_eq!(
        compare_snapshots(&view(1, BASE, HEAD), &view(1, BASE, HEAD_2)),
        Snapshot::HeadMoved {
            from: HEAD.to_owned(),
            to: HEAD_2.to_owned(),
        }
    );
}

/// Both sides moving is the base case: the base is what the issue scopes to, and
/// reporting the base names the reason the operator can act on (rebase / wait).
#[test]
fn a_moved_base_wins_over_a_moved_head() {
    assert_eq!(
        compare_snapshots(&view(1, BASE, HEAD), &view(1, BASE_2, HEAD_2)),
        Snapshot::BaseMoved {
            from: BASE.to_owned(),
            to: BASE_2.to_owned(),
        }
    );
}

/// The base branch name is not part of the identity: only the resolved commits
/// are compared, so a renamed branch that still points at the same commit is
/// unchanged.
#[test]
fn the_branch_name_does_not_affect_the_comparison() {
    let planned = view(1, BASE, HEAD);
    let mut fresh = view(1, BASE, HEAD);
    "trunk".clone_into(&mut fresh.base_ref_name);
    assert_eq!(compare_snapshots(&planned, &fresh), Snapshot::Unchanged);
}

/// An empty reported base (a `GitHub` payload with no `baseRefOid`) that stays
/// empty is unchanged; it must not read as a move from "" to "".
#[test]
fn an_absent_base_oid_is_stable_against_itself() {
    assert_eq!(
        compare_snapshots(&view(1, "", HEAD), &view(1, "", HEAD)),
        Snapshot::Unchanged
    );
    // But gaining one *is* a change: the proof could not have used a commit it
    // never had, so the caller must retry rather than reuse the old reasoning.
    assert_eq!(
        compare_snapshots(&view(1, "", HEAD), &view(1, BASE, HEAD)),
        Snapshot::BaseMoved {
            from: String::new(),
            to: BASE.to_owned(),
        }
    );
}
