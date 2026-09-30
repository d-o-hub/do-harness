//! Low-level text scanning shared by the waiver detectors.
//!
//! Every span here is derived from comment- and string-stripped text and then
//! matched by brace or paren depth, so an unusual layout degrades to "not
//! classified" instead of mis-classifying: the answers feed an advisory
//! comment, never a gate.

/// A brace-matched span of 1-based lines, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// First line of the span.
    pub start: u32,
    /// Last line of the span.
    pub end: u32,
}

impl Span {
    /// Whether the span covers `line`.
    #[must_use]
    pub fn contains(self, line: u32) -> bool {
        line >= self.start && line <= self.end
    }
}

/// 1-based line number for a zero-based index, saturating at `u32::MAX`.
#[must_use]
pub fn line_number(index: usize) -> u32 {
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

/// Line count as a `u32`, saturating at `u32::MAX`.
#[must_use]
pub fn line_count(lines: &[String]) -> u32 {
    u32::try_from(lines.len()).unwrap_or(u32::MAX)
}

/// Strips a `//` comment and double-quoted string bodies.
///
/// Character literals are left alone: `'` is rare in the constructs that
/// matter here and treating it as a quote would swallow real code. Callers that
/// need the *contents* of a string (a `#[cfg(feature = "…")]` name) must read
/// the raw text instead.
#[must_use]
pub fn code_only(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if in_string {
            if ch == '\\' {
                chars.next();
            } else if ch == '"' {
                in_string = false;
                out.push('"');
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push('"');
            }
            '/' if chars.peek() == Some(&'/') => break,
            _ => out.push(ch),
        }
    }
    out
}

/// Brace-matched span of the block opened at or after `open_line`.
///
/// Returns the span of the line containing the opening brace through the line
/// containing its match. A line with no brace (e.g. a `mod x;` declaration)
/// spans itself up to the terminating `;`.
#[must_use]
pub fn block_span(lines: &[String], open_line: u32) -> Span {
    let mut depth = 0i32;
    let mut seen_open = false;
    for (index, raw) in lines.iter().enumerate().skip(open_line as usize - 1) {
        let code = code_only(raw);
        for ch in code.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    seen_open = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        let line = line_number(index);
        if seen_open && depth <= 0 {
            return Span {
                start: open_line,
                end: line,
            };
        }
        if !seen_open && code.contains(';') {
            return Span {
                start: open_line,
                end: line,
            };
        }
    }
    Span {
        start: open_line,
        end: line_count(lines),
    }
}

/// Paren-matched span from an opening `(` at `line`/`column`.
#[must_use]
pub fn paren_span(lines: &[String], line: u32, column: usize) -> Span {
    let mut depth = 0i32;
    for (index, raw) in lines.iter().enumerate().skip(line as usize - 1) {
        let code = code_only(raw);
        let start_column = if line_number(index) == line {
            column
        } else {
            0
        };
        for ch in code.chars().skip(start_column) {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 {
            return Span {
                start: line,
                end: line_number(index),
            };
        }
    }
    Span {
        start: line,
        end: line_count(lines),
    }
}
