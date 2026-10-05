pub fn run_leaf_a() -> String {
    let val = shared_lib::compute_value();
    let msg = proc_macro_lib::make_greeting!();
    format!("{} - {}", msg, val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaf_a() {
        assert_eq!(run_leaf_a(), "Hello from proc macro! - 42");
    }
}
