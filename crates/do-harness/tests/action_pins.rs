//! Test suite for `scripts/check-action-pins.py` (the `action-pins` sensor).
//!
//! The cases are split by concern: `references` covers external reference
//! forms, `docker` covers image digests, `structure` covers the ways a
//! reference can be hidden, `shape` covers workflows with nothing to check,
//! and `discovery` covers which files are read. Shared fixtures live in
//! `fixtures`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

#[path = "action_pins/discovery.rs"]
mod discovery;
#[path = "action_pins/docker.rs"]
mod docker;
#[path = "action_pins/fixtures.rs"]
mod fixtures;
#[path = "action_pins/references.rs"]
mod references;
#[path = "action_pins/shape.rs"]
mod shape;
#[path = "action_pins/structure.rs"]
mod structure;
