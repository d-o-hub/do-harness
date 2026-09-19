pub fn new_of(value: NewName) -> NewName {
    NewName::from(value)
}

pub fn new_len(value: NewName) -> usize {
    NewName::from(value).len()
}

pub fn new_is_empty(value: NewName) -> bool {
    NewName::from(value).len() == 0
}
