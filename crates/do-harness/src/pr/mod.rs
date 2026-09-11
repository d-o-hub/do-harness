//! Deterministic PR analysis for the `pr-triage` skill.
//!
//! Reads only the repository and the `gh` CLI; the sole write is a
//! best-effort review cache under the repository git directory.

pub mod cache;
pub mod command;
pub mod diff;
pub mod gh;
pub mod no_effect;
pub mod review;

#[cfg(test)]
mod diff_tests;

#[cfg(test)]
mod tests;
