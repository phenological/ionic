use std::{borrow::Cow, collections::HashMap};

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use rayon::prelude::*;

use super::*;
use crate::ion::decoder::utilities::byte_source::SourceBytes;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum MetadatumValue {
    Number(f64),
    Text(String),
    Empty,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct Metadatum {
    pub item_index: u32,
    pub id: u32,
    pub parent_id: u32,
    pub tag_id: TagId,
    pub accession: Option<String>,
    pub unit_accession: Option<String>,
    pub value: MetadatumValue,
}

pub(crate) struct MzmlConverter<'d> {
    pub(crate) decoder: &'d mut IonReader,
}

impl<'d> MzmlConverter<'d> {
    #[inline]
    pub(crate) fn new(decoder: &'d mut IonReader) -> Self {
        Self { decoder }
    }

    pub(crate) fn metadata_only(decoder: &IonReader) -> IonResult<MzML> {
        let global_meta = decoder.global_metadata()?;
        let global_lookup = ChildrenLookup::new(&global_meta);
        let meta_refs: Vec<&Metadatum> = global_meta.iter().collect();
        let policy = DefaultMetadataPolicy;

        let mut owner_rows = OwnerRows::with_capacity(global_meta.len());
        for metadatum in &global_meta {
            owner_rows.insert(metadatum.id, metadatum);
        }

        let run_id = global_lookup
            .all_ids(TagId::Run)
            .first()
            .copied()
            .unwrap_or(0);
        let rows = owner_rows.get(run_id);

        let mut param_buffer: Vec<&Metadatum> = Vec::new();
        global_lookup.get_param_rows_into(&owner_rows, run_id, &policy, &mut param_buffer);
        let (cv_params, user_params) = parse_cv_and_user_params(&param_buffer);

        let spectrum_list = assemble_spectrum_list(&decoder.spectrum_metadata_grouped()?, &policy);
        let chromatogram_list =
            assemble_chromatogram_list(&decoder.chromatogram_metadata_grouped()?, &policy);

        let source_file_ref_list = parse_run_source_file_refs(&owner_rows, &global_lookup, run_id);

        Ok(MzML {
            cv_list: parse_cv_list(&meta_refs, &global_lookup),
            file_description: parse_file_description(&meta_refs, &global_lookup, &policy),
            referenceable_param_group_list: parse_referenceable_param_group_list(
                &meta_refs,
                &global_lookup,
                &policy,
            ),
            sample_list: parse_sample_list(&meta_refs, &global_lookup, &policy),
            instrument_list: parse_instrument_list(&meta_refs, &global_lookup, &policy),
            software_list: parse_software_list(&meta_refs, &global_lookup, &policy),
            data_processing_list: parse_data_processing_list(&meta_refs, &global_lookup, &policy),
            scan_settings_list: parse_scan_settings_list(&meta_refs, &global_lookup, &policy),
            run: Run {
                id: get_attr_text(rows, ACC_ATTR_ID).unwrap_or_default(),
                start_time_stamp: get_attr_text(rows, ACC_ATTR_START_TIME_STAMP)
                    .filter(|value| !value.is_empty()),
                default_instrument_configuration_ref: get_attr_text(
                    rows,
                    ACC_ATTR_DEFAULT_INSTRUMENT_CONFIGURATION_REF,
                )
                .or_else(|| get_attr_text(rows, ACC_ATTR_INSTRUMENT_CONFIGURATION_REF)),
                default_source_file_ref: get_attr_text(rows, ACC_ATTR_DEFAULT_SOURCE_FILE_REF),
                sample_ref: get_attr_text(rows, ACC_ATTR_SAMPLE_REF),
                referenceable_param_group_refs: global_lookup
                    .ids_for(run_id, TagId::ReferenceableParamGroupRef)
                    .iter()
                    .filter_map(|&ref_id| {
                        get_attr_text(owner_rows.get(ref_id), ACC_ATTR_REF)
                            .map(|r| ReferenceableParamGroupRef { r#ref: r })
                    })
                    .collect(),
                cv_params,
                user_params,
                source_file_ref_list,
                spectrum_list,
                chromatogram_list,
            },
        })
    }

    pub(crate) fn full(&mut self) -> IonResult<MzML> {
        let mut mzml = Self::metadata_only(self.decoder)?;

        if let Some(spectrum_list) = mzml.run.spectrum_list.as_mut() {
            attach_binaries(
                self.decoder.spec_entries_buf.as_ref(),
                self.decoder.spec_array_addresses.as_ref(),
                &mut spectrum_list.spectra,
                &self.decoder.spec_container,
                "spec",
                self.decoder.parallel,
            )?;
        }

        if let (Some(chrom_list), Some(container)) = (
            mzml.run.chromatogram_list.as_mut(),
            self.decoder.chrom_container.as_ref(),
        ) {
            attach_binaries(
                self.decoder.chrom_entries_buf.as_ref(),
                self.decoder.chrom_array_addresses.as_ref(),
                &mut chrom_list.chromatograms,
                container,
                "chrom",
                self.decoder.parallel,
            )?;
        }

        Ok(mzml)
    }
}

fn assemble_spectrum_list(
    groups: &[Vec<Metadatum>],
    policy: &DefaultMetadataPolicy,
) -> Option<SpectrumList> {
    let mut combined: Option<SpectrumList> = None;
    let mut index_base = 0u32;
    for group in groups {
        let refs: Vec<&Metadatum> = group.iter().collect();
        let Some(list) =
            parse_spectrum_list(&refs, &ChildrenLookup::new(group), policy, index_base)
        else {
            continue;
        };
        index_base += list.spectra.len() as u32;
        match combined.as_mut() {
            None => combined = Some(list),
            Some(existing) => existing.spectra.extend(list.spectra),
        }
    }
    if let Some(list) = combined.as_mut() {
        if list.count.is_none() {
            list.count = Some(list.spectra.len());
        }
    }
    combined
}

fn assemble_chromatogram_list(
    groups: &[Vec<Metadatum>],
    policy: &DefaultMetadataPolicy,
) -> Option<ChromatogramList> {
    let mut combined: Option<ChromatogramList> = None;
    let mut index_base = 0u32;
    for group in groups {
        let refs: Vec<&Metadatum> = group.iter().collect();
        let Some(list) =
            parse_chromatogram_list(&refs, &ChildrenLookup::new(group), policy, index_base)
        else {
            continue;
        };
        index_base += list.chromatograms.len() as u32;
        match combined.as_mut() {
            None => combined = Some(list),
            Some(existing) => existing.chromatograms.extend(list.chromatograms),
        }
    }
    if let Some(list) = combined.as_mut() {
        if list.count.is_none() {
            list.count = Some(list.chromatograms.len());
        }
    }
    combined
}

pub(crate) trait BinaryArrayOwner {
    fn binary_data_array_list_mut(&mut self) -> &mut Option<BinaryDataArrayList>;
}

impl BinaryArrayOwner for Spectrum {
    #[inline]
    fn binary_data_array_list_mut(&mut self) -> &mut Option<BinaryDataArrayList> {
        &mut self.binary_data_array_list
    }
}

impl BinaryArrayOwner for Chromatogram {
    #[inline]
    fn binary_data_array_list_mut(&mut self) -> &mut Option<BinaryDataArrayList> {
        &mut self.binary_data_array_list
    }
}

pub(crate) fn attach_binaries<E: BinaryArrayOwner>(
    entries_buf: &[u8],
    array_addresses_buf: &[u8],
    entries: &mut [E],
    container: &BlockReader<DefaultBlockProcessor>,
    ctx: &'static str,
    parallel: bool,
) -> IonResult<()> {
    let mut refs = Vec::new();
    let mut blocks = HashMap::new();
    #[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
    let _ = parallel;

    for index in 0..entries.len() {
        let Some(item_refs) =
            read_array_addresses_from_buffers(entries_buf, array_addresses_buf, index)
        else {
            continue;
        };
        if item_refs.is_empty() {
            continue;
        }
        for array_address in item_refs.as_slice() {
            let stride = if array_address.encoded_len > 0 {
                1
            } else {
                dtype_stride(array_address.dtype)
            };
            if let Some(old) = blocks.insert(array_address.block_id, stride)
                && old != stride
            {
                return Err(IonError::from(format!(
                    "{ctx}: stride mismatch for block {} (expected {old}, got {stride})",
                    array_address.block_id
                )));
            }
        }
        refs.push((index, item_refs));
    }

    let mut block_list: Vec<_> = blocks.into_iter().collect();
    block_list.sort_unstable_by_key(|(block_id, _)| *block_id);

    let load = |(block_id, stride): (u32, usize)| -> IonResult<(u32, SourceBytes)> {
        Ok((block_id, container.read_block(block_id, stride, ctx)?))
    };

    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    let data: HashMap<u32, SourceBytes> = if parallel && block_list.len() >= 4 {
        block_list
            .into_par_iter()
            .map(load)
            .collect::<IonResult<Vec<_>>>()?
            .into_iter()
            .collect()
    } else {
        block_list
            .into_iter()
            .map(load)
            .collect::<IonResult<Vec<_>>>()?
            .into_iter()
            .collect()
    };
    #[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
    let data: HashMap<u32, SourceBytes> = block_list
        .into_iter()
        .map(load)
        .collect::<IonResult<Vec<_>>>()?
        .into_iter()
        .collect();

    for (index, item_refs) in refs {
        let groups = group_arrays(item_refs.as_slice())?;
        let list = entries[index]
            .binary_data_array_list_mut()
            .get_or_insert_with(BinaryDataArrayList::default);

        for group in groups {
            if let [array_address] = group.refs.as_slice() {
                let bytes = unfiltered_ref_bytes(&data, &group, array_address, ctx)?;
                attach_logical_array(
                    list,
                    group.array_type,
                    group.array_cv_code,
                    group.dtype,
                    &bytes,
                )?;
            } else {
                let mut total = 0usize;
                for array_address in &group.refs {
                    total += checked_ref_bytes(&data, array_address, ctx)?.len();
                }

                let mut concatenated = Vec::new();
                concatenated.reserve_exact(total);
                for array_address in &group.refs {
                    let bytes = unfiltered_ref_bytes(&data, &group, array_address, ctx)?;
                    concatenated.extend_from_slice(&bytes);
                }

                attach_logical_array(
                    list,
                    group.array_type,
                    group.array_cv_code,
                    group.dtype,
                    &concatenated,
                )?;
            }
        }
        list.count = Some(list.binary_data_arrays.len());
    }

    Ok(())
}

fn ref_byte_range(array_address: &ArrayAddress, ctx: &str) -> IonResult<(usize, usize)> {
    let (element_offset, count, stride) = address_read_params(array_address);
    let start = usize::try_from(element_offset)
        .ok()
        .and_then(|offset| offset.checked_mul(stride))
        .ok_or_else(|| {
            IonError::from(format!(
                "{ctx}: item range overflow for block {}",
                array_address.block_id
            ))
        })?;
    let end = usize::try_from(count)
        .ok()
        .and_then(|c| c.checked_mul(stride))
        .and_then(|len| start.checked_add(len))
        .ok_or_else(|| {
            IonError::from(format!(
                "{ctx}: item range overflow for block {}",
                array_address.block_id
            ))
        })?;
    Ok((start, end))
}

fn checked_ref_bytes<'d>(
    data: &'d HashMap<u32, SourceBytes>,
    array_address: &ArrayAddress,
    ctx: &str,
) -> IonResult<&'d [u8]> {
    let block = data.get(&array_address.block_id).ok_or_else(|| {
        IonError::from(format!("{ctx}: missing block {}", array_address.block_id))
    })?;
    let (start, end) = ref_byte_range(array_address, ctx)?;
    block.get(start..end).ok_or_else(|| {
        IonError::from(format!(
            "{ctx}: item range [{start}..{end}] out of bounds for block {} (len={})",
            array_address.block_id,
            block.len()
        ))
    })
}

