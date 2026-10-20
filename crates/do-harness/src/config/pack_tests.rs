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
    assert_eq!(
        sensor("fmt").when_changed,
        vec!["**/*.rs", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]
    );
    assert_eq!(
        sensor("check").when_changed,
        vec!["**/*.rs", "Cargo.toml", "Cargo.lock"]
    );
    assert_eq!(
        sensor("clippy").when_changed,
        sensor("check").when_changed.clone()
    );
    assert_eq!(
        sensor("test").when_changed,
        sensor("check").when_changed.clone()
    );
    assert_eq!(sensor("loc").when_changed, vec!["**/*.rs"]);
    for name in ["deps", "audit", "commitlint"] {
        assert_eq!(sensor(name).when_changed, Vec::<String>::new());
    }
}
