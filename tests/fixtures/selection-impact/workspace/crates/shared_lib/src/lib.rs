pub fn compute_value() -> u32 {
    42
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_value() {
        assert_eq!(compute_value(), 42);
    }
}
