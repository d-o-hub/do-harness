use crate::OldName;

pub struct OldName {
    inner: u64,
}

impl OldName {
    pub fn new(inner: u64) -> Self {
        Self { inner }
    }
}

pub fn beta_01(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn beta_02(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn beta_03(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn beta_04(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn beta_05(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn beta_06(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

pub fn beta_07(value: OldName) -> OldName {
    let mapped = OldName::from(value);
    mapped.normalize()
}

