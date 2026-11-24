//! Synthetic pinned adopter fixture: a small multi-module logger.
//!
//! The crate intentionally keeps a few feature gaps (JSON routing, capacity
//! flushing, a banned legacy helper) so corpus cases have real work to do
//! while every previously passing regression check stays green.

pub mod format;
pub mod level;
pub mod sink;

/// Route one record through a sink. The baseline delegates to `Sink::record`,
/// which always renders text; a complete multi-file fix makes `Sink::record`
/// honor its configured format and keeps this delegation.
pub fn render(sink: &mut sink::Sink, ts_seconds: u64, level: char, msg: &str) {
    sink.record(ts_seconds, level, msg);
}
