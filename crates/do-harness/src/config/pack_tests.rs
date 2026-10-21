#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// The built-in pack must not carry `--no-tests=pass`: a workspace whose suite
/// shrank to zero has to fail its `test` sensor instead of reporting green
/// (AGENTS.md section 4, "No Vacuous Gates").
///
/// `tests/vacuous_gates.rs` proves the behaviour end to end where cargo-nextest
/// is installed; this pin holds on every host.
#[test]
fn builtin_rust_pack_test_sensor_fails_on_zero_tests() {
    let pack = rust_pack();
    let test = pack
        .iter()
        .find(|sensor| sensor.name == "test")
        .expect("the built-in pack ships a test sensor");
    assert_eq!(
        test.argv,
        vec!["cargo", "nextest", "run", "--workspace"],
        "nextest must fail the run when it executes zero tests"
    );
}

/// The built-in default ships feedback/verification/release signal sets.
#[test]
fn rust_default_ships_signal_sets() {
    let cfg = rust_default();
    assert_eq!(
        cfg.signal_sets.get("feedback"),
        Some(&vec![
            "fmt".to_owned(),
            "check".to_owned(),
            "clippy".to_owned()
        ])
    );
    assert_eq!(cfg.signal_sets.get("verification").unwrap().len(), 9);
    assert_eq!(cfg.signal_sets.get("release").unwrap().len(), 10);
    cfg.validate().expect("built-in default must validate");
}

/// The built-in pack pins `--all-targets` on clippy and narrows the cargo
/// sensors with `when-changed` globs so `verify --changed` is meaningful.
///
/// The exact glob vectors are not pinned here: they are an implementation
/// detail of the pack, and the selection behaviour they produce is proved
/// against the built-in pack and the shipped policy in
/// `crate::applicability`'s tests, where a wrong vector fails as a skipped
/// sensor instead of as a changed literal.
#[test]
fn rust_pack_pins_all_targets_and_when_changed() {
    let sensors = rust_pack();
    let sensor = |name: &str| sensors.iter().find(|spec| spec.name == name).unwrap();
    assert!(
        sensor("clippy")
            .argv
            .iter()
            .any(|arg| arg == "--all-targets")
    );
    for name in ["fmt", "check", "clippy", "test", "doctest", "coverage"] {
        assert_ne!(sensor(name).when_changed, Vec::<String>::new());
    }
    for name in ["check", "clippy", "test", "doctest", "coverage"] {
        assert_eq!(
            sensor(name).when_changed,
            sensor("check").when_changed.clone(),
            "{name} must narrow its applicability the same way check does"
        );
    }
    assert_eq!(sensor("loc").when_changed, vec!["**/*.rs"]);
    for name in ["deps", "audit", "commitlint"] {
        assert_eq!(sensor(name).when_changed, Vec::<String>::new());
    }
}
