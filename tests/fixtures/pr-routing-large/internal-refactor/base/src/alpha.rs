use crate::OldName;

pub struct OldName {
    inner: u64,
}

impl OldName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn alpha_01(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn alpha_02(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn alpha_03(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn alpha_04(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn alpha_05(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn alpha_06(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn alpha_07(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

