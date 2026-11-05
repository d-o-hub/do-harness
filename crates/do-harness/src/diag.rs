//! Bounded capture of the CLI's own failure diagnostics.
//!
//! A defect report is only actionable if it carries what the CLI printed when
//! it failed. The failure path renders failing-sensor output and the final
//! error to stderr, so those lines are mirrored into a bounded ring buffer that
//! `feedback::report` attaches to the issue.
//!
//! This is a diagnostic aid, not a log: it holds the last
//! [`MAX_CAPTURED_BYTES`] and dies with the process.

use std::sync::{Mutex, OnceLock};

/// Maximum bytes retained from the CLI's own failure output.
pub const MAX_CAPTURED_BYTES: usize = 8 * 1024;

static CAPTURE: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static ARGV: OnceLock<Vec<String>> = OnceLock::new();

/// Records the process arguments once, for diagnostic reports.
///
/// The arguments are reported verbatim and are never consulted for
/// authorization, so a later call cannot change what an earlier one recorded.
pub fn set_argv(argv: Vec<String>) {
    let _ = ARGV.set(argv);
}

/// The process arguments, or an empty list when they were never recorded.
#[must_use]
pub fn argv() -> Vec<String> {
    ARGV.get().cloned().unwrap_or_default()
}

/// Appends one diagnostic line, dropping the oldest bytes past the bound.
fn push_bounded(buffer: &mut Vec<u8>, line: &str) {
    buffer.extend_from_slice(line.as_bytes());
    buffer.push(b'\n');
    if buffer.len() > MAX_CAPTURED_BYTES {
        let excess = buffer.len() - MAX_CAPTURED_BYTES;
        buffer.drain(..excess);
    }
}

/// Records one diagnostic line for a later defect report.
pub fn record(line: &str) {
    if let Ok(mut buffer) = CAPTURE.lock() {
        push_bounded(&mut buffer, line);
    }
}

/// The captured diagnostics, oldest first.
#[must_use]
pub fn tail() -> String {
    let Ok(buffer) = CAPTURE.lock() else {
        return String::new();
    };
    String::from_utf8_lossy(&buffer).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_push_keeps_the_newest_bytes() {
        let mut buffer = Vec::new();
        for index in 0..1000 {
            push_bounded(&mut buffer, &format!("line {index}"));
        }
        assert!(buffer.len() <= MAX_CAPTURED_BYTES);
        let text = String::from_utf8_lossy(&buffer);
        assert!(text.ends_with("line 999\n"));
    }
}
