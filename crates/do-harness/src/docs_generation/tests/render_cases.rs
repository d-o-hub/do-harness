//! Deterministic rendering: tables, escaping, blocks, and JSON round-trip.

use crate::docs_generation::contract::CliContract;
use crate::docs_generation::render::{command_block, contract_json, escape_cell, readme_block};
use crate::docs_generation::tests::support::{leaf, sample_contract, value_option};

#[test]
fn readme_rows_cover_invocable_paths_with_aliases_and_flags() {
    let table = readme_block(&sample_contract());
    assert!(
        table.contains(
            "| `alpha [OPTIONS]` | Alpha command (alias: `a`) — flags: `-q, --format <FORMAT>` |"
        ),
        "{table}"
    );
    assert!(
        !table.contains("alpha beta"),
        "non-invocable namespaces get no row:\n{table}"
    );
    assert!(table.ends_with("Global flags: `-q, --quiet`.\n"));
}

#[test]
fn command_blocks_render_usage_aliases_table_and_subcommands() {
    let block = command_block(&sample_contract().commands[0]);
    assert_eq!(
        block,
        "`do-harness alpha [OPTIONS]`\n\n\
         Aliases: `a`.\n\n\
         | Argument | Required | Default | Possible values | Description |\n\
         | --- | --- | --- | --- | --- |\n\
         | `-q, --format <FORMAT>` | no | `text` | `text`, `json` | Output format |\n"
    );

    let mut namespace = leaf(
        "task",
        "Task state inspection and workflow management",
        false,
    );
    namespace.usage = "do-harness task <COMMAND>".to_string();
    namespace.subcommands = vec!["add".to_string(), "done".to_string()];
    let block = command_block(&namespace);
    assert!(block.contains("Subcommands: `add`, `done`."), "{block}");
}

#[test]
fn help_with_pipes_and_markdown_characters_survives_a_table_cell() {
    let mut command = leaf("alpha", "Alpha command", true);
    let arg = value_option(
        "mode",
        "MODE",
        "text",
        &["text", "json"],
        "Pipe | star *em* `code` <https://example.com/a|b>\nsecond  line",
    );
    command.args = vec![arg];
    let block = command_block(&command);
    let row = block.lines().last().expect("table row");
    assert!(
        row.contains(r"Pipe \| star *em* `code` <https://example.com/a\|b> second line"),
        "{row}"
    );
    assert_eq!(
        row.matches(" | ").count(),
        4,
        "the row keeps exactly five cells: {row}"
    );
}

#[test]
fn escape_cell_collapses_whitespace_and_escapes_pipes() {
    assert_eq!(escape_cell("a | b"), "a \\| b");
    assert_eq!(escape_cell("multi\nline\ttext"), "multi line text");
}

#[test]
fn contract_json_round_trips_and_is_stable() {
    let contract = sample_contract();
    let json = contract_json(&contract);
    assert!(json.ends_with('\n'));
    let parsed: CliContract = serde_json::from_str(&json).expect("contract JSON parses");
    assert_eq!(parsed, contract);
    assert_eq!(
        json,
        contract_json(&sample_contract()),
        "rendering is deterministic"
    );
}
