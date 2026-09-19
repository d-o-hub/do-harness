use crate::OldName;

pub struct OldName {
    inner: u64,
}

impl OldName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn epsilon_01(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn epsilon_02(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn epsilon_03(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn epsilon_04(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn epsilon_05(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn epsilon_06(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn epsilon_07(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

