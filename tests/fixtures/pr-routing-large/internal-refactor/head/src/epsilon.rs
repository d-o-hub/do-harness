use crate::NewName;

pub struct NewName {
    inner: u64,
}

impl NewName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn epsilon_01(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn epsilon_02(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn epsilon_03(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn epsilon_04(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn epsilon_05(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn epsilon_06(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn epsilon_07(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

