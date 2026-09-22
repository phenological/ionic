use super::{DeltaWord, Dtype, IonResult, Packing, PackingId, PackingInput};
use crate::ion::IonError;

pub(crate) static DELTA_SHUFFLE: DeltaShuffle = DeltaShuffle;
pub(crate) struct DeltaShuffle;

fn encode_delta<W: DeltaWord>(bits: impl Iterator<Item = W>, out: &mut Vec<u8>) {
    let mut prev = W::default();
    for w in bits {
        DeltaWord::wrapping_sub(w, prev).to_le_bytes_into(out);
        prev = w;
    }
}

fn decode_delta_u64(input: &[u8], mut emit: impl FnMut(u64)) {
    let mut prev = 0u64;
    for chunk in input.as_chunks::<8>().0 {
        prev = prev.wrapping_add(u64::from_le_bytes(*chunk));
        emit(prev);
    }
}

fn decode_delta_u32(input: &[u8], mut emit: impl FnMut(u32)) {
    let mut prev = 0u32;
    for chunk in input.as_chunks::<4>().0 {
        prev = prev.wrapping_add(u32::from_le_bytes(*chunk));
        emit(prev);
    }
}

fn require_aligned_input<W: DeltaWord>(input: &[u8]) -> IonResult<()> {
    if input.len().is_multiple_of(W::BYTES) {
        Ok(())
    } else {
        Err(IonError::from(
            "delta filter: input length is not a multiple of the word size",
        ))
    }
}

impl Packing for DeltaShuffle {
    fn id(&self) -> PackingId {
        PackingId::DeltaShuffle
    }

    fn supports(&self, dtype: Dtype) -> bool {
        matches!(dtype, Dtype::F64 | Dtype::F32)
    }

    fn encode(&self, input: PackingInput<'_>, out: &mut Vec<u8>) -> IonResult<()> {
        match input {
            PackingInput::F64(v) => {
                out.reserve(v.len() * 8);
                encode_delta::<u64>(v.iter().map(|x| x.to_bits()), out);
                Ok(())
            }
            PackingInput::F32(v) => {
                out.reserve(v.len() * 4);
                encode_delta::<u32>(v.iter().map(|x| x.to_bits()), out);
                Ok(())
            }
        }
    }

