//! Progress detection across attempts.
//!
//! Progress is measured from the *set* of failing and unrunnable sensor names,
//! never from diagnostic text: an executor that rewords the same failure has
//! not made progress. A signature seen before, or one that is not a proper
//! subset of the previous attempt's failure set, is a regression or a repeat,
//! and alternating failures therefore stop the job too. The total attempt and
//! time caps apply regardless, so changing text cannot buy more budget.

use std::collections::BTreeSet;

use super::receipt::AttemptRecord;

/// What an attempt's failure set means for continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressVerdict {
    /// No earlier attempt failed.
    First,
    /// The failure set is a proper subset of the previous attempt's set.
    Improved,
    /// The same signature was already observed.
    Repeated,
    /// The failure set is not a proper subset of the previous set.
    Regressed,
}

/// Tracks signatures observed so far, including those persisted before resume.
pub struct ProgressTracker {
    seen: BTreeSet<String>,
    previous: Option<BTreeSet<String>>,
}

impl ProgressTracker {
    /// Seeds the tracker from a receipt's attempt history.
    #[must_use]
    pub fn from_attempts(attempts: &[AttemptRecord]) -> Self {
        let mut seen = BTreeSet::new();
        for attempt in attempts {
            if let Some(signature) = &attempt.failure_signature {
                seen.insert(signature.clone());
            }
        }
        let previous = attempts
            .iter()
            .rev()
            .find(|attempt| !attempt.failure_set.is_empty())
            .map(|attempt| attempt.failure_set.iter().cloned().collect());
        Self { seen, previous }
    }

    /// Classifies one attempt's failure set.
    #[must_use]
    pub fn evaluate(&mut self, signature: &str, set: &BTreeSet<String>) -> ProgressVerdict {
        let previous = self.previous.replace(set.clone());
        let repeated = !self.seen.insert(signature.to_owned());
        if repeated {
            return ProgressVerdict::Repeated;
        }
        match previous {
            None => ProgressVerdict::First,
            Some(previous) if set.len() < previous.len() && set.is_subset(&previous) => {
                ProgressVerdict::Improved
            }
            Some(_) => ProgressVerdict::Regressed,
        }
    }
}

/// Builds the text-independent signature and its identifier set.
#[must_use]
pub fn signature(
    failed: &[String],
    unrunnable: &[String],
    state: Option<&str>,
    reason: Option<&str>,
) -> (String, BTreeSet<String>) {
    let mut set: BTreeSet<String> = failed
        .iter()
        .map(|name| format!("failed:{name}"))
        .chain(unrunnable.iter().map(|name| format!("unrunnable:{name}")))
        .collect();
    if set.is_empty() {
        set.insert(format!(
            "state:{}:{}",
            state.unwrap_or("unknown"),
            reason.unwrap_or("unknown")
        ));
    }
    (set.iter().cloned().collect::<Vec<_>>().join(","), set)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_strictly_smaller_failure_set_is_progress() {
        let mut tracker = ProgressTracker::from_attempts(&[]);
        assert_eq!(
            tracker.evaluate("a,b", &set(&["a", "b"])),
            ProgressVerdict::First
        );
        assert_eq!(
            tracker.evaluate("a", &set(&["a"])),
            ProgressVerdict::Improved
        );
    }

    #[test]
    fn a_repeated_or_alternating_failure_is_not_progress() {
        let mut tracker = ProgressTracker::from_attempts(&[]);
        let _ = tracker.evaluate("a", &set(&["a"]));
        assert_eq!(
            tracker.evaluate("b", &set(&["b"])),
            ProgressVerdict::Regressed
        );
        assert_eq!(
            tracker.evaluate("a", &set(&["a"])),
            ProgressVerdict::Repeated
        );
    }

    #[test]
    fn signatures_ignore_diagnostic_text() {
        let (first, _) = signature(&["fmt".to_owned()], &[], Some("red"), Some("failures"));
        let (second, _) = signature(&["fmt".to_owned()], &[], Some("red"), Some("failures"));
        assert_eq!(first, second);
    }
}
