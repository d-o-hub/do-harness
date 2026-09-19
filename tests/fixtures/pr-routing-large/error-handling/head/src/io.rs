use std::fmt;

#[derive(Debug)]
pub enum ioError {
    Variant01,
    Variant02,
    Variant03,
    Variant04,
    Variant05,
}

impl fmt::Display for ioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Variant01 => write!(f, "io error variant 01"),
            Self::Variant02 => write!(f, "io error variant 02"),
            Self::Variant03 => write!(f, "io error variant 03"),
            Self::Variant04 => write!(f, "io error variant 04"),
            Self::Variant05 => write!(f, "io error variant 05"),
        }
    }
}
