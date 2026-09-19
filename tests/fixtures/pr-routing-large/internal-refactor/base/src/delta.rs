use crate::OldName;

pub struct OldName {
    inner: u64,
}

impl OldName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn delta_01(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn delta_02(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn delta_03(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn delta_04(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn delta_05(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn delta_06(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn delta_07(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

