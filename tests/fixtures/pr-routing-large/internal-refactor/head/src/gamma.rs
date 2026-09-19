use crate::NewName;

pub struct NewName {
    inner: u64,
}

impl NewName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn gamma_01(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn gamma_02(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn gamma_03(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn gamma_04(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn gamma_05(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn gamma_06(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

pub fn gamma_07(value: NewName) -> NewName {
    let mapped = NewName::from(value);
    mapped.normalize()
}

