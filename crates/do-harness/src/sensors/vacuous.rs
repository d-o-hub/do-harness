//! Detection of a sensor run that executed zero checks.
//!
//! AGENTS.md section 4: "a green sensor that executed zero checks is a failed
//! gate, not evidence". The Rust pack enforces this by running `cargo nextest`
//! without `--no-tests=pass`; a delegated `test` sensor has no such flag, so
//! the runner reads the runner's own total summary instead.

/// Whether `output` carries a test runner's *total* summary reporting zero
/// tests.
///
/// Only totals are matched. A per-target line such as `cargo test`'s
/// `running 0 tests` is deliberately not evidence of an empty suite: a
/// workspace with one empty target and one populated target prints it while
/// running real tests. Node's built-in runner prints a single `tests <n>`
/// total, so `tests 0` there means the suite is genuinely empty — and
/// `node --test` exits 0 in that case, which is the vacuous gate this catches.
pub(crate) fn zero_test_total(output: &str) -> bool {
    output.lines().any(|line| {
        // Node prefixes the summary with a non-ASCII info marker (`ℹ tests 0`).
        let summary = line.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
        summary == "tests 0" || summary.starts_with("tests 0 ")
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn node_zero_test_total_is_detected() {
        let output = "\n> demo@1.0.0 test\n> node --test\n\n\u{2139} tests 0\n\u{2139} suites 0\n\u{2139} pass 0\n";
        assert!(zero_test_total(output));
    }

    #[test]
    fn a_populated_suite_is_not_vacuous() {
        let output = "\u{2139} tests 3\n\u{2139} pass 3\n\u{2139} fail 0\n";
        assert!(!zero_test_total(output));
    }

    /// `cargo test` prints `running 0 tests` per target, so a workspace with an
    /// empty target beside a populated one must not be read as vacuous.
    #[test]
    fn per_target_zero_lines_are_not_totals() {
        let output = "running 0 tests\n\ntest result: ok. 0 passed\n\nrunning 4 tests\ntest result: ok. 4 passed\n";
        assert!(!zero_test_total(output));
    }

    /// A count that merely starts with `0` is not zero.
    #[test]
    fn a_count_starting_with_zero_is_not_zero() {
        assert!(!zero_test_total("\u{2139} tests 01\n"));
        assert!(!zero_test_total("\u{2139} tests 10\n"));
    }
}
