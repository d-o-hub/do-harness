//! The Clap walk: paths, depth, globals, aliases, and hidden commands.

use std::collections::BTreeSet;

use clap::{Arg, ArgAction, Command};

use crate::docs_generation::contract::{ArgKind, build, build_from};

/// A small synthetic tree with depth, aliases, a global, and a hidden command.
fn synthetic_tree() -> Command {
    Command::new("tools")
        .arg(
            Arg::new("root")
                .long("root")
                .global(true)
                .help("Workspace root override"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Verbose output"),
        )
        .subcommand(
            Command::new("alpha")
                .about("Alpha command")
                .visible_alias("a")
                .arg(
                    Arg::new("format")
                        .long("format")
                        .default_value("text")
                        .value_parser(["text", "json"])
                        .help("Output format"),
                )
                .arg(
                    Arg::new("only")
                        .long("only")
                        .action(ArgAction::Append)
                        .value_name("NAME")
                        .help("Repeatable filter"),
                )
                .arg(Arg::new("title").required(true).help("Title positional"))
                .subcommand(Command::new("secret").hide(true))
                .subcommand(
                    Command::new("beta")
                        .about("Beta namespace")
                        .subcommand_required(true)
                        .subcommand(Command::new("gamma").about("Gamma leaf")),
                ),
        )
}

#[test]
fn walker_covers_paths_globals_aliases_and_hidden_commands() {
    let mut tree = synthetic_tree();
    let contract = build_from(&mut tree);
    let paths: Vec<&str> = contract
        .commands
        .iter()
        .map(|command| command.path.as_str())
        .collect();
    assert_eq!(
        paths,
        ["alpha", "alpha beta", "alpha beta gamma"],
        "hidden and help are dropped"
    );
    assert_eq!(contract.root.subcommands, ["alpha"]);

    let alpha = &contract.commands[0];
    let ids: Vec<&str> = alpha.args.iter().map(|arg| arg.id.as_str()).collect();
    assert_eq!(
        ids,
        ["format", "only", "title"],
        "the propagated global is not re-documented"
    );
    assert_eq!(alpha.aliases, ["a"]);
    assert!(
        alpha.invocable,
        "beta is optional even when it has its own required subcommand"
    );
    assert!(alpha.usage.starts_with("tools alpha"), "{}", alpha.usage);

    let format = &alpha.args[0];
    assert_eq!(format.default_values, ["text"]);
    assert_eq!(format.possible_values, ["text", "json"]);
    assert!(format.takes_value);
    assert!(alpha.args[1].repeatable);
    assert_eq!(alpha.args[2].kind, ArgKind::Positional);
    assert!(alpha.args[2].required);

    let beta = &contract.commands[1];
    assert!(
        !beta.invocable,
        "required subcommands make a namespace non-invocable"
    );
    assert_eq!(contract.root.args.len(), 2, "root keeps its own globals");
}

#[test]
fn real_cli_contract_keeps_flag_ownership_and_key_paths() {
    let contract = build();
    let paths: Vec<&str> = contract
        .commands
        .iter()
        .map(|command| command.path.as_str())
        .collect();
    for expected in [
        "verify",
        "init",
        "pr review",
        "pr ready",
        "task add",
        "metrics pr",
        "skills drift",
    ] {
        assert!(
            paths.contains(&expected),
            "missing path `{expected}` in {paths:?}"
        );
    }
    let globals: BTreeSet<&str> = contract
        .root
        .args
        .iter()
        .map(|arg| arg.id.as_str())
        .collect();
    assert!(
        globals.contains("dry_run"),
        "the root global set is recorded on the root"
    );
    for command in &contract.commands {
        for arg in &command.args {
            assert!(
                !globals.contains(arg.id.as_str()),
                "`{}` re-documents the global `{}`",
                command.path,
                arg.id
            );
        }
        assert!(
            command.usage.starts_with("do-harness "),
            "usage: {}",
            command.usage
        );
        assert!(!command.subcommands.iter().any(|name| name == "help"));
    }
    let verify = contract
        .commands
        .iter()
        .find(|command| command.path == "verify")
        .expect("verify exists");
    assert_eq!(verify.aliases, ["check"]);
    let task = contract
        .commands
        .iter()
        .find(|command| command.path == "task")
        .expect("task exists");
    assert!(!task.invocable);
    assert!(task.subcommands.contains(&"add".to_string()));
}
