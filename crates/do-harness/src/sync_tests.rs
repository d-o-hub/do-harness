//! Unit coverage for the `sync` parsers.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::git::{parse_ls_remote, short, validate_object_id};

const SHA: &str = "29d52f776146c3d898382ea045d70fd5211aaa09";

#[test]
fn parses_one_ref_line() {
    let stdout = format!("{SHA}\trefs/heads/main\n");
    assert_eq!(parse_ls_remote(&stdout).unwrap(), SHA);
}

#[test]
fn ignores_blank_lines() {
    let stdout = format!("\n{SHA}\trefs/heads/main\n\n");
    assert_eq!(parse_ls_remote(&stdout).unwrap(), SHA);
}

#[test]
fn rejects_an_empty_response() {
    let error = parse_ls_remote("\n\n").unwrap_err().to_string();
    assert!(error.contains("no ref match"), "{error}");
}

#[test]
fn rejects_multiple_lines() {
    let stdout = format!("{SHA}\trefs/heads/main\n{SHA}\trefs/heads/other\n");
    let error = parse_ls_remote(&stdout).unwrap_err().to_string();
    assert!(error.contains("more than one"), "{error}");
}

#[test]
fn rejects_the_wrong_ref() {
    let stdout = format!("{SHA}\trefs/heads/other\n");
    let error = parse_ls_remote(&stdout).unwrap_err().to_string();
    assert!(error.contains("unexpected ref match"), "{error}");
}

#[test]
fn rejects_extra_columns() {
    let stdout = format!("{SHA}\trefs/heads/main\textra\n");
    let error = parse_ls_remote(&stdout).unwrap_err().to_string();
    assert!(error.contains("malformed"), "{error}");
}

#[test]
fn accepts_sha256_object_ids() {
    let sha = "a".repeat(64);
    validate_object_id(&sha).unwrap();
}

#[test]
fn rejects_malformed_object_ids() {
    for candidate in ["", "abc", &"g".repeat(40), &"a".repeat(41)] {
        assert!(
            validate_object_id(candidate).is_err(),
            "{candidate:?} must be rejected"
        );
    }
}

#[test]
fn shortens_to_seven_characters() {
    assert_eq!(short(SHA), "29d52f7");
    assert_eq!(short("abc"), "abc");
}
