use super::{Dtype, IonResult, Packing, PackingId, PackingInput};

pub(crate) static RAW: Raw = Raw;
pub(crate) struct Raw;

impl Packing for Raw {
    fn id(&self) -> PackingId {
        PackingId::Raw
    }

    fn supports(&self, _dtype: Dtype) -> bool {
        true
    }

    fn encode(&self, input: PackingInput<'_>, out: &mut Vec<u8>) -> IonResult<()> {
        match input {
            PackingInput::F64(values) => {
                out.extend(values.iter().flat_map(|v| v.to_le_bytes()))
            }
            PackingInput::F32(values) => {
                out.extend(values.iter().flat_map(|v| v.to_le_bytes()))
            }
        }
        Ok(())
    }

    fn decode(&self, input: &[u8], _dtype: Dtype, out: &mut Vec<u8>) -> IonResult<()> {
        out.extend_from_slice(input);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_writes_f64_le_bytes() {
        let mut out = Vec::new();
        RAW.encode(PackingInput::F64(&[1.0, 2.0]), &mut out)
            .unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(&1.0f64.to_le_bytes());
        expected.extend_from_slice(&2.0f64.to_le_bytes());
        assert_eq!(out, expected);
    }

    #[test]
    fn encode_writes_f32_le_bytes() {
        let mut out = Vec::new();
        RAW.encode(PackingInput::F32(&[1.0, 2.0]), &mut out)
            .unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(&1.0f32.to_le_bytes());
        expected.extend_from_slice(&2.0f32.to_le_bytes());
        assert_eq!(out, expected);
    }

    #[test]
    fn decode_copies_bytes() {
        let mut out = Vec::new();
        RAW.decode(&[5, 6, 7, 8], Dtype::F64, &mut out).unwrap();
        assert_eq!(out, [5, 6, 7, 8]);
    }

    #[test]
    fn decode_empty_input_produces_no_output() {
        let mut out = Vec::new();
        RAW.decode(&[], Dtype::F64, &mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn f64_roundtrip() {
        let input = [1.5f64, -2.25, 3.0];
        let mut enc = Vec::new();
        RAW.encode(PackingInput::F64(&input), &mut enc).unwrap();
        let mut dec = Vec::new();
        RAW.decode(&enc, Dtype::F64, &mut dec).unwrap();
        let got: Vec<f64> = dec
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| f64::from_le_bytes(*c))
            .collect();
        assert_eq!(got, input);
    }
}
