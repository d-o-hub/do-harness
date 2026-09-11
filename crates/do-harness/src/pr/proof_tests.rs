//! Fixtures for the deterministic proof-skipping guard.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::diff::{Change, HEADER_MODE_CHANGE, HEADER_RENAME_ONLY, Unit};
use super::proof::{Matcher, ProofRules, Verdict};

fn unit(path: &str, header: Option<&str>, lines: &[&str]) -> Unit {
    Unit {
        id: format!("{path}:0"),
        path: path.to_owned(),
        change: Change::Modified,
        old_start: 0,
        new_start: 0,
        header: header.map(str::to_owned),
        lines: lines.iter().map(|line| (*line).to_owned()).collect(),
    }
}

fn rules(mechanical: &[&str], behavioral: &[&str]) -> ProofRules {
    ProofRules {
        mechanical: mechanical.iter().map(|item| (*item).to_owned()).collect(),
        behavioral: behavioral.iter().map(|item| (*item).to_owned()).collect(),
    }
}

#[test]
fn no_policy_proves_nothing() {
    let matcher = Matcher::compile(None);
    let lockfile = unit("Cargo.lock", None, &["+dep = \"1\""]);
    assert_eq!(matcher.evaluate(&lockfile), Verdict::Residual);
    let rename = unit("old.rs", Some(HEADER_RENAME_ONLY), &[]);
    assert_eq!(matcher.evaluate(&rename), Verdict::Residual);
}

#[test]
fn mechanical_glob_is_proven_and_others_stay_residual() {
    let matcher = Matcher::compile(Some(&rules(&["**/Cargo.lock"], &[])));
    assert!(matcher.warnings().is_empty());
    let lockfile = unit("Cargo.lock", None, &["+dep = \"1\""]);
    assert_eq!(matcher.evaluate(&lockfile), Verdict::Proven);
    let source = unit("src/lib.rs", None, &["+fn a() {}"]);
    assert_eq!(matcher.evaluate(&source), Verdict::Residual);
}

#[test]
fn behavioral_rule_overrides_and_revokes_mechanical_claims() {
    let matcher = Matcher::compile(Some(&rules(&["**"], &["crates/**"])));
    let revoked = matcher.evaluate(&unit("crates/core/src/lib.rs", None, &["+x"]));
    match revoked {
        Verdict::Revoked { reason } => assert!(reason.contains("both"), "{reason}"),
        other => panic!("expected revocation, got {other:?}"),
    }
    let proven = matcher.evaluate(&unit("docs/readme.md", None, &["+x"]));
    assert_eq!(proven, Verdict::Proven);
}

#[test]
fn behavioral_rule_keeps_structural_units_residual() {
    let matcher = Matcher::compile(Some(&rules(&["docs/**"], &["crates/**"])));
    let structural = unit("crates/core/src/lib.rs", Some(HEADER_RENAME_ONLY), &[]);
    assert_eq!(matcher.evaluate(&structural), Verdict::Residual);
}

#[test]
fn structural_proofs_need_a_policy_but_no_glob() {
    let matcher = Matcher::compile(Some(&rules(&[], &[])));
    assert_eq!(
        matcher.evaluate(&unit("old.rs", Some(HEADER_RENAME_ONLY), &[])),
        Verdict::Proven
    );
    assert_eq!(
        matcher.evaluate(&unit("run.sh", Some(HEADER_MODE_CHANGE), &[])),
        Verdict::Proven
    );
    assert_eq!(
        matcher.evaluate(&unit("old.rs", Some(HEADER_RENAME_ONLY), &["+x"])),
        Verdict::Residual
    );
}

#[test]
fn policy_path_is_never_proven() {
    let matcher = Matcher::compile(Some(&rules(&["**/*.toml"], &[])));
    let claim = matcher.evaluate(&unit(".github/pr-gate.toml", None, &["+x"]));
    assert!(matches!(claim, Verdict::Revoked { .. }), "{claim:?}");
}

#[test]
fn invalid_glob_disables_all_proofs() {
    let matcher = Matcher::compile(Some(&rules(&["["], &[])));
    assert!(!matcher.warnings().is_empty());
    let lockfile = unit("Cargo.lock", None, &["+dep = \"1\""]);
    assert_eq!(matcher.evaluate(&lockfile), Verdict::Residual);
}
