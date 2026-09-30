//! Text-level source scanning for the patch-coverage waiver classifier.
//!
//! The classifier must answer structural questions about a single line: is it
//! inside a logging macro's event fields, the fallback of a `let … else`, an
//! arm that a previous guarded arm already proved unreachable, or a
//! `#[cfg(feature = "…")]` item? A full parse is not warranted — the answers
//! feed an advisory comment, and every span here is brace- or paren-matched
//! over comment- and string-stripped text, so an unusual layout degrades to
//! "not classified" instead of mis-classifying.

use std::collections::BTreeSet;

pub use super::waiver_text::{Span, block_span, code_only, line_number, paren_span};

/// A `#[cfg(feature = "…")]` item span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureRegion {
    /// Feature names the attribute requires.
    pub features: Vec<String>,
    /// Lines the attribute covers, attribute line included.
    pub span: Span,
}

/// A macro invocation span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroSpan {
    /// Macro path as written (e.g. `tracing::info`).
    pub name: String,
    /// Lines of the invocation, from the macro name to the closing paren.
    pub span: Span,
}

/// One match arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    /// Arm head up to (and including) `=>`, whitespace-normalized.
    pub head: String,
    /// Lines the arm occupies, head included.
    pub span: Span,
}

/// Feature-gated item spans in `lines`.
#[must_use]
pub fn cfg_feature_regions(lines: &[String]) -> Vec<FeatureRegion> {
    let mut regions = Vec::new();
    for (index, raw) in lines.iter().enumerate() {
        // The attribute is read verbatim: `code_only` blanks string bodies,
        // which is exactly where the feature name lives.
        let attribute_text = raw.trim_start();
        if !attribute_text.starts_with("#[cfg") || !attribute_text.contains("feature") {
            continue;
        }
        let features = quoted_features(attribute_text);
        if features.is_empty() {
            continue;
        }
        let attribute = line_number(index);
        let mut item = attribute;
        for (offset, next) in lines.iter().enumerate().skip(index + 1) {
            let code = code_only(next);
            let text = code.trim();
            item = line_number(offset);
            if text.is_empty() || text.starts_with("//") || text.starts_with("#[") {
                continue;
            }
            break;
        }
        let span = block_span(lines, item);
        regions.push(FeatureRegion {
            features,
            span: Span {
                start: attribute,
                end: span.end,
            },
        });
    }
    regions
}

/// Feature names in a `#[cfg(…)]` attribute, in written order.
fn quoted_features(attribute: &str) -> Vec<String> {
    let mut features = Vec::new();
    let mut rest = attribute;
    while let Some(position) = rest.find("feature") {
        rest = &rest[position + "feature".len()..];
        let after_equals = rest.trim_start().strip_prefix('=').map(str::trim_start);
        let Some(after_equals) = after_equals else {
            continue;
        };
        let Some(after_quote) = after_equals.strip_prefix('"') else {
            continue;
        };
        if let Some(end) = after_quote.find('"') {
            features.push(after_quote[..end].to_owned());
            rest = &after_quote[end..];
        }
    }
    features
}

/// Macro-invocation spans whose macro path ends in one of `macros`.
#[must_use]
pub fn macro_spans(lines: &[String], macros: &[&str]) -> Vec<MacroSpan> {
    let mut spans = Vec::new();
    for (index, raw) in lines.iter().enumerate() {
        let code = code_only(raw);
        let bytes = code.as_bytes();
        let mut position = 0;
        while let Some(offset) = code[position..].find('!') {
            let bang = position + offset;
            position = bang + 1;
            let mut cursor = bang + 1;
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            if bytes.get(cursor) != Some(&b'(') {
                continue;
            }
            let mut start = bang;
            while start > 0
                && (bytes[start - 1].is_ascii_alphanumeric()
                    || bytes[start - 1] == b'_'
                    || bytes[start - 1] == b':')
            {
                start -= 1;
            }
            let name = code[start..bang].to_owned();
            if !macros.contains(&name.rsplit("::").next().unwrap_or("")) {
                continue;
            }
            spans.push(MacroSpan {
                name,
                span: paren_span(lines, line_number(index), cursor),
            });
        }
    }
    spans
}

/// `let … else { … }` fallback spans, with the bound expression as evidence.
#[must_use]
pub fn let_else_spans(lines: &[String]) -> Vec<(Span, String)> {
    let mut spans = Vec::new();
    for (index, raw) in lines.iter().enumerate() {
        let code = code_only(raw);
        let Some(else_at) = code.find("else") else {
            continue;
        };
        let after = code[else_at + "else".len()..].trim_start();
        if !after.starts_with('{') {
            continue;
        }
        let statement = statement_start(lines, line_number(index), else_at);
        let Some(text) = statement.strip_prefix("let ") else {
            continue;
        };
        let binding = text.trim().trim_end_matches('{').trim().to_owned();
        spans.push((block_span(lines, line_number(index)), binding));
    }
    spans
}

/// Text of the statement containing `line` up to `column`, joined across lines.
fn statement_start(lines: &[String], line: u32, column: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut index = line as usize - 1;
    let mut limit = column;
    while index > 0 {
        let code = code_only(&lines[index]);
        parts.push(code[..limit.min(code.len())].to_owned());
        let previous = lines[index - 1].trim();
        if previous.ends_with(';') || previous.ends_with('{') || previous.ends_with('}') {
            break;
        }
        index -= 1;
        limit = usize::MAX;
        if parts.len() > 8 {
            break;
        }
    }
    parts.reverse();
    parts.join(" ").trim().to_owned()
}

