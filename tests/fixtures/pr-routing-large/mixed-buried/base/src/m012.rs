pub fn old_of(value: OldName) -> OldName {
    OldName::from(value)
}

pub fn old_len(value: OldName) -> usize {
    OldName::from(value).len()
}

pub fn old_is_empty(value: OldName) -> bool {
    OldName::from(value).len() == 0
}
