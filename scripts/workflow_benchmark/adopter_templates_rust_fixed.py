"""Fixed (achievable) Rust fixture variants used as corpus patches and by
self-tests. Each keeps every regression check green.
"""

# ---------------------------------------------------------------------------
# Fixed variants: the achievable correct end-states used as corpus patches and
# by self-tests. Each must keep every regression check green.
# ---------------------------------------------------------------------------

RUST_LIB_FIXED = r"""
//! Synthetic pinned adopter fixture: a small multi-module logger.

pub mod format;
pub mod level;
pub mod sink;

/// Route one record through a sink, honoring the sink's configured format.
pub fn render(sink: &mut sink::Sink, ts_seconds: u64, level: char, msg: &str) {
    sink.record(ts_seconds, level, msg);
}
"""

RUST_SINK_FIXED = r"""
//! Buffered log sink with format-aware recording and capacity flushing.

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

    /// Render and record one log entry in the configured format. Returns the
    /// oldest lines flushed to an archive once the buffer exceeds capacity.
    pub fn record(&mut self, ts_seconds: u64, level: char, msg: &str) -> Vec<String> {
        let line = match self.format {
            Format::Text => format::text_line(ts_seconds, level, msg),
            Format::Json => format::json_line(ts_seconds, level, msg),
        };
        self.buffer.push(line);
        if self.buffer.len() > self.capacity {
            return self.buffer.split_off(self.capacity);
        }
        Vec::new()
    }

    /// Drain all buffered lines, leaving the sink empty.
    pub fn drain(&mut self) -> Vec<String> {
        std::mem::take(&mut self.buffer)
    }
}
"""

RUST_SINK_POLICY = r"""
//! Buffered log sink (policy-invalidation state: no allow-attributes).

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

    /// Render and record one log entry. The baseline renders text; the
    /// multi-file fix honors `self.format`.
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
"""

RUST_FORMAT_EDIT = r"""
//! Log line formatting: plain text and JSON rendering.

use std::fmt::Write as _;

/// Fixed-width UTC timestamp for a synthetic clock. Pure and deterministic.
pub fn timestamp(ts_seconds: u64) -> String {
    let elapsed_days = ts_seconds / 86400;
    let rem = ts_seconds % 86400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!("{elapsed_days:04}-{hh:02}:{mm:02}:{ss:02}")
}

/// One log line in plain text.
pub fn text_line(ts_seconds: u64, level: char, msg: &str) -> String {
    format!("[{}] {level} {msg}", timestamp(ts_seconds))
}

/// One log line as JSON. Escapes the characters that can appear in the
/// synthetic messages used by this repository's tests.
pub fn json_line(ts_seconds: u64, level: char, msg: &str) -> String {
    format!(
        r#"{{"time":"{}","level":"{}","message":{}}}"#,
        timestamp(ts_seconds),
        level,
        json_string(msg)
    )
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}
"""
