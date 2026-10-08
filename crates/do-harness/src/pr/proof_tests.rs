//! Fixtures for the deterministic proof-skipping guard.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::diff::{Change, HEADER_MODE_CHANGE, HEADER_RENAME_ONLY, Unit};
use super::proof::{Matcher, ProofRules, Verdict};

fn unit(path: &str, header: Option<&str>, lines: &[&str]) -> Unit {
    Unit {
        id: format!("{path}:0"),
        path: path.to_owned(),
        old_path: None,
        old_mode: None,
        new_mode: None,
        change: Change::Modified,
        old_start: 0,
        new_start: 0,
        header: header.map(str::to_owned),
        lines: lines.iter().map(|line| (*line).to_owned()).collect(),
    }
}

fn rename_unit(old_path: &str, path: &str, header: Option<&str>, lines: &[&str]) -> Unit {
    Unit {
        id: format!("{path}:0"),
        path: path.to_owned(),
        old_path: Some(old_path.to_owned()),
        old_mode: None,
        new_mode: None,
        change: Change::Renamed,
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
fn mechanical_glob_is_exempt_and_others_stay_residual() {
    let matcher = Matcher::compile(Some(&rules(&["**/Cargo.lock"], &[])));
    assert_eq!(matcher.warnings().len(), 0);
    let lockfile = unit("Cargo.lock", None, &["+dep = \"1\""]);
    assert_eq!(matcher.evaluate(&lockfile), Verdict::Exempt);
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
    let exempt = matcher.evaluate(&unit("docs/readme.md", None, &["+x"]));
    assert_eq!(exempt, Verdict::Exempt);
}

#[test]
fn behavioral_rule_keeps_structural_units_residual() {
    let matcher = Matcher::compile(Some(&rules(&["docs/**"], &["crates/**"])));
    let structural = unit("crates/core/src/lib.rs", Some(HEADER_RENAME_ONLY), &[]);
    assert_eq!(matcher.evaluate(&structural), Verdict::Residual);
}

#[test]
fn structural_units_without_matching_glob_remain_residual() {
    let matcher = Matcher::compile(Some(&rules(&[], &[])));
    assert_eq!(
        matcher.evaluate(&rename_unit(
            "old.rs",
            "new.rs",
            Some(HEADER_RENAME_ONLY),
            &[]
        )),
        Verdict::Residual
    );
    assert_eq!(
        matcher.evaluate(&unit("run.sh", Some(HEADER_MODE_CHANGE), &[])),
        Verdict::Residual
    );
}

#[test]
fn both_rename_endpoints_participate_in_matching() {
    let matcher = Matcher::compile(Some(&rules(&["docs/**"], &["crates/**"])));
    // Move out of behavioral area into mechanical area is revoked
    let move_out = rename_unit("crates/core/lib.rs", "docs/lib.rs", None, &["+x"]);
    assert!(matches!(
        matcher.evaluate(&move_out),
        Verdict::Revoked { .. }
    ));

    // Move out of protected area into mechanical area is revoked
    let matcher_all = Matcher::compile(Some(&rules(&["docs/**"], &[])));
    let move_protected = rename_unit(".github/pr-gate.toml", "docs/gate.toml", None, &["+x"]);
    assert!(matches!(
        matcher_all.evaluate(&move_protected),
        Verdict::Revoked { .. }
    ));

    // Move within mechanical area is exempt
    let move_mechanical = rename_unit("docs/old.md", "docs/new.md", None, &["+x"]);
    assert_eq!(matcher_all.evaluate(&move_mechanical), Verdict::Exempt);
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
    assert_ne!(matcher.warnings().len(), 0);
    let lockfile = unit("Cargo.lock", None, &["+dep = \"1\""]);
    assert_eq!(matcher.evaluate(&lockfile), Verdict::Residual);
}