fn unfiltered_ref_bytes<'d>(
    data: &'d HashMap<u32, SourceBytes>,
    group: &ArrayGroup,
    array_address: &ArrayAddress,
    ctx: &str,
) -> IonResult<Cow<'d, [u8]>> {
    let raw = checked_ref_bytes(data, array_address, ctx)?;
    unfilter_array_bytes(raw, group.dtype, group.array_filter)
}

pub(crate) fn attach_logical_array(
    binary_array_list: &mut BinaryDataArrayList,
    array_type: u32,
    array_cv_code: u8,
    dtype: u8,
    decoded_bytes: &[u8],
) -> IonResult<()> {
    let binary = decoded_bytes_to_binary_data(decoded_bytes, dtype)?;
    let numeric_type = dtype_to_numeric_type(dtype)?;

    let empty_index = binary_array_list
        .binary_data_arrays
        .iter()
        .position(|array| binary_array_has_type(array, array_type) && array.binary.is_none());

    let binary_array = if let Some(index) = empty_index {
        &mut binary_array_list.binary_data_arrays[index]
    } else {
        binary_array_list
            .binary_data_arrays
            .push(make_binary_array_stub(array_type, array_cv_code));
        binary_array_list.binary_data_arrays.last_mut().unwrap()
    };

    binary_array.binary = Some(binary);
    sync_numeric_meta(binary_array, numeric_type);
    Ok(())
}

