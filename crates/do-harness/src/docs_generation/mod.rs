//! Mechanical CLI documentation generation (compiled only for tests).
//!
//! The CLI definitions are the single source of truth for the mechanical
//! facts a reference document must repeat: command paths, aliases, usage
//! lines, argument arity, required status, defaults, possible values, and
//! help text. This module reflects over `Cli::command()` and renders:
//!
//! - the delimited command table in `README.md` (`cli-commands`);
//! - the global-options and per-command blocks in `docs/cli.md`
//!   (`cli-globals`, `cli-contract <path>`);
//! - the machine-readable contract at `tests/fixtures/cli-contract.json`
//!   that example tooling consumes instead of scraping formatted help.
//!
//! `sync::generated_docs_are_in_sync` is the maintenance gate. Plain
//! `cargo test` compares the committed blocks against the render
//! (newline-normalized) and fails with file, section, expected/actual, and
//! the repair command; `DO_HARNESS_DOCS_WRITE=1` rewrites the blocks in
//! place, and writes are refused while `CI` is set. `scripts/docs.py sync
//! --check|--write` is the command-line entry point.
//!
//! Clap 4.6.6 exposes no public getters for `requires`/`conflicts`
//! relationships, so those are the one facet this walk cannot record;
//! `cli_tests.rs` pins the two relationships the CLI uses at parse level.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod contract;
mod markers;
mod render;
mod sync;
mod tests;
