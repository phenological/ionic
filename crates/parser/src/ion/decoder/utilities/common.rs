use cosmoz::{DecompressOptions, Decoder, compressed_size};

use crate::ion::{
    IonError, IonResult,
    attr_meta::{AccessionTail, CV_CODE_UNKNOWN, cv_ref_code_from_str, parse_accession_tail},
    decoder::{
        decode::{Metadatum, MetadatumValue},
        utilities::decompression_limit::DecompressionLimit,
    },
};

#[inline]
pub(crate) fn take<'a>(
    bytes: &'a [u8],
    pos: &mut usize,
    n: usize,
    field: &'static str,
) -> IonResult<&'a [u8]> {
    let start = *pos;
    let end = start
        .checked_add(n)
        .ok_or_else(|| IonError::from(format!("overflow while reading {field}")))?;
    if end > bytes.len() {
        return Err(format!(
            "unexpected EOF while reading {field}: need {n} bytes at pos {pos}, len {}",
            bytes.len()
        )
        .into());
    }
    *pos = end;
    Ok(&bytes[start..end])
}

#[inline]
pub(crate) fn read_u16_le_at(bytes: &[u8], pos: &mut usize, ctx: &str) -> IonResult<u16> {
    if *pos + 2 > bytes.len() {
        return Err(format!("{ctx}: not enough bytes for u16 at offset {pos}").into());
    }
    let v = u16::from_le_bytes(bytes[*pos..*pos + 2].try_into().unwrap());
    *pos += 2;
    Ok(v)
}

#[inline]
pub(crate) fn read_u32_le_at(bytes: &[u8], pos: &mut usize, field: &'static str) -> IonResult<u32> {
    let s = take(bytes, pos, 4, field)?;
    Ok(u32::from_le_bytes(s.try_into().unwrap()))
}

#[inline]
pub(crate) fn read_u64_le_at(bytes: &[u8], pos: &mut usize, field: &'static str) -> IonResult<u64> {
    let s = take(bytes, pos, 8, field)?;
    Ok(u64::from_le_bytes(s.try_into().unwrap()))
}

#[inline]
pub(crate) fn read_u32_vec(bytes: &[u8], pos: &mut usize, n: usize) -> IonResult<Vec<u32>> {
    let byte_len = n
        .checked_mul(4)
        .ok_or_else(|| IonError::from("u32 vector length overflows usize"))?;
    let raw = take(bytes, pos, byte_len, "u32 vector")?;
    let mut out = Vec::with_capacity(n);
    for chunk in raw.as_chunks::<4>().0 {
        out.push(u32::from_le_bytes(*chunk));
    }
    Ok(out)
}

#[inline]
pub(crate) fn read_f64_vec(bytes: &[u8], pos: &mut usize, n: usize) -> IonResult<Vec<f64>> {
    let byte_len = n
        .checked_mul(8)
        .ok_or_else(|| IonError::from("f64 vector length overflows usize"))?;
    let raw = take(bytes, pos, byte_len, "f64 vector")?;
    let mut out = Vec::with_capacity(n);
    for chunk in raw.as_chunks::<8>().0 {
        out.push(f64::from_le_bytes(*chunk));
    }
    Ok(out)
}

#[inline]
pub(crate) fn decompress_zstd(
    comp: &[u8],
    expected: usize,
    budget: DecompressionLimit,
) -> IonResult<Vec<u8>> {
    if expected == 0 {
        return Ok(Vec::new());
    }

    budget.validate(comp.len(), expected)?;

    let mut out = vec![0u8; expected];
    let mut decoder = Decoder::new(&DecompressOptions::default());
    let actual = decoder
        .decompress_into(comp, &mut out)
        .map_err(|err| IonError::from(format!("zstd decode failed: {err:?}")))?;

    if actual != expected {
        return Err(format!("zstd: bad decoded size (got={actual}, expected={expected})").into());
    }

    Ok(out)
}

