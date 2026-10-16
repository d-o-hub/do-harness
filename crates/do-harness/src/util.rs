//! Small shared helpers with no better home.

/// Percentiles are expressed on a 0-100 scale, so the nearest-rank divisor is
/// the whole scale.
const PERCENTILE_SCALE: u64 = 100;

/// Nearest-rank percentile over an ascending slice.
///
/// The rank is `ceil(pct * n / 100)` clamped to at least 1 (the nearest-rank
/// definition), and the index is clamped to the slice, so `pct = 0` returns
/// the minimum rather than an out-of-range index. The DORA collector and the
/// PR-loop metrics previously carried a private copy of this algorithm.
pub(crate) fn percentile<T: Ord + Copy>(sorted: &[T], pct: u32) -> Option<T> {
    if sorted.is_empty() {
        return None;
    }
    let n = u64::try_from(sorted.len()).ok()?;
    let rank = (u64::from(pct) * n).div_ceil(PERCENTILE_SCALE).max(1);
    let index = usize::try_from(rank - 1).ok()?.min(sorted.len() - 1);
    sorted.get(index).copied()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::percentile;

    #[test]
    fn nearest_rank_covers_both_former_implementations() {
        let values = [1i64, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        // ceil(0.5 * 10) = 5 -> the fifth smallest value.
        assert_eq!(percentile(&values, 50), Some(5));
        assert_eq!(percentile(&values, 90), Some(9));
        assert_eq!(percentile(&values, 100), Some(10));
        // pct 0 clamps rank to 1 (the minimum), never a zero index.
        assert_eq!(percentile(&values, 0), Some(1));
        assert_eq!(percentile::<u64>(&[], 50), None);
    }
}
