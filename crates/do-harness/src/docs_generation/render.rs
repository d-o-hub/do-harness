//! Deterministic rendering of the contract into committed artifacts.
//!
//! Every renderer is pure and returns LF text without a trailing blank line:
//! the sync layer compares these bytes (newline-normalized) against the
//! committed regions and splices exactly this content back in.

use std::fmt::Write as _;

use super::contract::{ArgContract, ArgKind, CliContract, CommandContract, sanitize_text};

/// The `README.md` command index: one row per invocable path plus the global
/// flag summary line.
pub fn readme_block(contract: &CliContract) -> String {
    let mut out = String::from("| Command | Description |\n| --- | --- |\n");
    for command in &contract.commands {
        if !command.invocable {
            continue;
        }
        let mut description = command.about.clone().unwrap_or_else(|| "—".to_string());
        let aliases: Vec<String> = command
            .aliases
            .iter()
            .map(|alias| format!("`{alias}`"))
            .collect();
        match aliases.as_slice() {
            [alias] => {
                let _ = write!(description, " (alias: {alias})");
            }
            [] => {}
            _ => {
                let _ = write!(description, " (aliases: {})", aliases.join(", "));
            }
        }
        let flags: Vec<String> = command
            .args
            .iter()
            .filter(|arg| arg.kind == ArgKind::Option)
            .map(|arg| format!("`{}`", argument_syntax(arg)))
            .collect();
        if !flags.is_empty() {
            description.push_str(" — flags: ");
            description.push_str(&flags.join(", "));
        }
        let synopsis = command
            .usage
            .strip_prefix("do-harness ")
            .unwrap_or(&command.usage)
            .trim();
        let _ = writeln!(
            out,
            "| `{}` | {} |",
            escape_cell(synopsis),
            escape_cell(&description)
        );
    }
    let globals: Vec<String> = contract
        .root
        .args
        .iter()
        .map(|arg| format!("`{}`", argument_syntax(arg)))
        .collect();
    let _ = writeln!(out, "\nGlobal flags: {}.", globals.join(", "));
    out
}

/// The `docs/cli.md` block for the root global options.
pub fn globals_block(contract: &CliContract) -> String {
    format!(
        "`{}`\n\n{}",
        contract.root.usage,
        args_table(&contract.root.args)
    )
}

/// The `docs/cli.md` block for one command path.
pub fn command_block(command: &CommandContract) -> String {
    let mut sections = vec![format!("`{}`", command.usage)];
    if !command.aliases.is_empty() {
        let aliases: Vec<String> = command
            .aliases
            .iter()
            .map(|alias| format!("`{alias}`"))
            .collect();
        sections.push(format!("Aliases: {}.", aliases.join(", ")));
    }
    if !command.args.is_empty() {
        sections.push(args_table(&command.args));
    }
    if !command.subcommands.is_empty() {
        let subcommands: Vec<String> = command
            .subcommands
            .iter()
            .map(|name| format!("`{name}`"))
            .collect();
        sections.push(format!("Subcommands: {}.", subcommands.join(", ")));
    }
    let mut block = sections.join("\n\n");
    while block.ends_with('\n') {
        block.pop();
    }
    block.push('\n');
    block
}

/// The serialized command contract shared with external tooling.
pub fn contract_json(contract: &CliContract) -> String {
    let mut json = serde_json::to_string_pretty(contract).expect("contract serializes to JSON");
    json.push('\n');
    json
}

/// One markdown table of arguments, in the order the walk recorded them.
fn args_table(args: &[ArgContract]) -> String {
    let mut out = String::from(
        "| Argument | Required | Default | Possible values | Description |\n\
         | --- | --- | --- | --- | --- |\n",
    );
    for arg in args {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            escape_cell(&argument_cell(arg)),
            if arg.required { "yes" } else { "no" },
            code_list(&arg.default_values),
            code_list(&arg.possible_values),
            escape_cell(arg.help.as_deref().unwrap_or("—")),
        );
    }
    out
}

/// The call syntax of one argument: `-o, --output <FILE>`, `<TITLE>`, `[PR]`.
fn argument_syntax(arg: &ArgContract) -> String {
    match arg.kind {
        ArgKind::Positional => {
            let names: Vec<String> = arg
                .value_names
                .iter()
                .map(|name| format!("<{name}>"))
                .collect();
            let rendered = if names.is_empty() {
                format!("<{}>", arg.id)
            } else {
                names.join(" ")
            };
            if arg.required {
                rendered
            } else {
                rendered.replace('<', "[").replace('>', "]")
            }
        }
        ArgKind::Option => {
            let mut head = String::new();
            if let Some(short) = arg.short {
                let _ = write!(head, "-{short}");
            }
            if let Some(long) = &arg.long {
                if !head.is_empty() {
                    head.push_str(", ");
                }
                let _ = write!(head, "--{long}");
            }
            if !arg.takes_value {
                return head;
            }
            let values: Vec<String> = arg
                .value_names
                .iter()
                .map(|name| format!("<{name}>"))
                .collect();
            if values.is_empty() {
                head
            } else {
                format!("{head} {}", values.join(" "))
            }
        }
    }
}

/// [`argument_syntax`] backticked, plus the repeat marker for appended options.
fn argument_cell(arg: &ArgContract) -> String {
    let syntax = argument_syntax(arg);
    if arg.repeatable {
        format!("`{syntax}` (repeatable)")
    } else {
        format!("`{syntax}`")
    }
}

/// Backticked, comma-joined values, or an em dash when the list is empty.
fn code_list(values: &[String]) -> String {
    if values.is_empty() {
        "—".to_string()
    } else {
        values
            .iter()
            .map(|value| format!("`{}`", escape_cell(value)))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Collapses whitespace and escapes `|` so a value survives a table cell.
pub fn escape_cell(text: &str) -> String {
    sanitize_text(text).replace('|', "\\|")
}
