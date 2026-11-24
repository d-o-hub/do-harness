//! Buffered log sink.
//!
//! The baseline buffers every emitted line and can drain them. It already
//! exposes `record`, but it always renders text (the multi-file bugfix gap)
//! and never flushes overflow (the resume-case gap), so target behaviors fail
//! at runtime while every regression check compiles and passes.

use crate::format;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

pub struct Sink {
    buffer: Vec<String>,
    format: Format,
    capacity: usize,
}

const DEFAULT_CAPACITY: usize = 256;

impl Sink {
    /// A fresh sink configured to render in `format`.
    pub fn new(format: Format) -> Self {
        Self {
            buffer: Vec::with_capacity(DEFAULT_CAPACITY),
            format,
            capacity: DEFAULT_CAPACITY,
        }
    }

    /// The format this sink is configured to render.
    pub fn format(&self) -> Format {
        self.format
    }

    /// Append a pre-rendered line.
    pub fn emit(&mut self, line: String) {
        self.buffer.push(line);
    }

    /// Render and record one log entry.
    ///
    /// Returns lines flushed to an archive when the buffer overflows its
    /// capacity. The baseline renders text regardless of the configured
    /// format and never flushes: both gaps are targets for corpus cases.
    pub fn record(&mut self, ts_seconds: u64, level: char, msg: &str) -> Vec<String> {
        let _ = self.capacity; // reserved for the capacity-flush feature
        self.buffer.push(format::text_line(ts_seconds, level, msg));
        Vec::new()
    }

    /// Drain all buffered lines, leaving the sink empty.
    pub fn drain(&mut self) -> Vec<String> {
        std::mem::take(&mut self.buffer)
    }
}

/// Kept for parity with `format::text_line`; banned by the policy-invalidation
/// case and removed as part of that fix.
#[allow(dead_code)]
pub fn legacy_fallback(level: char, msg: &str) -> String {
    format::text_line(0, level, msg)
}
