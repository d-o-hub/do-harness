use crate::NewName;

pub struct NewName {
    inner: u64,
}

impl NewName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn delta_01(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn delta_02(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn delta_03(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn delta_04(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn delta_05(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn delta_06(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn delta_07(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

