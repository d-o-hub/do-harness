pub const DDL: &str = "CREATE TABLE t (id INTEGER PRIMARY KEY)";

pub fn old_of(value: OldName) -> OldName {
    OldName::from(value)
}

pub fn old_len(value: OldName) -> usize {
    OldName::from(value).len()
}