fn raw_to_vec<T>(raw: &[u8], elem_size: usize, read: impl Fn(&[u8]) -> T) -> IonResult<Vec<T>> {
    if !raw.len().is_multiple_of(elem_size) {
        return Err(IonError::from(format!(
            "array: length {} not a multiple of {elem_size}",
            raw.len()
        )));
    }
    let mut out = Vec::with_capacity(raw.len() / elem_size);
    out.extend(raw.chunks_exact(elem_size).map(read));
    Ok(out)
}

pub(crate) fn decoded_bytes_to_binary_data(bytes: &[u8], dtype: u8) -> IonResult<NumericArray> {
    match dtype {
        FILE_DTYPE_F64 => Ok(NumericArray::F64(raw_to_vec(bytes, 8, |c| {
            f64::from_le_bytes(c.try_into().unwrap())
        })?)),
        FILE_DTYPE_F32 => Ok(NumericArray::F32(raw_to_vec(bytes, 4, |c| {
            f32::from_le_bytes(c.try_into().unwrap())
        })?)),
        FILE_DTYPE_F16 => Ok(NumericArray::F16(raw_to_vec(bytes, 2, |c| {
            u16::from_le_bytes(c.try_into().unwrap())
        })?)),
        FILE_DTYPE_I16 => Ok(NumericArray::I16(raw_to_vec(bytes, 2, |c| {
            i16::from_le_bytes(c.try_into().unwrap())
        })?)),
        FILE_DTYPE_I32 => Ok(NumericArray::I32(raw_to_vec(bytes, 4, |c| {
            i32::from_le_bytes(c.try_into().unwrap())
        })?)),
        FILE_DTYPE_I64 => Ok(NumericArray::I64(raw_to_vec(bytes, 8, |c| {
            i64::from_le_bytes(c.try_into().unwrap())
        })?)),
        _ => Err(IonError::from(format!(
            "unsupported dtype {dtype} in binary array"
        ))),
    }
}

