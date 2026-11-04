use crate::accessions::{INTENSITY_ARRAY, MZ_ARRAY};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteRangesOptions {
    pub arrays: Vec<u32>,
}

impl Default for ByteRangesOptions {
    fn default() -> Self {
        Self {
            arrays: vec![MZ_ARRAY, INTENSITY_ARRAY],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Range {
    pub from: f64,
    pub to: f64,
}
