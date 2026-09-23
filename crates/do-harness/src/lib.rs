//! `do-harness` is a unified CLI for the do-harness agent execution harness:
//! computational sensors, the task workflow, skill evaluation, and database
//! maintenance.
//!
//! # Where the documentation is
//!
//! This package publishes a **binary**, not a library API. The binary's modules
//! are declared under `src/main.rs` and are deliberately not re-exported here,
//! so the stable surface is the command line:
//!
//! - the CLI reference, `docs/cli.md`, which is gated against `--help` by
//!   `tests/docs_coverage.rs`;
//! - `do-harness --help` and `do-harness <subcommand> --help`;
//! - the README at <https://github.com/d-o-hub/do-harness#readme>.
//!
//! The library target exists so documentation tooling has something to build
//! for a binary-only crate: `docs.rs` cannot document a package with no library
//! target and fails every release with `no library targets found`. It exports
//! no items on purpose — re-exporting a module would commit this crate to a
//! semver surface that no consumer has asked for, and the CLI is the contract.
