//! Severity filtering.

fn rank(level: char) -> u8 {
    match level {
        'D' => 0,
        'I' => 1,
        'W' => 2,
        'E' => 3,
        _ => 100,
    }
}

/// True when `level` meets or exceeds `threshold`.
pub fn is_allowed(threshold: char, level: char) -> bool {
    rank(level) >= rank(threshold)
}
