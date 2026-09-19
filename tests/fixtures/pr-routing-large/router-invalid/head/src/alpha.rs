use crate::NewName;

pub struct NewName {
    inner: u64,
}

impl NewName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn alpha_01(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn alpha_02(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn alpha_03(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn alpha_04(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn alpha_05(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn alpha_06(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn alpha_07(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

