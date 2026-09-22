use crate::ion::{IonError, IonResult};

pub(crate) trait DeltaWord: Copy + Default {
    const BYTES: usize;
    fn wrapping_sub(self, rhs: Self) -> Self;
    fn to_le_bytes_into(self, out: &mut Vec<u8>);
}

impl DeltaWord for u32 {
    const BYTES: usize = 4;
    fn wrapping_sub(self, rhs: Self) -> Self {
        self.wrapping_sub(rhs)
    }
    fn to_le_bytes_into(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
}

impl DeltaWord for u64 {
    const BYTES: usize = 8;
    fn wrapping_sub(self, rhs: Self) -> Self {
        self.wrapping_sub(rhs)
    }
    fn to_le_bytes_into(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
}

pub(crate) mod byte_shuffle;
pub(crate) mod delta_shuffle;
pub(crate) mod raw;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dtype {
    F64 = 1,
    F32 = 2,
    F16 = 3,
    I16 = 4,
    I32 = 5,
    I64 = 6,
}

impl Dtype {
    pub(crate) fn from_byte(b: u8) -> IonResult<Self> {
        match b {
            1 => Ok(Self::F64),
            2 => Ok(Self::F32),
            3 => Ok(Self::F16),
            4 => Ok(Self::I16),
            5 => Ok(Self::I32),
            6 => Ok(Self::I64),
            _ => Err(IonError::BadDtype {
                dtype: b,
                kind: "packing dtype",
            }),
        }
    }

    pub(crate) fn byte_stride(self) -> usize {
        match self {
            Self::F64 | Self::I64 => 8,
            Self::F32 | Self::I32 => 4,
            Self::F16 | Self::I16 => 2,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PackingId {
    Raw = 0,
    ByteShuffle = 1,
    DeltaShuffle = 2,
}

impl PackingId {
    pub(crate) fn from_byte(b: u8) -> IonResult<Self> {
        match b {
            0 => Ok(Self::Raw),
            1 => Ok(Self::ByteShuffle),
            2 => Ok(Self::DeltaShuffle),
            _ => Err(IonError::UnsupportedPacking(b)),
        }
    }
}

pub(crate) enum PackingInput<'a> {
    F32(&'a [f32]),
    F64(&'a [f64]),
}

pub(crate) trait Packing: Send + Sync {
    fn id(&self) -> PackingId;

    fn is_variable_length(&self) -> bool {
        false
    }

    fn supports(&self, dtype: Dtype) -> bool;

    fn encode(&self, input: PackingInput<'_>, out: &mut Vec<u8>) -> IonResult<()>;

    fn decode(&self, input: &[u8], dtype: Dtype, out: &mut Vec<u8>) -> IonResult<()>;
}

pub(crate) fn packing_for(array_type: u32, dtype: Dtype) -> &'static dyn Packing {
    use crate::accessions::{
        ION_MOBILITY_ARRAY, MEAN_ION_MOBILITY_ARRAY, MZ_ARRAY, RAW_ION_MOBILITY_ARRAY,
        RAW_ION_MOBILITY_DRIFT_TIME_ARRAY, TIME_ARRAY,
    };
    let is_delta_axis = matches!(
        array_type,
        MZ_ARRAY
            | TIME_ARRAY
            | ION_MOBILITY_ARRAY
            | MEAN_ION_MOBILITY_ARRAY
            | RAW_ION_MOBILITY_ARRAY
            | RAW_ION_MOBILITY_DRIFT_TIME_ARRAY
    );
    match dtype {
        Dtype::F64 | Dtype::F32 if is_delta_axis => &delta_shuffle::DELTA_SHUFFLE,
        _ => &raw::RAW,
    }
}

pub(crate) fn packing_by_id(id: PackingId) -> &'static dyn Packing {
    use byte_shuffle::BYTE_SHUFFLE;
    use delta_shuffle::DELTA_SHUFFLE;
    use raw::RAW;

    match id {
        PackingId::Raw => &RAW,
        PackingId::ByteShuffle => &BYTE_SHUFFLE,
        PackingId::DeltaShuffle => &DELTA_SHUFFLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accessions::MZ_ARRAY;

    #[test]
    fn packing_id_from_byte_roundtrip() {
        for b in 0u8..=2 {
            let id = PackingId::from_byte(b).unwrap();
            assert_eq!(id as u8, b);
        }
        assert!(PackingId::from_byte(3).is_err());
    }

    #[test]
    fn dtype_from_byte_roundtrip() {
        for b in 1u8..=6 {
            let d = Dtype::from_byte(b).unwrap();
            assert_eq!(d as u8, b);
        }
        assert!(Dtype::from_byte(0).is_err());
        assert!(Dtype::from_byte(7).is_err());
    }

    #[test]
    fn dtype_byte_stride_correct() {
        assert_eq!(Dtype::F64.byte_stride(), 8);
        assert_eq!(Dtype::I64.byte_stride(), 8);
        assert_eq!(Dtype::F32.byte_stride(), 4);
        assert_eq!(Dtype::I32.byte_stride(), 4);
        assert_eq!(Dtype::F16.byte_stride(), 2);
        assert_eq!(Dtype::I16.byte_stride(), 2);
    }

    #[test]
    fn packing_for_dtype_dispatch() {
        assert_eq!(
            packing_for(MZ_ARRAY, Dtype::F64).id(),
            PackingId::DeltaShuffle
        );
        assert_eq!(packing_for(0, Dtype::F32).id(), PackingId::Raw);
        assert_eq!(packing_for(0, Dtype::I32).id(), PackingId::Raw);
    }

    #[test]
    fn packing_by_id_covers_all_variants() {
        for b in 0u8..=2 {
            let id = PackingId::from_byte(b).unwrap();
            let _ = packing_by_id(id);
        }
    }
}
