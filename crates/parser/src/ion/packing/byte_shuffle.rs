use super::{Dtype, IonResult, Packing, PackingId, PackingInput};
use crate::ion::{IonError, byte_transpose};

pub(crate) static BYTE_SHUFFLE: ByteShuffle = ByteShuffle;
pub(crate) struct ByteShuffle;

impl Packing for ByteShuffle {
    fn id(&self) -> PackingId {
        PackingId::ByteShuffle
    }

    fn supports(&self, _dtype: Dtype) -> bool {
        true
    }

    fn encode(&self, input: PackingInput<'_>, out: &mut Vec<u8>) -> IonResult<()> {
        let (bytes, stride) = typed_to_le_bytes_and_stride(input);
        out.resize(bytes.len(), 0);
        byte_transpose::shuffle_with_tail(&bytes, out, stride);
        Ok(())
    }

    fn decode(&self, input: &[u8], dtype: Dtype, out: &mut Vec<u8>) -> IonResult<()> {
        let stride = dtype.byte_stride();
        if !input.len().is_multiple_of(stride) {
            return Err(IonError::from(
                "byte shuffle: input length is not a multiple of the stride",
            ));
        }
        out.resize(input.len(), 0);
        byte_transpose::unshuffle(input, out, stride);
        Ok(())
    }
}

fn typed_to_le_bytes_and_stride(input: PackingInput<'_>) -> (Vec<u8>, usize) {
    match input {
        PackingInput::F64(s) => (s.iter().flat_map(|v| v.to_le_bytes()).collect(), 8),
        PackingInput::F32(s) => (s.iter().flat_map(|v| v.to_le_bytes()).collect(), 4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f64_roundtrip(values: &[f64]) -> Vec<f64> {
        let mut enc = Vec::new();
        BYTE_SHUFFLE
            .encode(PackingInput::F64(values), &mut enc)
            .unwrap();
        let mut dec = Vec::new();
        BYTE_SHUFFLE.decode(&enc, Dtype::F64, &mut dec).unwrap();
        dec.as_chunks::<8>()
            .0
            .iter()
            .map(|c| f64::from_le_bytes(*c))
            .collect()
    }

    #[test]
    fn empty_produces_no_output() {
        let got = f64_roundtrip(&[]);
        assert!(got.is_empty());
    }

    #[test]
    fn single_f64_roundtrip() {
        let input = [1.23456789f64];
        let got = f64_roundtrip(&input);
        assert_eq!(got[0].to_bits(), input[0].to_bits());
    }

    #[test]
    fn multiple_f64_roundtrip() {
        let input: Vec<f64> = (0..16).map(|i| i as f64 * 1.5).collect();
        let got = f64_roundtrip(&input);
        for (a, b) in got.iter().zip(input.iter()) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn encoded_length_equals_input_length() {
        let input = vec![1.0f64; 8];
        let mut enc = Vec::new();
        BYTE_SHUFFLE
            .encode(PackingInput::F64(&input), &mut enc)
            .unwrap();
        assert_eq!(enc.len(), input.len() * 8);
    }

    #[test]
    fn decode_rejects_misaligned_input() {
        let seven_bytes = [0u8; 7];
        let mut out = Vec::new();
        let err = BYTE_SHUFFLE
            .decode(&seven_bytes, Dtype::F64, &mut out)
            .expect_err("7 bytes is not a multiple of the f64 stride of 8");
        assert!(err.contains("not a multiple of the stride"));

        let nine_bytes = [0u8; 9];
        let mut out = Vec::new();
        let err = BYTE_SHUFFLE
            .decode(&nine_bytes, Dtype::F64, &mut out)
            .expect_err("9 bytes is not a multiple of the f64 stride of 8");
        assert!(err.contains("not a multiple of the stride"));
    }

    #[test]
    fn decode_accepts_stride_multiple_input() {
        let sixteen_bytes = [0u8; 16];
        let mut out = Vec::new();
        BYTE_SHUFFLE
            .decode(&sixteen_bytes, Dtype::F64, &mut out)
            .expect("16 bytes is a multiple of the f64 stride of 8");
        assert_eq!(out.len(), 16);
    }
}
