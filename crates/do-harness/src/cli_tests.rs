//! Unit tests for CLI parsing and flag rules (`cli.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use clap::Parser;

use super::*;

#[test]
fn visible_aliases_parse_correctly() {
    let check = Cli::try_parse_from(["do-harness", "check"]).expect("check alias");
    assert!(matches!(check.command, Command::Verify { .. }));

    let ls = Cli::try_parse_from(["do-harness", "ls"]).expect("ls alias");
    assert!(matches!(ls.command, Command::List { .. }));
}

#[test]
fn verify_task_flag_requires_record() {
    let err = Cli::try_parse_from(["do-harness", "verify", "--task", "42"])
        .expect_err("task without record must fail");
    assert!(err.to_string().contains("--record"));

    let ok = Cli::try_parse_from(["do-harness", "verify", "--record", "--task", "42"])
        .expect("task with record must pass");
    if let Command::Verify { task, record, .. } = ok.command {
        assert_eq!(task, Some(42));
        assert!(record);
    } else {
        panic!("expected Command::Verify");
    }
}

#[test]
fn global_options_and_subcommands_parse() {
    let parsed = Cli::try_parse_from([
        "do-harness",
        "--root",
        "/tmp/root",
        "--config",
        "/tmp/cfg.toml",
        "-vv",
        "doctor",
        "--strict",
        "--format",
        "json",
    ])
    .expect("global options");

    assert_eq!(parsed.root.as_deref(), Some(Path::new("/tmp/root")));
    assert_eq!(parsed.config.as_deref(), Some(Path::new("/tmp/cfg.toml")));
    assert_eq!(parsed.verbose, 2);

    if let Command::Doctor { format, strict } = parsed.command {
        assert_eq!(format, Format::Json);
        assert!(strict);
    } else {
        panic!("expected Command::Doctor");
    }
}

#[test]
fn completions_and_man_subcommands_parse() {
    let comp = Cli::try_parse_from(["do-harness", "completions", "bash"]).expect("completions");
    assert!(matches!(comp.command, Command::Completions { .. }));

    let man = Cli::try_parse_from(["do-harness", "man", "/tmp/man"]).expect("man");
    assert!(matches!(man.command, Command::Man { .. }));
}

#[test]
fn pr_no_effect_parses_range_and_rejects_mixed_modes() {
    let range = Cli::try_parse_from([
        "do-harness",
        "pr",
        "no-effect",
        "--base",
        "main",
        "--head",
        "HEAD",
    ])
    .expect("range mode parses");
    assert!(matches!(range.command, Command::Pr { .. }));

    let pr = Cli::try_parse_from(["do-harness", "pr", "no-effect", "42"]).expect("pr mode parses");
    assert!(matches!(pr.command, Command::Pr { .. }));

    let conflict = Cli::try_parse_from([
        "do-harness",
        "pr",
        "no-effect",
        "42",
        "--base",
        "main",
        "--head",
        "HEAD",
    ])
    .expect_err("pr and range must conflict");
    assert!(conflict.to_string().contains("--base") || conflict.to_string().contains("--head"));

    let missing = Cli::try_parse_from(["do-harness", "pr", "no-effect", "--base", "main"])
        .expect_err("base without head must fail");
    assert!(missing.to_string().contains("--head"));
}

#[test]
fn pr_review_parses_recompute_and_rejects_mixed_modes() {
    let range = Cli::try_parse_from([
        "do-harness",
        "pr",
        "review",
        "--base",
        "main",
        "--head",
        "HEAD",
        "--recompute",
    ])
    .expect("range mode with recompute parses");
    assert!(matches!(range.command, Command::Pr { .. }));

    let pr = Cli::try_parse_from(["do-harness", "pr", "review", "42"]).expect("pr mode parses");
    assert!(matches!(pr.command, Command::Pr { .. }));

    let conflict = Cli::try_parse_from([
        "do-harness",
        "pr",
        "review",
        "42",
        "--base",
        "main",
        "--head",
        "HEAD",
    ])
    .expect_err("pr and range must conflict");
    assert!(conflict.to_string().contains("--base") || conflict.to_string().contains("--head"));

    let missing = Cli::try_parse_from(["do-harness", "pr", "review", "--head", "main"])
        .expect_err("head without base must fail");
    assert!(missing.to_string().contains("--base"));
}
