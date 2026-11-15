//! Small shared helpers with no better home.

/// Percentiles are expressed on a 0-100 scale, so the nearest-rank divisor is
/// the whole scale.
const PERCENTILE_SCALE: u64 = 100;

/// Quotes `value` as one POSIX shell single-quoted fragment.
///
/// The drafted commands are meant to be pasted and run, and the sensor name
/// comes from event records and config, so an embedded quote must not close
/// the fragment.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Strips the Windows verbatim prefix a canonical path may carry.
///
/// `Path::canonicalize` returns `\\?\C:\...` on Windows (and
/// `\\?\UNC\server\share` for network paths). Verbatim paths disable the
/// normalisation every other tool performs, so a root captured with the prefix
/// stops matching the paths the rest of the harness builds from it with `join`.
/// Returns `None` when the text carries no prefix this function can remove.
///
/// Only the Windows path of [`without_verbatim_prefix`] calls it outside tests,
/// so other targets would otherwise report it as dead code.
#[cfg_attr(not(windows), allow(dead_code))]
fn strip_verbatim_prefix(text: &str) -> Option<String> {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return Some(format!(r"\\{rest}"));
    }
    let rest = text.strip_prefix(r"\\?\")?;
    let bytes = rest.as_bytes();
    (bytes.len() >= 2 && bytes[1] == b':').then(|| rest.to_owned())
}

/// Canonical root without the Windows verbatim prefix.
///
/// Other platforms return the path unchanged.
#[must_use]
pub(crate) fn without_verbatim_prefix(path: std::path::PathBuf) -> std::path::PathBuf {
    #[cfg(windows)]
    if let Some(stripped) = strip_verbatim_prefix(&path.to_string_lossy()) {
        return std::path::PathBuf::from(stripped);
    }
    path
}

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

    use super::{percentile, strip_verbatim_prefix};

    /// Windows canonicalization prefixes paths; a root captured with it stops
    /// matching the paths the harness builds from it with `join`.
    #[test]
    fn verbatim_prefixes_are_removed() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\work\repo").as_deref(),
            Some(r"C:\work\repo")
        );
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share").as_deref(),
            Some(r"\\server\share")
        );
        assert_eq!(strip_verbatim_prefix(r"C:\work\repo"), None);
        assert_eq!(strip_verbatim_prefix("/home/work/repo"), None);
        assert_eq!(strip_verbatim_prefix(r"\\?\relative"), None);
    }

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
