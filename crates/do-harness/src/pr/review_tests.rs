//! Unit tests for the `review` module's report assembly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn measure_reduces_and_reports_no_go() {
    let reduced = measure(100, 50);
    assert_eq!(reduced.verdict, Reduction::Reduced);
    assert!((reduced.ratio - 0.5).abs() < 1e-9);
    let no_go = measure(100, 100);
    assert_eq!(no_go.verdict, Reduction::NoGo);
    let empty = measure(0, 0);
    assert_eq!(empty.verdict, Reduction::NoGo);
    assert!(empty.ratio.abs() < 1e-9);
}