/// Match arms of every `match` block in `lines`, in source order.
#[must_use]
pub fn match_arms(lines: &[String]) -> Vec<Arm> {
    let mut arms = Vec::new();
    for (index, raw) in lines.iter().enumerate() {
        let code = code_only(raw);
        let trimmed = code.trim_start();
        if !(trimmed.starts_with("match ") || trimmed.contains(" match ")) || !trimmed.contains('{')
        {
            continue;
        }
        let block = block_span(lines, line_number(index));
        arms.extend(arms_in_block(lines, block));
    }
    arms.sort_by_key(|arm| arm.span.start);
    arms
}

/// Splits a match block body into arms.
///
/// An arm head ends at a top-level `=>`; the arm that follows starts after the
/// previous arm's terminator (a top-level `,`, or the `}` closing a
/// block-bodied arm). Heads keep their `if …` guard so the caller can compare a
/// guarded arm with the arm that follows it.
fn arms_in_block(lines: &[String], block: Span) -> Vec<Arm> {
    let mut segments: Vec<(u32, String)> = Vec::new();
    for line in block.start..=block.end {
        let Some(raw) = lines.get(line as usize - 1) else {
            continue;
        };
        let code = code_only(raw);
        let code = if line == block.start {
            // The block opener is not part of the first arm's pattern.
            code.split_once('{')
                .map_or(String::new(), |(_, rest)| rest.to_owned())
        } else {
            code
        };
        segments.push((line, code));
    }

    let mut arms: Vec<Arm> = Vec::new();
    let mut depth = 0i32;
    let mut head = String::new();
    let mut head_start: Option<u32> = None;
    let mut in_body = false;
    for (line, code) in &segments {
        if code.trim().is_empty() {
            continue;
        }
        for ch in code.chars() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' => depth -= 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 && in_body {
                        in_body = false;
                        head.clear();
                        head_start = None;
                        // The terminator belongs to the finished arm, not to
                        // the head of the next one.
                        continue;
                    }
                }
                ',' if depth == 0 && in_body => {
                    in_body = false;
                    head.clear();
                    head_start = None;
                    continue;
                }
                _ => {}
            }
            if in_body {
                continue;
            }
            if head.is_empty() {
                head_start = Some(*line);
            }
            head.push(ch);
            if head.trim_end().ends_with("=>") {
                let normalized = head.split_whitespace().collect::<Vec<_>>().join(" ");
                arms.push(Arm {
                    head: normalized,
                    span: Span {
                        start: head_start.unwrap_or(*line),
                        end: *line,
                    },
                });
                in_body = true;
                head.clear();
                head_start = None;
            }
        }
        if !in_body && !head.trim().is_empty() {
            head.push(' ');
        }
    }

    // An arm owns its body: extend each arm to the line before the next arm,
    // so a line inside an arm body resolves to that arm rather than to the
    // enclosing one.
    for index in 0..arms.len() {
        let end = if index + 1 < arms.len() {
            arms[index + 1].span.start.saturating_sub(1)
        } else {
            block.end
        };
        arms[index].span.end = end;
    }
    arms
}

/// Unreachable-style comment above (or on) `line`.
#[must_use]
pub fn unreachable_comment(lines: &[String], line: u32) -> Option<String> {
    const KEYWORDS: [&str; 7] = [
        "unreachable",
        "cannot happen",
        "can't happen",
        "never happens",
        "guaranteed",
        "defensive",
        "fail-safe",
    ];
    let own = lines.get(line as usize - 1).map(|raw| comment_text(raw));
    if let Some(text) = own.filter(|text| matches_keyword(text, &KEYWORDS)) {
        return Some(text);
    }
    for offset in 1..=4u32 {
        if line <= offset {
            break;
        }
        let Some(text) = lines
            .get((line - offset) as usize - 1)
            .map(|raw| comment_text(raw))
            .filter(|text| !text.is_empty())
        else {
            continue;
        };
        if matches_keyword(&text, &KEYWORDS) {
            return Some(text);
        }
        // A non-comment, non-empty line ends the window above the line.
        if !lines[(line - offset) as usize - 1]
            .trim_start()
            .starts_with("//")
        {
            break;
        }
    }
    None
}

/// The `//` comment on a line, when present.
fn comment_text(line: &str) -> String {
    line.split_once("//")
        .map_or_else(String::new, |(_, text)| text.trim().to_owned())
}

/// Whether a comment mentions one of the keywords.
fn matches_keyword(text: &str, keywords: &[&str]) -> bool {
    let lowered = text.to_lowercase();
    keywords.iter().any(|keyword| lowered.contains(keyword))
}

/// Feature names covering `line`, when a cfg region does.
#[must_use]
pub fn features_at(regions: &[FeatureRegion], line: u32) -> Option<Vec<String>> {
    regions
        .iter()
        .filter(|region| region.span.contains(line))
        .map(|region| region.features.clone())
        .next()
}

/// Deduplicated feature names, in order.
#[must_use]
pub fn join_features(features: &[String]) -> String {
    let seen: BTreeSet<&String> = features.iter().collect();
    seen.into_iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}
