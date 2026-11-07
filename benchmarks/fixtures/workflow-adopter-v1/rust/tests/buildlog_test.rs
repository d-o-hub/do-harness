//! End-to-end CLI tests for the build-log filter fixture.
//!
//! These tests pin the entire behavioral contract of the fixture: parsing,
//! severity/component/message filtering, count aggregation, and record-order
//! preservation. The independent workload-benchmark grader additionally probes
//! selected behaviors through this same binary. The fixture is dependency-free
//! (no external crates), so assertions work on the stable JSON text shape.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_buildlog-cli");

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().expect("spawn buildlog-cli")
}

/// Extract the `records` array body from the fixed-shape JSON report.
fn records_body(stdout: &str) -> &str {
    let start = stdout.find("\"records\":[").expect("records key") + "\"records\":[".len();
    stdout[start..stdout.len() - 2].trim()
}

#[test]
fn filters_by_component_and_preserves_order() {
    let output = run(&["--component", "db", "--file", "logs/sample.log"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let records = records_body(&stdout);
    assert!(records.contains("\"component\":\"db\""));
    let first = records.find("\"severity\":\"ERROR\",\"component\":\"db\",\"message\":\"connection refused\"");
    let second = records.find("\"severity\":\"INFO\",\"component\":\"db\",\"message\":\"retry scheduled\"");
    let third = records.find("\"severity\":\"WARN\",\"component\":\"db\",\"message\":\"slow query 120ms\"");
    assert!(first.is_some_and(|index| index < second.expect("second record")));
    assert!(second.is_some_and(|index| index < third.expect("third record")));
}

#[test]
fn minimum_severity_and_substring_filter() {
    let output = run(&["--min-severity", "WARN", "--message", "query", "--file", "logs/sample.log"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"total\":1,\"by_severity\":{\"ERROR\":0,\"INFO\":0,\"WARN\":1}"));
    assert!(stdout.contains("\"severity\":\"WARN\",\"component\":\"db\",\"message\":\"slow query 120ms\""));
}

#[test]
fn counts_match_retained_records() {
    let output = run(&["--min-severity", "ERROR", "--file", "logs/sample.log"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"total\":2,\"by_severity\":{\"ERROR\":2,\"INFO\":0,\"WARN\":0}"));
    assert!(stdout.contains("\"severity\":\"ERROR\",\"component\":\"db\",\"message\":\"connection refused\""));
    assert!(stdout.contains("\"severity\":\"ERROR\",\"component\":\"net\",\"message\":\"timeout after 30s\""));
}

#[test]
fn empty_input_is_stable() {
    let output = run(&["--file", "logs/empty.log"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "{\"total\":0,\"by_severity\":{\"ERROR\":0,\"INFO\":0,\"WARN\":0},\"records\":[]}");
}

#[test]
fn unknown_option_fails_with_exit_2() {
    let output = run(&["--bogus"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn help_exits_zero() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
}
