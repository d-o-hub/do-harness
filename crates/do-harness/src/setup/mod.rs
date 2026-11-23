//! `do-harness setup`: post-`init` configuration helpers.
//!
//! `init` scaffolds a pack; `setup` completes what a pack cannot resolve by
//! itself (peer dependencies, environment wiring). Subcommands are
//! deliberately small and focused on one adoptable surface.

pub mod web;
