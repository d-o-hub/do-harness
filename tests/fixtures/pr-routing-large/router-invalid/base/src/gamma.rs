use crate::OldName;

pub struct OldName {
    inner: u64,
}

impl OldName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn gamma_01(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn gamma_02(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn gamma_03(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn gamma_04(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn gamma_05(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn gamma_06(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn gamma_07(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

