//! Test suite for `scripts/check-action-pins.py` (the `action-pins` sensor).
//!
//! The cases are split by concern: `references` covers what a reference may
//! say, `structure` covers the ways a reference can be hidden and the
//! discovery boundaries that decide which files are read at all. The shared
//! fixtures live in `fixtures`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

#[path = "action_pins/fixtures.rs"]
mod fixtures;
#[path = "action_pins/references.rs"]
mod references;
#[path = "action_pins/structure.rs"]
mod structure;
