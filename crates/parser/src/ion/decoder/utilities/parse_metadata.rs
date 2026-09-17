pub(crate) use crate::ion::format::{CODEC_NONE, CODEC_ZSTD};
use crate::{
    ion::{
        IonResult,
        attr_meta::format_accession,
        decoder::{
            decode::{Metadatum, MetadatumValue},
            utilities::{
                common::{
                    decompress_zstd_allow_aligned_padding, read_u32_vec, sum_string_lengths, take,
                },
                decompression_limit::DecompressionLimit,
                meta_column_layout::MetaColumnLayout,
            },
        },
        utilities::common::*,
    },
    mzml::schema::TagId,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_metadata(
    bytes: &[u8],
    item_count: u64,
    meta_count: u64,
    num_count: u64,
    str_count: u64,
    compression_codec: u8,
    expected_uncompressed_bytes: usize,
    decompression_limit: DecompressionLimit,
    layout: MetaColumnLayout,
) -> IonResult<Vec<Metadatum>> {
    let owned;
    let bytes = match compression_codec {
        CODEC_NONE => bytes,
        CODEC_ZSTD => {
            owned = decompress_zstd_allow_aligned_padding(
                bytes,
                expected_uncompressed_bytes,
                decompression_limit,
            )?;
            owned.as_slice()
        }
        other => return Err(format!("unsupported compression_codec={other}").into()),
    };

    let byte_budget = bytes.len();
    let item_count = bound_count(item_count, byte_budget, "metadata item_count")?;
    let meta_count = bound_count(meta_count, byte_budget, "metadata meta_count")?;
    let num_count = bound_count(num_count, byte_budget, "metadata num_count")?;
    let str_count = bound_count(str_count, byte_budget, "metadata str_count")?;

    let mut pos = 0usize;

    let children_index_len = item_count
        .checked_add(1)
        .ok_or_else(|| crate::ion::IonError::from("metadata item_count+1 overflows usize"))?;
    let children_index = read_u32_vec(bytes, &mut pos, children_index_len)?;
    let mut metadatum_owner_ids = read_u32_vec(bytes, &mut pos, meta_count)?;
    let mut metadatum_parent_ids = read_u32_vec(bytes, &mut pos, meta_count)?;
    let metadatum_tag_ids = take(bytes, &mut pos, meta_count, "metadatum tag id")?;
    let metadatum_ref_ids = take(bytes, &mut pos, meta_count, "metadatum ref id")?;
    let metadatum_accessions = read_u32_vec(bytes, &mut pos, meta_count)?;
    let metadatum_unit_refs = take(bytes, &mut pos, meta_count, "metadatum unit ref id")?;
    let metadatum_unit_accs = read_u32_vec(bytes, &mut pos, meta_count)?;
    let value_kinds = take(bytes, &mut pos, meta_count, "metadatum value kind")?;
    let value_indices = rebuild_value_indices(value_kinds)?;

    let numeric_values = read_f64_vec(bytes, &mut pos, num_count)?;

    let string_lengths = read_u32_vec(bytes, &mut pos, str_count)?;
    let string_offsets = rebuild_string_offsets(&string_lengths)?;

    let string_bytes_needed = sum_string_lengths(&string_lengths)?;
    let string_data = take(bytes, &mut pos, string_bytes_needed, "string values")?;

    validate_trailing_bytes(bytes, pos, compression_codec, expected_uncompressed_bytes)?;
    validate_children_index(&children_index, item_count, meta_count)?;

    if layout.ids_reset {
        restore_group_unique_ids(
            &children_index,
            item_count,
            &mut metadatum_owner_ids,
            &mut metadatum_parent_ids,
        )?;
    }

    let mut out = Vec::with_capacity(meta_count);

    for item_index in 0..item_count {
        let meta_start = children_index[item_index] as usize;
        let meta_end = children_index[item_index + 1] as usize;

        for meta_index in meta_start..meta_end {
            let tag_id = TagId::from_u8(metadatum_tag_ids[meta_index]).unwrap_or(TagId::Unknown);
            let value = parse_value(
                value_kinds[meta_index],
                value_indices[meta_index],
                &numeric_values,
                &string_offsets,
                &string_lengths,
                string_data,
            )?;

            let accession = format_accession(
                metadatum_ref_ids[meta_index],
                metadatum_accessions[meta_index],
            );
            let unit_accession = format_accession(
                metadatum_unit_refs[meta_index],
                metadatum_unit_accs[meta_index],
            );

            out.push(Metadatum {
                item_index: item_index as u32,
                id: metadatum_owner_ids[meta_index],
                parent_id: metadatum_parent_ids[meta_index],
                tag_id,
                accession,
                unit_accession,
                value,
            });
        }
    }

    Ok(out)
}

#[inline]
fn validate_trailing_bytes(
    bytes: &[u8],
    pos: usize,
    compression_codec: u8,
    expected_uncompressed_bytes: usize,
) -> IonResult<()> {
    if compression_codec == CODEC_ZSTD {
        if pos != bytes.len() {
            return Err("trailing bytes in decompressed metadata section".into());
        }
    } else {
        let trailing = &bytes[pos..];
        if trailing.len() > 7 || trailing.iter().any(|&b| b != 0) {
            return Err("trailing bytes in metadata section".into());
        }
        let _ = expected_uncompressed_bytes;
    }
    Ok(())
}

#[inline]
fn validate_children_index(
    children_index: &[u32],
    item_count: usize,
    meta_count: usize,
) -> IonResult<()> {
    if children_index.is_empty() || children_index[0] != 0 {
        return Err("CI[0] must be 0".into());
    }
    if children_index[item_count] as usize != meta_count {
        return Err("CI[last] must equal meta_count".into());
    }
    let mut previous = 0u32;
    for &entry in children_index {
        if entry < previous || (entry as usize) > meta_count {
            return Err("CI is not monotonic or out of range".into());
        }
        previous = entry;
    }
    Ok(())
}

const SHARED_LIST_NODE_ID: u32 = 1;

#[inline]
fn restore_group_unique_ids(
    children_index: &[u32],
    item_count: usize,
    owner_ids: &mut [u32],
    parent_ids: &mut [u32],
) -> IonResult<()> {
    let largest_local_id = owner_ids
        .iter()
        .chain(parent_ids.iter())
        .copied()
        .max()
        .unwrap_or(0);
    let stride = largest_local_id
        .checked_add(1)
        .ok_or_else(|| crate::ion::IonError::from("metadata id remap: stride overflows u32"))?;

    for item_index in 0..item_count {
        let meta_start = children_index[item_index] as usize;
        let meta_end = children_index[item_index + 1] as usize;
        let item_base = (item_index as u32).checked_mul(stride).ok_or_else(|| {
            crate::ion::IonError::from("metadata id remap: item base overflows u32")
        })?;
        for meta_index in meta_start..meta_end {
            owner_ids[meta_index] = restore_one_id(owner_ids[meta_index], item_base)?;
            parent_ids[meta_index] = restore_one_id(parent_ids[meta_index], item_base)?;
        }
    }
    Ok(())
}

#[inline]
fn restore_one_id(local_id: u32, item_base: u32) -> IonResult<u32> {
    if local_id == SHARED_LIST_NODE_ID {
        return Ok(local_id);
    }
    item_base.checked_add(local_id).ok_or_else(|| {
        crate::ion::IonError::from("metadata id remap: group-unique id overflows u32")
    })
}

#[inline]
fn parse_value(
    value_kind: u8,
    value_index: u32,
    numeric_values: &[f64],
    string_offsets: &[u32],
    string_lengths: &[u32],
    string_data: &[u8],
) -> IonResult<MetadatumValue> {
    match value_kind {
        0 => {
            let index = value_index as usize;
            if index >= numeric_values.len() {
                return Err("numeric VI out of range".into());
            }
            Ok(MetadatumValue::Number(numeric_values[index]))
        }
        1 => {
            let index = value_index as usize;
            if index >= string_offsets.len() || index >= string_lengths.len() {
                return Err("string VI out of range".into());
            }
            let offset = string_offsets[index] as usize;
            let length = string_lengths[index] as usize;
            if offset
                .checked_add(length)
                .is_none_or(|end| end > string_data.len())
            {
                return Err("string slice out of bounds".into());
            }
            if length == 0 {
                return Ok(MetadatumValue::Text(String::new()));
            }
            let slice = &string_data[offset..offset + length];
            let text = std::str::from_utf8(slice)
                .map_err(|_| "string metadata is not valid UTF-8")?
                .to_string();
            Ok(MetadatumValue::Text(text))
        }
        2 => Ok(MetadatumValue::Empty),
        other => Err(format!("invalid value kind VK={other}").into()),
    }
}

#[inline]
fn rebuild_value_indices(value_kinds: &[u8]) -> IonResult<Vec<u32>> {
    let mut next_numeric_ordinal = 0u32;
    let mut next_string_ordinal = 0u32;
    let mut value_indices = Vec::with_capacity(value_kinds.len());
    for &kind in value_kinds {
        match kind {
            0 => {
                value_indices.push(next_numeric_ordinal);
                next_numeric_ordinal = next_numeric_ordinal.checked_add(1).ok_or_else(|| {
                    crate::ion::IonError::from("numeric value ordinal overflows u32")
                })?;
            }
            1 => {
                value_indices.push(next_string_ordinal);
                next_string_ordinal = next_string_ordinal.checked_add(1).ok_or_else(|| {
                    crate::ion::IonError::from("string value ordinal overflows u32")
                })?;
            }
            _ => value_indices.push(0),
        }
    }
    Ok(value_indices)
}

#[inline]
fn rebuild_string_offsets(string_lengths: &[u32]) -> IonResult<Vec<u32>> {
    let mut string_offsets = Vec::with_capacity(string_lengths.len());
    let mut running_offset = 0u32;
    for &length in string_lengths {
        string_offsets.push(running_offset);
        running_offset = running_offset
            .checked_add(length)
            .ok_or_else(|| crate::ion::IonError::from("string offset prefix sum overflows u32"))?;
    }
    Ok(string_offsets)
}

#[inline]
fn bound_count(count: u64, byte_budget: usize, ctx: &'static str) -> IonResult<usize> {
    if count > byte_budget as u64 {
        return Err(
            format!("{ctx}: declared count {count} exceeds byte budget {byte_budget}").into(),
        );
    }
    Ok(count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_utf8_string_value() {
        let numeric_values: [f64; 0] = [];
        let string_offsets = [0u32];
        let string_lengths = [3u32];
        let string_data: [u8; 3] = [b'A', 0xFF, b'B'];

        let result = parse_value(
            1,
            0,
            &numeric_values,
            &string_offsets,
            &string_lengths,
            &string_data,
        );

        assert!(result.is_err());
    }

    #[test]
    fn accepts_valid_multibyte_utf8_string_value() {
        let numeric_values: [f64; 0] = [];
        let text = "café";
        let string_data = text.as_bytes();
        let string_offsets = [0u32];
        let string_lengths = [string_data.len() as u32];

        let value = parse_value(
            1,
            0,
            &numeric_values,
            &string_offsets,
            &string_lengths,
            string_data,
        )
        .expect("valid UTF-8 must parse");

        match value {
            MetadatumValue::Text(t) => assert_eq!(t, text),
            other => panic!("expected Text value, got {other:?}"),
        }
    }
}
