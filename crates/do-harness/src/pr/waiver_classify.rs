//! Line classification for the patch-coverage waiver report.
//!
//! Every rule here answers the same question for one uncovered changed line:
//! which waiver class justifies it, or none? The rules are ordered from the
//! most specific structural evidence (a `#[cfg(feature = "…")]` item the
//! measured pipeline never compiled) to the weakest (a nearby comment), and a
//! line that no rule can justify stays `missing` — a classifier that guesses
//! turns a coverage gap into a silent waiver.

use super::waiver_scan;
use super::waivers::{LineVerdict, WaiverClass};

/// Logging macros whose field expressions run only when a subscriber consumes
/// the event, so an installed-no-subscriber suite never executes them.
pub const LOG_MACROS: [&str; 10] = [
    "info", "warn", "error", "debug", "trace", "log", "println", "eprintln", "print", "panic",
];

/// Classifies one uncovered changed line.
#[must_use]
pub fn classify_line(
    source: &[String],
    line: u32,
    regions: &[waiver_scan::FeatureRegion],
    let_elses: &[(waiver_scan::Span, String)],
    macros: &[waiver_scan::MacroSpan],
    arms: &[waiver_scan::Arm],
) -> LineVerdict {
    let verdict = |class, evidence: String| LineVerdict {
        line,
        class,
        evidence,
    };

    if let Some(features) = waiver_scan::features_at(regions, line) {
        return verdict(
            WaiverClass::FeatureGated,
            format!(
                "inside `#[cfg(feature = \"{}\")]`",
                waiver_scan::join_features(&features)
            ),
        );
    }

    if let Some((_, binding)) = let_elses.iter().find(|(span, _)| span.contains(line)) {
        return verdict(
            WaiverClass::GuardedArm,
            format!("`let … else` fallback after the checks on `{binding}`"),
        );
    }

    if let Some(arm) = enclosing_arm(arms, line) {
        if let Some(previous) = previous_arm(arms, arm) {
            let (pattern, _) = split_guard(&previous.head);
            let (current, _) = split_guard(&arm.head);
            let guard = previous
                .head
                .split_once(" if ")
                .map(|(_, guard)| guard.trim_end_matches("=>").trim().to_owned());
            if let Some(guard) = guard.filter(|guard| is_guard(guard)) {
                if pattern.trim() == current.trim() {
                    return verdict(
                        WaiverClass::GuardedArm,
                        format!("arm after the guarded arm `{pattern} if {guard}`"),
                    );
                }
            }
        }
    }

    if let Some(comment) = waiver_scan::unreachable_comment(source, line) {
        return verdict(WaiverClass::GuardedArm, format!("comment: {comment}"));
    }

    if let Some(span) = macros
        .iter()
        .find(|span| span.span.contains(line) && line > span.span.start && line < span.span.end)
    {
        let text = waiver_scan::code_only(source.get(line as usize - 1).map_or("", String::as_str));
        if is_field_expression(&text) {
            return verdict(
                WaiverClass::MacroField,
                format!(
                    "`{}!` event field: evaluated only when a subscriber consumes the event",
                    span.name
                ),
            );
        }
    }

    verdict(WaiverClass::Missing, "no waiver class applies".to_owned())
}

/// Innermost arm containing `line`.
fn enclosing_arm(arms: &[waiver_scan::Arm], line: u32) -> Option<&waiver_scan::Arm> {
    arms.iter().rev().find(|arm| arm.span.contains(line))
}

/// Arm immediately before `arm` in source order.
fn previous_arm<'a>(
    arms: &'a [waiver_scan::Arm],
    arm: &waiver_scan::Arm,
) -> Option<&'a waiver_scan::Arm> {
    arms.iter()
        .rev()
        .find(|candidate| candidate.span.end < arm.span.start)
}

/// Splits an arm head into its pattern (without the `=>`) and the rest after `if`.
fn split_guard(head: &str) -> (String, String) {
    let (pattern, rest) = match head.split_once(" if ") {
        Some((pattern, rest)) => (pattern.trim(), rest.trim()),
        None => (head.trim(), ""),
    };
    (
        pattern.trim_end_matches("=>").trim().to_owned(),
        rest.to_owned(),
    )
}

/// Whether a guard expression narrows the pattern it follows.
fn is_guard(guard: &str) -> bool {
    ["==", "!=", "contains(", "is_empty()", "<", ">"]
        .iter()
        .any(|needle| guard.contains(needle))
}

/// Whether a logging-macro argument line evaluates an expression.
fn is_field_expression(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('"') {
        return false;
    }
    trimmed.contains('(') || trimmed.starts_with('%')
}