#[inline]
pub(crate) fn decompress_zstd_allow_aligned_padding(
    input: &[u8],
    expected: usize,
    budget: DecompressionLimit,
) -> IonResult<Vec<u8>> {
    if expected == 0 {
        return Ok(Vec::new());
    }

    if let Ok(n) = compressed_size(input)
        && n > 0
        && n <= input.len()
        && let Ok(v) = decompress_zstd(&input[..n], expected, budget)
    {
        return Ok(v);
    }

    match decompress_zstd(input, expected, budget) {
        Ok(v) => Ok(v),
        Err(first_err) => {
            let mut trimmed = input;
            for _ in 0..7 {
                let Some(&last) = trimmed.last() else { break };
                if last != 0 {
                    break;
                }
                trimmed = &trimmed[..trimmed.len() - 1];
                if let Ok(v) = decompress_zstd(trimmed, expected, budget) {
                    return Ok(v);
                }
            }
            Err(first_err)
        }
    }
}

#[inline]
pub(crate) fn is_cv_prefix(p: &str) -> bool {
    cv_ref_code_from_str(Some(p)) != CV_CODE_UNKNOWN
}

#[inline]
pub(crate) fn unit_cv_ref(unit_accession: Option<&str>) -> Option<String> {
    unit_accession
        .and_then(|ua| ua.split_once(':'))
        .map(|(pref, _)| pref.to_owned())
}

#[inline]
pub(crate) fn value_to_opt_string(v: &MetadatumValue) -> Option<String> {
    match v {
        MetadatumValue::Number(x) => Some(x.to_string()),
        MetadatumValue::Text(s) => Some(s.clone()),
        MetadatumValue::Empty => None,
    }
}

#[inline]
pub(crate) fn get_attr_u32(rows: &[&Metadatum], tail: AccessionTail) -> Option<u32> {
    for m in rows {
        if let Some(acc) = m.accession.as_deref()
            && parse_accession_tail(Some(acc)) == tail
        {
            return match &m.value {
                MetadatumValue::Number(n)
                    if n.is_finite() && *n >= 0.0 && *n <= u32::MAX as f64 =>
                {
                    Some(*n as u32)
                }
                MetadatumValue::Text(s) => s.parse::<u32>().ok(),
                _ => None,
            };
        }
    }
    None
}

#[inline]
pub(crate) fn get_attr_text(rows: &[&Metadatum], tail: AccessionTail) -> Option<String> {
    for m in rows {
        if let Some(acc) = m.accession.as_deref()
            && parse_accession_tail(Some(acc)) == tail
        {
            return match &m.value {
                MetadatumValue::Text(s) => Some(s.clone()),
                MetadatumValue::Number(n) => Some(n.to_string()),
                MetadatumValue::Empty => None,
            };
        }
    }
    None
}

#[inline]
pub(crate) fn sum_string_lengths(string_lengths: &[u32]) -> IonResult<usize> {
    let mut total: usize = 0;
    for &length in string_lengths {
        total = total
            .checked_add(length as usize)
            .ok_or_else(|| IonError::from("string_lengths sum overflows usize"))?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use cosmoz::{CompressOptions, Encoder};

    use super::*;
    use crate::ion::decoder::utilities::decompression_limit::DecompressionLimit;

    fn compress_to_frame(data: &[u8]) -> Vec<u8> {
        let options = CompressOptions::default();
        let mut encoder = Encoder::new(&options).unwrap();
        let mut out = vec![0u8; encoder.max_compressed_size(data.len())];
        let written = encoder.compress_into(data, &mut out).unwrap();
        out.truncate(written);
        out
    }

    #[test]
    fn decompress_zstd_round_trips_bytes() {
        let original: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let compressed = compress_to_frame(&original);
        let restored =
            decompress_zstd(&compressed, original.len(), DecompressionLimit::default()).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn decompress_zstd_empty_returns_empty() {
        let restored = decompress_zstd(&[], 0, DecompressionLimit::default()).unwrap();
        assert!(restored.is_empty());
    }
}
