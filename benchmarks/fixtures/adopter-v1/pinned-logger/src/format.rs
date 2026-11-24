//! Log line formatting: plain text and JSON rendering.

use std::fmt::Write as _;

/// Fixed-width UTC timestamp for a synthetic clock. Pure and deterministic.
pub fn timestamp(ts_seconds: u64) -> String {
    let days = ts_seconds / 86400;
    let rem = ts_seconds % 86400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!("{days:04}-{hh:02}:{mm:02}:{ss:02}")
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
