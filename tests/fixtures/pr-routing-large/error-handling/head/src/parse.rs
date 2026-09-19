use std::fmt;

#[derive(Debug)]
pub enum parseError {
    Variant01,
    Variant02,
    Variant03,
    Variant04,
    Variant05,
    Variant06,
}

impl fmt::Display for parseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Variant01 => write!(f, "parse error variant 01"),
            Self::Variant02 => write!(f, "parse error variant 02"),
            Self::Variant03 => write!(f, "parse error variant 03"),
            Self::Variant04 => write!(f, "parse error variant 04"),
            Self::Variant05 => write!(f, "parse error variant 05"),
            Self::Variant06 => write!(f, "parse error variant 06"),
        }
    }
}
