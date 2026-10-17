//! The command-contract model and the Clap walk that builds it.
//!
//! Facts, not prose: every visible command path (no depth cap) with its
//! aliases, usage, own arguments, and visible subcommands. Arguments the root
//! marks `global = true` are recorded once on the root — clap copies them into
//! every subcommand when the tree is built, and re-documenting an inherited
//! flag under each command would destroy flag ownership.

use std::collections::BTreeSet;

use clap::{Arg, ArgAction, Command, CommandFactory};

use crate::cli::Cli;

/// Schema version of the exported contract (`tests/fixtures/cli-contract.json`).
pub const SCHEMA_VERSION: u32 = 1;

/// Whether an argument is a positional or an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgKind {
    /// A positional argument (`<TITLE>`, `[PR]`).
    Positional,
    /// A `--flag` or `--option <VALUE>`.
    Option,
}

/// One command-line argument, owned by exactly one command path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgContract {
    /// Clap argument id (the Rust field name).
    pub id: String,
    /// Positional or option.
    pub kind: ArgKind,
    /// Single-character short flag (`-q`), when present.
    pub short: Option<char>,
    /// Long name without the leading dashes (`quiet`), when present.
    pub long: Option<String>,
    /// Value placeholders clap records (`FILE`, `SENSOR`); flags carry a
    /// derived name (the uppercased id) as well, and [`ArgContract::takes_value`]
    /// decides whether a placeholder is rendered.
    pub value_names: Vec<String>,
    /// Whether the argument must be provided.
    pub required: bool,
    /// Whether the argument may be repeated (`ArgAction::Append`).
    pub repeatable: bool,
    /// Whether the argument consumes a value.
    pub takes_value: bool,
    /// Default values, formatted as strings.
    pub default_values: Vec<String>,
    /// Accepted values of an enumerated argument, in clap's order.
    pub possible_values: Vec<String>,
    /// Short help (the `help`, never the `long_help`, string).
    pub help: Option<String>,
    /// Positional declaration index; `None` for options.
    pub index: Option<usize>,
}

/// One visible command path and its own mechanical facts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandContract {
    /// Space-separated full path (`pr review`); empty for the root command.
    pub path: String,
    /// Short help from `about`.
    pub about: Option<String>,
    /// Rendered usage line, including the `do-harness` prefix.
    pub usage: String,
    /// Visible aliases (`check` for `verify`).
    pub aliases: Vec<String>,
    /// Arguments owned by this path.
    pub args: Vec<ArgContract>,
    /// Visible subcommand names, in declaration order.
    pub subcommands: Vec<String>,
    /// Whether the path can be invoked without selecting a subcommand.
    pub invocable: bool,
}

/// The full contract: root globals plus every visible command path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliContract {
    /// Contract schema version ([`SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// The root command: usage and the global argument set.
    pub root: CommandContract,
    /// Every visible command path, depth-first in declaration order.
    pub commands: Vec<CommandContract>,
}

/// Builds the contract from the real CLI definitions.
pub fn build() -> CliContract {
    let mut command = Cli::command();
    build_from(&mut command)
}

/// Builds the contract from any Clap definition (the fixtures use this).
pub fn build_from(command: &mut Command) -> CliContract {
    command.build();
    let globals = global_ids(command);
    let root = command_contract(command, "", &globals);
    let mut commands = Vec::new();
    walk(command, "", &globals, &mut commands);
    CliContract {
        schema_version: SCHEMA_VERSION,
        root,
        commands,
    }
}

/// Argument ids the root marks global: recorded on the root only.
fn global_ids(root: &Command) -> BTreeSet<String> {
    root.get_arguments()
        .filter(|arg| arg.is_global_set())
        .map(|arg| arg.get_id().to_string())
        .collect()
}

/// Depth-first walk of every visible subcommand path.
fn walk(
    parent: &mut Command,
    prefix: &str,
    globals: &BTreeSet<String>,
    out: &mut Vec<CommandContract>,
) {
    let names: Vec<String> = visible_subcommands(parent)
        .map(|command| command.get_name().to_string())
        .collect();
    for name in names {
        let child = parent
            .find_subcommand_mut(&name)
            .expect("subcommand was just listed");
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix} {name}")
        };
        out.push(command_contract(child, &path, globals));
        walk(child, &path, globals, out);
    }
}

/// Subcommands a user can see: hidden, empty-named, and the autogenerated
/// `help` are not part of the documented surface.
fn visible_subcommands(command: &Command) -> impl Iterator<Item = &Command> {
    command
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set() && !sub.get_name().is_empty() && sub.get_name() != "help")
}

/// Own arguments, usage, aliases, and subcommands of one command.
fn command_contract(
    command: &mut Command,
    path: &str,
    globals: &BTreeSet<String>,
) -> CommandContract {
    let subcommands: Vec<String> = visible_subcommands(command)
        .map(|sub| sub.get_name().to_string())
        .collect();
    let invocable = subcommands.is_empty() || !command.is_subcommand_required_set();
    let aliases = command.get_visible_aliases().map(str::to_string).collect();
    let about = command
        .get_about()
        .map(|about| sanitize_text(&about.to_string()))
        .filter(|text| !text.is_empty());
    let usage = usage_line(command);
    let args = command
        .get_arguments()
        .filter(|arg| !is_builtin(arg))
        .filter(|arg| path.is_empty() || !globals.contains(arg.get_id().as_str()))
        .map(arg_contract)
        .collect();
    CommandContract {
        path: path.to_string(),
        about,
        usage,
        aliases,
        args,
        subcommands,
        invocable,
    }
}

/// The `--help` and `--version` flags clap adds are not contract surface.
fn is_builtin(arg: &Arg) -> bool {
    matches!(arg.get_id().as_str(), "help" | "version")
}

/// The command's usage line, collapsed; clap already writes the full command
/// chain, bin name included (`do-harness pr review [OPTIONS] [PR]`).
fn usage_line(command: &mut Command) -> String {
    let rendered = command.render_usage().to_string();
    let trimmed = rendered.trim();
    let without_label = trimmed.strip_prefix("Usage: ").map_or(trimmed, str::trim);
    sanitize_text(without_label)
}

/// Extracts one argument's mechanical facts.
///
/// `SetTrue`/`SetFalse` actions carry an internal `default_value("false")` /
/// `("true")` that clap's own help never displays; flags therefore record no
/// defaults, matching what a reader of `--help` sees.
fn arg_contract(arg: &Arg) -> ArgContract {
    let action = arg.get_action();
    let takes_value = action.takes_values();
    ArgContract {
        id: arg.get_id().to_string(),
        kind: if arg.is_positional() {
            ArgKind::Positional
        } else {
            ArgKind::Option
        },
        short: arg.get_short(),
        long: arg.get_long().map(str::to_string),
        value_names: arg
            .get_value_names()
            .map(|names| names.iter().map(ToString::to_string).collect())
            .unwrap_or_default(),
        required: arg.is_required_set(),
        repeatable: matches!(action, ArgAction::Append | ArgAction::Count),
        takes_value,
        default_values: if takes_value {
            arg.get_default_values()
                .iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect()
        } else {
            Vec::new()
        },
        possible_values: arg
            .get_possible_values()
            .iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_name().to_string())
            .collect(),
        help: arg
            .get_help()
            .map(|help| sanitize_text(&help.to_string()))
            .filter(|text| !text.is_empty()),
        index: arg.get_index(),
    }
}

/// Collapses whitespace runs so cells and blocks are single-line and stable.
pub fn sanitize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
