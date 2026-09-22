use crate::accessions::{INTENSITY_ARRAY, MZ_ARRAY, TIME_ARRAY};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArrayKind {
    Mz,
    Intensity,
    Time,
}

impl ArrayKind {
    pub const fn accession(self) -> u32 {
        match self {
            ArrayKind::Mz => MZ_ARRAY,
            ArrayKind::Intensity => INTENSITY_ARRAY,
            ArrayKind::Time => TIME_ARRAY,
        }
    }
}

impl From<ArrayKind> for u32 {
    fn from(kind: ArrayKind) -> u32 {
        kind.accession()
    }
}