    fn decode(&self, input: &[u8], dtype: Dtype, out: &mut Vec<u8>) -> IonResult<()> {
        match dtype {
            Dtype::F64 => {
                require_aligned_input::<u64>(input)?;
                out.reserve(input.len());
                decode_delta_u64(input, |w| {
                    out.extend_from_slice(&f64::from_bits(w).to_le_bytes())
                });
                Ok(())
            }
            Dtype::F32 => {
                require_aligned_input::<u32>(input)?;
                out.reserve(input.len());
                decode_delta_u32(input, |w| {
                    out.extend_from_slice(&f32::from_bits(w).to_le_bytes())
                });
                Ok(())
            }
            _ => Err(IonError::BadDtype {
                dtype: dtype as u8,
                kind: "delta filter needs f32 or f64",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_f64(input: &[f64]) -> Vec<f64> {
        let mut enc = Vec::new();
        DELTA_SHUFFLE
            .encode(PackingInput::F64(input), &mut enc)
            .unwrap();
        let mut dec = Vec::new();
        DELTA_SHUFFLE.decode(&enc, Dtype::F64, &mut dec).unwrap();
        dec.as_chunks::<8>()
            .0
            .iter()
            .map(|c| f64::from_le_bytes(*c))
            .collect()
    }

    fn roundtrip_f32(input: &[f32]) -> Vec<f32> {
        let mut enc = Vec::new();
        DELTA_SHUFFLE
            .encode(PackingInput::F32(input), &mut enc)
            .unwrap();
        assert_eq!(enc.len(), input.len() * 4);
        let mut dec = Vec::new();
        DELTA_SHUFFLE.decode(&enc, Dtype::F32, &mut dec).unwrap();
        dec.as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect()
    }

    #[test]
    fn monotonic_mz_f32_is_bit_exact() {
        let input: Vec<f32> = (0..10_000).map(|i| 100.0 + i as f32 * 0.01).collect();
        let got = roundtrip_f32(&input);
        assert_eq!(got.len(), input.len());
        for (a, b) in got.iter().zip(input.iter()) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn special_values_f32_are_bit_exact() {
        let input = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0f32, 0.0f32];
        let got = roundtrip_f32(&input);
        for (a, b) in got.iter().zip(input.iter()) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn empty_produces_no_output() {
        assert!(roundtrip_f64(&[]).is_empty());
    }

    #[test]
    fn single_element_f64_is_bit_exact() {
        let input = [503.42f64];
        let got = roundtrip_f64(&input);
        assert_eq!(got[0].to_bits(), input[0].to_bits());
    }

    #[test]
    fn two_elements_f64_are_bit_exact() {
        let input = [100.0f64, 200.0];
        let got = roundtrip_f64(&input);
        for (a, b) in got.iter().zip(input.iter()) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn special_values_f64_are_bit_exact() {
        let input = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0f64, 0.0f64];
        let got = roundtrip_f64(&input);
        for (a, b) in got.iter().zip(input.iter()) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn monotonic_mz_f64_is_bit_exact() {
        let input: Vec<f64> = (0..10_000).map(|i| 100.0 + i as f64 * 0.01).collect();
        let got = roundtrip_f64(&input);
        for (a, b) in got.iter().zip(input.iter()) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn encoded_length_f64_equals_input_bytes() {
        let input: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let mut enc = Vec::new();
        DELTA_SHUFFLE
            .encode(PackingInput::F64(&input), &mut enc)
            .unwrap();
        assert_eq!(enc.len(), input.len() * 8);
    }

    #[test]
    fn decode_rejects_misaligned_f64_input() {
        let seven_bytes = [0u8; 7];
        let mut dec = Vec::new();
        let err = DELTA_SHUFFLE
            .decode(&seven_bytes, Dtype::F64, &mut dec)
            .expect_err("7 bytes is not a multiple of 8");
        assert!(err.contains("not a multiple of the word size"));

        let nine_bytes = [0u8; 9];
        let mut dec = Vec::new();
        let err = DELTA_SHUFFLE
            .decode(&nine_bytes, Dtype::F64, &mut dec)
            .expect_err("9 bytes is not a multiple of 8");
        assert!(err.contains("not a multiple of the word size"));
    }

    #[test]
    fn decode_rejects_misaligned_f32_input() {
        let three_bytes = [0u8; 3];
        let mut dec = Vec::new();
        let err = DELTA_SHUFFLE
            .decode(&three_bytes, Dtype::F32, &mut dec)
            .expect_err("3 bytes is not a multiple of 4");
        assert!(err.contains("not a multiple of the word size"));

        let five_bytes = [0u8; 5];
        let mut dec = Vec::new();
        let err = DELTA_SHUFFLE
            .decode(&five_bytes, Dtype::F32, &mut dec)
            .expect_err("5 bytes is not a multiple of 4");
        assert!(err.contains("not a multiple of the word size"));
    }

    #[test]
    fn packing_for_dtype_dispatch() {
        use super::super::packing_for;
        use crate::accessions::{INTENSITY_ARRAY, MZ_ARRAY};
        assert_eq!(
            packing_for(MZ_ARRAY, Dtype::F64).id(),
            PackingId::DeltaShuffle
        );
        assert_eq!(
            packing_for(MZ_ARRAY, Dtype::F32).id(),
            PackingId::DeltaShuffle,
            "f32 m/z must also use delta-shuffle"
        );
        assert_eq!(
            packing_for(INTENSITY_ARRAY, Dtype::F32).id(),
            PackingId::Raw,
            "intensity must stay raw"
        );
        assert_eq!(
            packing_for(INTENSITY_ARRAY, Dtype::F64).id(),
            PackingId::Raw,
            "intensity is never delta-shuffled, even at f64"
        );
        assert_eq!(packing_for(0, Dtype::I32).id(), PackingId::Raw);
    }
}
