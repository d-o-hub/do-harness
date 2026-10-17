//! Fixture suites for the documentation generator.
//!
//! Everything here is synthetic (in-memory commands and `tempfile`
//! repositories) except `walker_cases::real_cli_contract_keeps_flag_ownership_and_key_paths`,
//! which reflects over the real CLI definition; the committed artifacts
//! themselves are checked by `sync::generated_docs_are_in_sync`.

mod marker_cases;
mod render_cases;
mod support;
mod sync_cases;
mod walker_cases;