fn dtype_to_numeric_type(dtype: u8) -> IonResult<NumericType> {
    match dtype {
        FILE_DTYPE_F64 => Ok(NumericType::Float64),
        FILE_DTYPE_F32 => Ok(NumericType::Float32),
        FILE_DTYPE_F16 => Ok(NumericType::Float16),
        FILE_DTYPE_I16 => Ok(NumericType::Int16),
        FILE_DTYPE_I32 => Ok(NumericType::Int32),
        FILE_DTYPE_I64 => Ok(NumericType::Int64),
        _ => Err(IonError::BadDtype {
            dtype,
            kind: "numeric type",
        }),
    }
}

fn make_binary_array_stub(array_type: u32, array_cv_code: u8) -> BinaryDataArray {
    let cv_ref = crate::ion::attr_meta::cv_ref_prefix_from_code(array_cv_code).unwrap_or("MS");
    let accession = crate::ion::attr_meta::format_accession(array_cv_code, array_type)
        .unwrap_or_else(|| format_accession(array_type));
    BinaryDataArray {
        cv_params: vec![CvParam {
            cv_ref: Some(cv_ref.to_string()),
            accession: Some(accession),
            name: String::new(),
            ..Default::default()
        }],
        ..BinaryDataArray::default()
    }
}

#[inline]
fn sync_numeric_meta(binary_array: &mut BinaryDataArray, numeric_type: NumericType) {
    let target = match numeric_type {
        NumericType::Float16 => 1_000_520,
        NumericType::Float32 => 1_000_521,
        NumericType::Float64 => 1_000_523,
        NumericType::Int16 => 1_000_518,
        NumericType::Int32 => 1_000_519,
        NumericType::Int64 => 1_000_522,
    };
    let param = CvParam {
        cv_ref: Some("MS".into()),
        accession: Some(format_accession(target)),
        name: match target {
            1_000_520 => "16-bit float",
            1_000_521 => "32-bit float",
            1_000_523 => "64-bit float",
            1_000_518 => "16-bit integer",
            1_000_519 => "32-bit integer",
            1_000_522 => "64-bit integer",
            _ => "numeric",
        }
        .into(),
        ..Default::default()
    };
    match binary_array
        .cv_params
        .iter()
        .position(|p| is_numeric_acc(parse_accession_tail(p.accession.as_deref())))
    {
        Some(position) => binary_array.cv_params[position] = param,
        None => binary_array.cv_params.push(param),
    }
    binary_array.numeric_type = Some(numeric_type);
}

