use crate::NewName;

pub struct NewName {
    inner: u64,
}

impl NewName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn beta_01(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn beta_02(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn beta_03(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn beta_04(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn beta_05(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn beta_06(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn beta_07(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

