//! Shared fixture helpers: synthetic contracts and temporary repositories.

use std::fs;
use std::path::Path;

use crate::docs_generation::contract::{
    ArgContract, ArgKind, CliContract, CommandContract, SCHEMA_VERSION,
};

/// Writes a fixture file, creating parent directories.
pub fn write_file(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("relative path has a parent"))
        .expect("create fixture dirs");
    fs::write(&path, content).expect("write fixture file");
}

/// Renders a marker pair around `content`; `content` must end with a newline.
pub fn region(id: &str, content: &str) -> String {
    format!("<!-- BEGIN GENERATED: {id} -->\n{content}<!-- END GENERATED: {id} -->\n")
}

/// An option argument with clap-like facts, no value.
pub fn flag(id: &str, short: Option<char>, help: &str) -> ArgContract {
    ArgContract {
        id: id.to_string(),
        kind: ArgKind::Option,
        short,
        long: Some(id.to_string()),
        value_names: Vec::new(),
        required: false,
        repeatable: false,
        takes_value: false,
        default_values: Vec::new(),
        possible_values: Vec::new(),
        help: Some(help.to_string()),
        index: None,
    }
}

/// An enumerable option with a default, like `--format`.
pub fn value_option(
    id: &str,
    value_name: &str,
    default: &str,
    values: &[&str],
    help: &str,
) -> ArgContract {
    ArgContract {
        id: id.to_string(),
        kind: ArgKind::Option,
        short: None,
        long: Some(id.to_string()),
        value_names: vec![value_name.to_string()],
        required: false,
        repeatable: false,
        takes_value: true,
        default_values: vec![default.to_string()],
        possible_values: values.iter().map(|value| (*value).to_string()).collect(),
        help: Some(help.to_string()),
        index: None,
    }
}

/// A command with no arguments and no subcommands.
pub fn leaf(path: &str, about: &str, invocable: bool) -> CommandContract {
    CommandContract {
        path: path.to_string(),
        about: Some(about.to_string()),
        usage: format!("do-harness {path}"),
        aliases: Vec::new(),
        args: Vec::new(),
        subcommands: Vec::new(),
        invocable,
    }
}

/// A hand-built contract for renderer fixtures: one invocable leaf with an
/// alias and an enumerable option, plus one non-invocable namespace.
pub fn sample_contract() -> CliContract {
    let mut alpha = leaf("alpha", "Alpha command", true);
    alpha.aliases = vec!["a".to_string()];
    alpha.usage = "do-harness alpha [OPTIONS]".to_string();
    alpha.args = vec![value_option(
        "format",
        "FORMAT",
        "text",
        &["text", "json"],
        "Output format",
    )];
    alpha.args[0].short = Some('q');
    let namespace = leaf("alpha beta", "Beta namespace", false);
    CliContract {
        schema_version: SCHEMA_VERSION,
        root: CommandContract {
            path: String::new(),
            about: Some("Sample CLI".to_string()),
            usage: "do-harness [OPTIONS] <COMMAND>".to_string(),
            aliases: Vec::new(),
            args: vec![flag("quiet", Some('q'), "Suppress verdict lines")],
            subcommands: vec!["alpha".to_string()],
            invocable: false,
        },
        commands: vec![alpha, namespace],
    }
}