#[inline]
fn is_numeric_acc(tail: AccessionTail) -> bool {
    matches!(
        tail.raw(),
        INT_16BIT | INT_32BIT | INT_64BIT | FLOAT_16BIT | FLOAT_32BIT | FLOAT_64BIT
    )
}

#[inline]
fn binary_array_has_type(binary_array: &BinaryDataArray, array_type: u32) -> bool {
    binary_array
        .cv_params
        .iter()
        .any(|param| parse_accession_tail(param.accession.as_deref()).raw() == array_type)
}

fn parse_run_source_file_refs(
    owner_rows: &OwnerRows,
    lookup: &ChildrenLookup,
    run_id: u32,
) -> Option<SourceFileRefList> {
    if let Some(&list_id) = lookup.ids_for(run_id, TagId::SourceFileRefList).first() {
        let refs: Vec<_> = lookup
            .ids_for(list_id, TagId::SourceFileRef)
            .iter()
            .filter_map(|&id| {
                get_attr_text(owner_rows.get(id), ACC_ATTR_REF)
                    .map(|value| SourceFileRef { r#ref: value })
            })
            .collect();
        if !refs.is_empty() {
            return Some(SourceFileRefList {
                count: Some(refs.len()),
                source_file_refs: refs,
            });
        }
    }

    if let Some(&list_id) = lookup.ids_for(run_id, TagId::SourceFileList).first() {
        let refs: Vec<_> = lookup
            .ids_for(list_id, TagId::SourceFile)
            .iter()
            .filter_map(|&id| {
                get_attr_text(owner_rows.get(id), ACC_ATTR_ID)
                    .map(|value| SourceFileRef { r#ref: value })
            })
            .collect();
        if !refs.is_empty() {
            return Some(SourceFileRefList {
                count: Some(refs.len()),
                source_file_refs: refs,
            });
        }
    }

    None
}

impl IonReader {
    pub fn metadata(&self) -> IonResult<MzML> {
        MzmlConverter::metadata_only(self)
    }

    pub fn to_mzml(&mut self) -> IonResult<MzML> {
        MzmlConverter::new(self).full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_i16_arrays_get_real_cvparam_name_6() {
        let mut float16_array = BinaryDataArray::default();
        sync_numeric_meta(&mut float16_array, NumericType::Float16);
        let float16_param = float16_array
            .cv_params
            .iter()
            .find(|p| p.accession.as_deref() == Some("MS:1000520"))
            .expect("sync_numeric_meta must add the 16-bit float precision cvParam");
        assert_eq!(float16_param.name, "16-bit float");

        let mut int16_array = BinaryDataArray::default();
        sync_numeric_meta(&mut int16_array, NumericType::Int16);
        let int16_param = int16_array
            .cv_params
            .iter()
            .find(|p| p.accession.as_deref() == Some("MS:1000518"))
            .expect("sync_numeric_meta must add the 16-bit integer precision cvParam");
        assert_eq!(int16_param.name, "16-bit integer");
    }
}
