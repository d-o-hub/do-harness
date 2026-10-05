pub fn get_schema_version() -> u32 {
    let raw = include_str!("../data/schema.json");
    if raw.contains("\"version\": 1") {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version() {
        assert_eq!(get_schema_version(), 1);
    }
}
