pub const DDL: &str = "CREATE TABLE t (id INTEGER PRIMARY KEY, slug TEXT NOT NULL)";

pub fn new_of(value: NewName) -> NewName {
    NewName::from(value)
}

pub fn new_len(value: NewName) -> usize {
    NewName::from(value).len()
}
