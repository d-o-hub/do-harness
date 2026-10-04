pub fn run_leaf_b() -> u32 {
    shared_lib::compute_value() + 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaf_b() {
        assert_eq!(run_leaf_b(), 52);
    }
}
