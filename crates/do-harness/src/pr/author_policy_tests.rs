//! Unit table for the author policy (#519).
//!
//! The policy decides whether automation may merge code it does not own, so the
//! cases here are the whole safety argument: the default, the one relaxation,
//! what the relaxation never covers, and what a malformed declaration does.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::pr::sweep::step::Owner;

use super::{AuthorPolicy, PrConfig, resolve};

/// The compiled-in default keeps today's behaviour.
#[test]
fn the_default_is_owned() {
    assert_eq!(AuthorPolicy::default(), AuthorPolicy::Owned);
    assert_eq!(PrConfig::default().author_policy, AuthorPolicy::Owned);
}

/// Report tokens are the contract the CLI flag and the JSON report share.
#[test]
fn policy_tokens_are_stable() {
    assert_eq!(AuthorPolicy::Owned.as_str(), "owned");
    assert_eq!(
        AuthorPolicy::OwnedOrExternalGreen.as_str(),
        "owned-or-external-green"
    );
}

/// The default policy merges ownership and nothing else.
#[test]
fn owned_never_merges_an_external_author() {
    assert!(AuthorPolicy::Owned.may_merge(Owner::Owned));
    assert!(!AuthorPolicy::Owned.may_merge(Owner::External));
    assert!(!AuthorPolicy::Owned.may_merge(Owner::UnknownIdentity));
}

/// The relaxed policy widens `External` — and only `External`.
#[test]
fn the_relaxed_policy_widens_only_external() {
    let relaxed = AuthorPolicy::OwnedOrExternalGreen;
    assert!(relaxed.may_merge(Owner::Owned));
    assert!(relaxed.may_merge(Owner::External));
    // "We could not tell whose branch this is" is not evidence of safety.
    assert!(!relaxed.may_merge(Owner::UnknownIdentity));
}

/// An unknown identity is refused by every policy, including future ones.
///
/// Written as a loop over the exhaustive list so adding a policy variant forces
/// a decision here rather than silently inheriting "merge it".
#[test]
fn no_policy_merges_an_undetermined_identity() {
    for policy in [AuthorPolicy::Owned, AuthorPolicy::OwnedOrExternalGreen] {
        assert!(
            !policy.may_merge(Owner::UnknownIdentity),
            "{policy:?} must not merge an undetermined identity"
        );
    }
}

/// A missing `author_policy` key means `owned`, never the relaxed value.
#[test]
fn an_absent_configuration_key_defaults_to_owned() {
    let config: PrConfig = toml::from_str("").unwrap();
    assert_eq!(config.author_policy, AuthorPolicy::Owned);
}

/// A malformed value is a parse error, not a silent fallback.
///
/// This is the case that matters: a repository that believes it opted in, but
/// misspelled the value, must not be quietly restricted — and one that
/// misspelled it while believing it was safe must not be quietly widened.
#[test]
fn a_malformed_value_is_an_error_not_a_default() {
    for text in [
        "author_policy = \"bogus\"",
        "author_policy = \"Owned\"",
        "author_policy = \"owned_or_external_green\"",
        "author_policy = true",
        "author_policy = 1",
    ] {
        assert!(
            toml::from_str::<PrConfig>(text).is_err(),
            "{text:?} must be rejected"
        );
    }
}

/// Both documented spellings parse.
#[test]
fn the_documented_spellings_parse() {
    let owned: PrConfig = toml::from_str("author_policy = \"owned\"").unwrap();
    assert_eq!(owned.author_policy, AuthorPolicy::Owned);
    let relaxed: PrConfig = toml::from_str("author_policy = \"owned-or-external-green\"").unwrap();
    assert_eq!(relaxed.author_policy, AuthorPolicy::OwnedOrExternalGreen);
}

/// An unknown key under `[pr]` is rejected rather than ignored.
#[test]
fn an_unknown_pr_key_is_rejected() {
    assert!(toml::from_str::<PrConfig>("authorpolicy = \"owned\"").is_err());
}

/// The flag wins over the file, so one invocation can narrow a permissive repo.
#[test]
fn the_flag_overrides_the_repository_declaration() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("do-harness.toml"),
        "[[sensors]]\nname = \"fmt\"\nargv = [\"cargo\", \"fmt\", \"--check\"]\n\n[pr]\nauthor_policy = \"owned-or-external-green\"\n",
    )
    .unwrap();
    assert_eq!(
        resolve(dir.path(), Some(AuthorPolicy::Owned)).unwrap(),
        AuthorPolicy::Owned,
        "the flag must narrow a permissive declaration"
    );
    assert_eq!(
        resolve(dir.path(), None).unwrap(),
        AuthorPolicy::OwnedOrExternalGreen
    );
}

/// A malformed file is a configuration error, not a silent default.
#[test]
fn resolve_refuses_a_malformed_configuration() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("do-harness.toml"),
        "[pr]\nauthor_policy = \"bogus\"\n",
    )
    .unwrap();
    let error = resolve(dir.path(), None).unwrap_err();
    assert!(format!("{error:#}").contains("invalid config"), "{error:#}");
    // The flag still decides when the caller stated one explicitly, so a broken
    // file cannot block a deliberately narrow invocation.
    assert_eq!(
        resolve(dir.path(), Some(AuthorPolicy::Owned)).unwrap(),
        AuthorPolicy::Owned
    );
}

/// A repository with no config file at all gets the default.
#[test]
fn a_missing_configuration_is_the_default() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(resolve(dir.path(), None).unwrap(), AuthorPolicy::Owned);
}

/// The whole `[pr]` table must survive the full `Config` round trip, because
/// that is how a real file reaches the sweep.
#[test]
fn the_pr_table_parses_inside_the_full_config() {
    let cfg: crate::config::Config = toml::from_str(
        "[[sensors]]\nname = \"fmt\"\nargv = [\"cargo\", \"fmt\", \"--check\"]\n\n[pr]\nauthor_policy = \"owned-or-external-green\"\n",
    )
    .unwrap();
    assert_eq!(cfg.pr.author_policy, AuthorPolicy::OwnedOrExternalGreen);
    // And an unknown key anywhere in the file still fails closed.
    assert!(
        toml::from_str::<crate::config::Config>(
            "[[sensors]]\nname = \"fmt\"\nargv = [\"cargo\", \"fmt\", \"--check\"]\n\n[pr]\nauthorpolicy = \"owned\"\n"
        )
        .is_err()
    );
}
