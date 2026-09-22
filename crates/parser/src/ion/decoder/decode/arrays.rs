use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ArrayAddress {
    pub(crate) block_id: u32,
    pub(crate) element_offset: u64,
    pub(crate) element_count: u64,
    pub(crate) array_type: u32,
    pub(crate) dtype: u8,
    pub(crate) array_filter: u8,
    pub(crate) encoded_len: u32,
    pub(crate) continues_previous_segment: u8,
    pub(crate) array_cv_code: u8,
}

#[cfg(test)]
impl ArrayAddress {
    pub(crate) fn array_type(&self) -> u32 {
        self.array_type
    }

    pub(crate) fn dtype(&self) -> u8 {
        self.dtype
    }

    pub(crate) fn array_filter(&self) -> u8 {
        self.array_filter
    }
}

#[derive(Debug, Clone)]
pub struct ArrayGroup {
    pub array_type: u32,
    pub array_cv_code: u8,
    pub dtype: u8,
    pub array_filter: u8,
    pub refs: Vec<ArrayAddress>,
}

#[derive(Clone)]
pub(crate) struct ArrayAddressList {
    pub(crate) len: usize,
    pub(crate) inline: [ArrayAddress; INLINE_ARRAY_ADDRESS_CAP],
    pub(crate) heap: Option<Vec<ArrayAddress>>,
}

impl ArrayAddressList {
    #[inline]
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            len: 0,
            inline: [ArrayAddress::default(); INLINE_ARRAY_ADDRESS_CAP],
            heap: (capacity > INLINE_ARRAY_ADDRESS_CAP).then(|| Vec::with_capacity(capacity)),
        }
    }

    #[inline]
    pub(crate) fn push(&mut self, value: ArrayAddress) {
        if let Some(heap) = self.heap.as_mut() {
            heap.push(value);
            self.len = heap.len();
            return;
        }
        self.inline[self.len] = value;
        self.len += 1;
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub(crate) fn as_slice(&self) -> &[ArrayAddress] {
        match self.heap.as_deref() {
            Some(heap) => heap,
            None => &self.inline[..self.len],
        }
    }

    #[inline]
    pub(crate) fn into_vec(self) -> Vec<ArrayAddress> {
        self.heap
            .unwrap_or_else(|| self.inline[..self.len].to_vec())
    }
}

#[inline]
pub(crate) fn address_read_params(array_address: &ArrayAddress) -> (u64, u64, usize) {
    if array_address.encoded_len > 0 {
        (
            array_address.element_offset,
            array_address.encoded_len as u64,
            1,
        )
    } else {
        (
            array_address.element_offset,
            array_address.element_count,
            dtype_stride(array_address.dtype),
        )
    }
}

#[inline]
pub(crate) fn parse_array_address(bytes: &[u8]) -> ArrayAddress {
    ArrayAddress {
        element_offset: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
        element_count: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        block_id: u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        array_type: u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
        dtype: bytes[24],
        array_filter: bytes[25],
        encoded_len: u32::from_le_bytes(bytes[26..30].try_into().unwrap()),
        continues_previous_segment: bytes[30],
        array_cv_code: bytes[31],
    }
}

#[inline]
pub(crate) fn read_array_addresses_from_buffers(
    entries_buf: &[u8],
    array_addresses_buf: &[u8],
    index: usize,
) -> Option<ArrayAddressList> {
    let entry_offset = index.checked_mul(INDEX_ENTRY_BYTES)?;
    let entry_end = entry_offset.checked_add(INDEX_ENTRY_BYTES)?;
    let entry = entries_buf.get(entry_offset..entry_end)?;
    let ref_start = usize::try_from(u64::from_le_bytes(entry[0..8].try_into().unwrap())).ok()?;
    let address_count =
        usize::try_from(u64::from_le_bytes(entry[8..16].try_into().unwrap())).ok()?;
    let max_refs = array_addresses_buf.len() / ARRAY_ADDRESS_BYTES;
    if address_count > max_refs {
        return None;
    }
    let mut refs = ArrayAddressList::with_capacity(address_count);
    for offset in 0..address_count {
        let pos = ref_start
            .checked_add(offset)?
            .checked_mul(ARRAY_ADDRESS_BYTES)?;
        let end = pos.checked_add(ARRAY_ADDRESS_BYTES)?;
        refs.push(parse_array_address(array_addresses_buf.get(pos..end)?));
    }
    Some(refs)
}

pub(crate) fn group_arrays(refs: &[ArrayAddress]) -> IonResult<Vec<ArrayGroup>> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }

    if refs[0].continues_previous_segment != 0 {
        return Err("array grouping: first ref must have continues_previous_segment = 0".into());
    }

    let mut groups = Vec::new();
    let mut current_group_refs = Vec::new();
    let mut current_type = refs[0].array_type;
    let mut current_cv_code = refs[0].array_cv_code;
    let mut current_dtype = refs[0].dtype;
    let mut current_filter = refs[0].array_filter;

    for address in refs {
        if address.continues_previous_segment != 0 && address.continues_previous_segment != 1 {
            return Err(format!(
                "array grouping: invalid continues_previous_segment value {}, must be 0 or 1",
                address.continues_previous_segment
            )
            .into());
        }

        if address.continues_previous_segment == 0 {
            if !current_group_refs.is_empty() {
                groups.push(ArrayGroup {
                    array_type: current_type,
                    array_cv_code: current_cv_code,
                    dtype: current_dtype,
                    array_filter: current_filter,
                    refs: current_group_refs,
                });
                current_group_refs = Vec::new();
            }
            current_type = address.array_type;
            current_cv_code = address.array_cv_code;
            current_dtype = address.dtype;
            current_filter = address.array_filter;
        } else if address.array_type != current_type
            || address.dtype != current_dtype
            || address.array_filter != current_filter
        {
            return Err(
                "array grouping: continuation ref has different array_type, dtype, or filter"
                    .into(),
            );
        }

        current_group_refs.push(*address);
    }

    if !current_group_refs.is_empty() {
        groups.push(ArrayGroup {
            array_type: current_type,
            array_cv_code: current_cv_code,
            dtype: current_dtype,
            array_filter: current_filter,
            refs: current_group_refs,
        });
    }

    for group in &groups {
        if group.refs.len() > 1 {
            for address in &group.refs {
                if address.encoded_len > 0 {
                    return Err(
                        "array grouping: multi-ref group cannot contain variable-length arrays"
                            .into(),
                    );
                }
            }
        }
    }

    Ok(groups)
}

pub(crate) fn read_group_decoded_bytes(
    group: &ArrayGroup,
    container: &mut BlockReader<DefaultBlockProcessor>,
) -> IonResult<Vec<u8>> {
    if let [array_address] = group.refs.as_slice() {
        let (element_offset, count, stride) = address_read_params(array_address);
        let raw = container.get_array_bytes_from_block(
            array_address.block_id,
            element_offset,
            count,
            stride,
            "read_group_decoded_bytes",
        )?;
        let unfiltered = unfilter_array_bytes(raw, group.dtype, group.array_filter)?;
        return Ok(unfiltered.into_owned());
    }

    let mut decoded = Vec::new();
    for array_address in &group.refs {
        let (element_offset, count, stride) = address_read_params(array_address);
        let raw = container.get_array_bytes_from_block(
            array_address.block_id,
            element_offset,
            count,
            stride,
            "read_group_decoded_bytes",
        )?;
        let unfiltered = unfilter_array_bytes(raw, group.dtype, group.array_filter)?;
        decoded.reserve(unfiltered.len());
        decoded.extend_from_slice(&unfiltered);
    }

    Ok(decoded)
}

#[inline]
pub(crate) fn dtype_stride(dtype: u8) -> usize {
    match dtype {
        FILE_DTYPE_F64 | FILE_DTYPE_I64 => 8,
        FILE_DTYPE_F32 | FILE_DTYPE_I32 => 4,
        FILE_DTYPE_F16 | FILE_DTYPE_I16 => 2,
        _ => 1,
    }
}

pub(crate) fn unfilter_array_bytes(
    raw: &[u8],
    dtype: u8,
    array_filter: u8,
) -> IonResult<std::borrow::Cow<'_, [u8]>> {
    let pk_id = PackingId::from_byte(array_filter)?;
    match pk_id {
        PackingId::Raw => Ok(std::borrow::Cow::Borrowed(raw)),
        PackingId::ByteShuffle => match crate::ion::packing::Dtype::from_byte(dtype) {
            Ok(
                dtype_enum @ (crate::ion::packing::Dtype::F64 | crate::ion::packing::Dtype::F32),
            ) => {
                let mut out = Vec::new();
                crate::ion::packing::packing_by_id(PackingId::ByteShuffle)
                    .decode(raw, dtype_enum, &mut out)?;
                Ok(std::borrow::Cow::Owned(out))
            }
            _ => Err(format!("array filter {array_filter} is not valid for dtype {dtype}").into()),
        },
        PackingId::DeltaShuffle => match crate::ion::packing::Dtype::from_byte(dtype) {
            Ok(
                dtype_enum @ (crate::ion::packing::Dtype::F64 | crate::ion::packing::Dtype::F32),
            ) => {
                let mut out = Vec::new();
                crate::ion::packing::packing_by_id(PackingId::DeltaShuffle)
                    .decode(raw, dtype_enum, &mut out)?;
                Ok(std::borrow::Cow::Owned(out))
            }
            _ => Err(format!("array filter {array_filter} is not valid for dtype {dtype}").into()),
        },
    }
}

pub(crate) fn decode_into(
    buf: &mut Vec<f64>,
    raw: &[u8],
    dtype: u8,
    array_filter: u8,
) -> IonResult<()> {
    buf.clear();
    let bytes = unfilter_array_bytes(raw, dtype, array_filter)?;
    match dtype {
        FILE_DTYPE_F64 => {
            buf.reserve(bytes.len() / 8);
            buf.extend(
                bytes
                    .chunks_exact(8)
                    .map(|c| f64::from_le_bytes(c.try_into().unwrap())),
            );
        }
        FILE_DTYPE_F32 => {
            buf.reserve(bytes.len() / 4);
            buf.extend(
                bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes(c.try_into().unwrap()) as f64),
            );
        }
        FILE_DTYPE_F16 => {
            buf.reserve(bytes.len() / 2);
            buf.extend(
                bytes
                    .chunks_exact(2)
                    .map(|c| f16_bits_to_f64(u16::from_le_bytes(c.try_into().unwrap()))),
            );
        }
        FILE_DTYPE_I16 => {
            buf.reserve(bytes.len() / 2);
            buf.extend(
                bytes
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes(c.try_into().unwrap()) as f64),
            );
        }
        FILE_DTYPE_I32 => {
            buf.reserve(bytes.len() / 4);
            buf.extend(
                bytes
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes(c.try_into().unwrap()) as f64),
            );
        }
        FILE_DTYPE_I64 => {
            buf.reserve(bytes.len() / 8);
            buf.extend(
                bytes
                    .chunks_exact(8)
                    .map(|c| i64::from_le_bytes(c.try_into().unwrap()) as f64),
            );
        }
        _ => {
            return Err(IonError::BadDtype {
                dtype,
                kind: "decode array dtype",
            });
        }
    }
    Ok(())
}

#[cfg(test)]
fn collect_entry_array_addresses(
    entry_bytes: &[u8],
    address_bytes: &[u8],
) -> Option<Vec<ArrayAddress>> {
    let ref_start =
        usize::try_from(u64::from_le_bytes(entry_bytes[0..8].try_into().unwrap())).ok()?;
    let address_count =
        usize::try_from(u64::from_le_bytes(entry_bytes[8..16].try_into().unwrap())).ok()?;
    let start = ref_start.checked_mul(ARRAY_ADDRESS_BYTES)?;
    let span = address_count.checked_mul(ARRAY_ADDRESS_BYTES)?;
    let end = start.checked_add(span)?;
    let mut refs = Vec::with_capacity(address_count);
    for bytes in address_bytes
        .get(start..end)?
        .chunks_exact(ARRAY_ADDRESS_BYTES)
    {
        refs.push(parse_array_address(bytes));
    }
    Some(refs)
}

#[cfg(test)]
pub(crate) fn read_scan_arrays(
    container: &mut dyn ContainerAccess,
    entry_bytes: &[u8],
    address_bytes: &[u8],
    mz: &mut Vec<f64>,
    intensity: &mut Vec<f64>,
) -> bool {
    mz.clear();
    intensity.clear();
    let Some(refs) = collect_entry_array_addresses(entry_bytes, address_bytes) else {
        return false;
    };
    let Ok(groups) = group_arrays(&refs) else {
        return false;
    };
    let mut segment = Vec::new();
    for group in &groups {
        let target = match group.array_type {
            ACC_MZ => &mut *mz,
            ACC_INT => &mut *intensity,
            _ => continue,
        };
        for array_address in &group.refs {
            let (element_offset, count, stride) = address_read_params(array_address);
            let raw = match container.get_array_bytes_from_block(
                array_address.block_id,
                element_offset,
                count,
                stride,
                "scan",
            ) {
                Ok(raw) => raw,
                Err(_) => return false,
            };
            if decode_into(
                &mut segment,
                raw,
                array_address.dtype,
                array_address.array_filter,
            )
            .is_err()
            {
                return false;
            }
            target.extend_from_slice(&segment);
        }
    }
    mz.len().min(intensity.len()) > 0
}

impl IonReader {
    #[allow(private_interfaces)]
    pub fn spectrum_array_addresses(&self, index: usize) -> Option<Vec<ArrayAddress>> {
        if index >= self.header.spectrum_count as usize {
            return None;
        }
        read_array_addresses_from_buffers(&self.spec_entries_buf, &self.spec_array_addresses, index)
            .map(ArrayAddressList::into_vec)
    }

    #[allow(private_interfaces)]
    pub fn chromatogram_array_addresses(&self, index: usize) -> Option<Vec<ArrayAddress>> {
        if index >= self.header.chrom_count as usize {
            return None;
        }
        read_array_addresses_from_buffers(
            &self.chrom_entries_buf,
            &self.chrom_array_addresses,
            index,
        )
        .map(ArrayAddressList::into_vec)
    }

    #[allow(private_interfaces)]
    pub fn read_spectrum_values(
        &mut self,
        array_address: &ArrayAddress,
        out: &mut Vec<f64>,
    ) -> IonResult<()> {
        let (element_offset, count, stride) = address_read_params(array_address);
        let raw = self.spec_container.get_array_bytes_from_block(
            array_address.block_id,
            element_offset,
            count,
            stride,
            "read_array",
        )?;
        decode_into(out, raw, array_address.dtype, array_address.array_filter)
    }

    #[allow(private_interfaces)]
    pub fn read_spectrum_array(&mut self, array_address: &ArrayAddress) -> IonResult<NumericArray> {
        let (element_offset, count, stride) = address_read_params(array_address);
        let raw = self.spec_container.get_array_bytes_from_block(
            array_address.block_id,
            element_offset,
            count,
            stride,
            "read_spectrum_array",
        )?;
        let values = unfilter_array_bytes(raw, array_address.dtype, array_address.array_filter)?;
        super::to_mzml::decoded_bytes_to_binary_data(&values, array_address.dtype)
    }

    #[allow(private_interfaces)]
    pub fn read_chromatogram_values(
        &mut self,
        array_address: &ArrayAddress,
        out: &mut Vec<f64>,
    ) -> IonResult<()> {
        let container = self
            .chrom_container
            .as_mut()
            .ok_or_else(|| IonError::from("no chromatogram container"))?;
        let (element_offset, count, stride) = address_read_params(array_address);
        let raw = container.get_array_bytes_from_block(
            array_address.block_id,
            element_offset,
            count,
            stride,
            "read_chromatogram_values",
        )?;
        decode_into(out, raw, array_address.dtype, array_address.array_filter)
    }

    #[allow(private_interfaces)]
    pub fn read_chromatogram_array(
        &mut self,
        array_address: &ArrayAddress,
    ) -> IonResult<NumericArray> {
        let container = self
            .chrom_container
            .as_mut()
            .ok_or_else(|| IonError::from("no chromatogram container"))?;
        let (element_offset, count, stride) = address_read_params(array_address);
        let raw = container.get_array_bytes_from_block(
            array_address.block_id,
            element_offset,
            count,
            stride,
            "read_chromatogram_array",
        )?;
        let values = unfilter_array_bytes(raw, array_address.dtype, array_address.array_filter)?;
        super::to_mzml::decoded_bytes_to_binary_data(&values, array_address.dtype)
    }

    pub(crate) fn read_group_values(
        &mut self,
        group: &ArrayGroup,
        out: &mut Vec<f64>,
    ) -> IonResult<()> {
        let Some((first, rest)) = group.refs.split_first() else {
            out.clear();
            return Ok(());
        };
        self.read_spectrum_values(first, out)?;
        let mut window = Vec::new();
        for array_address in rest {
            self.read_spectrum_values(array_address, &mut window)?;
            out.extend_from_slice(&window);
        }
        Ok(())
    }

    pub fn array(&mut self, spectrum_index: usize, array_type: impl Into<u32>) -> IonResult<Vec<f64>> {
        let mut values = Vec::new();
        self.array_into(spectrum_index, array_type, &mut values)?;
        Ok(values)
    }

    pub fn array_into(
        &mut self,
        spectrum_index: usize,
        array_type: impl Into<u32>,
        out: &mut Vec<f64>,
    ) -> IonResult<()> {
        out.clear();
        let array_type = array_type.into();
        if spectrum_index >= self.header.spectrum_count as usize {
            return Err(IonError::OutOfRange {
                index: spectrum_index,
                count: self.header.spectrum_count,
            });
        }

        let Some(array_addresses) = read_array_addresses_from_buffers(
            &self.spec_entries_buf,
            &self.spec_array_addresses,
            spectrum_index,
        ) else {
            return Ok(());
        };

        let groups = group_arrays(array_addresses.as_slice())?;

        for group in groups {
            if group.array_type != array_type {
                continue;
            }
            self.read_group_values(&group, out)?;
            return Ok(());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfilter_array_bytes_delta_shuffle_f64_matches_cumulative_sum() {
        let deltas: [u64; 4] = [100u64, 5, 3, 10];
        let raw: Vec<u8> = deltas.iter().flat_map(|w| w.to_le_bytes()).collect();

        let unfiltered =
            unfilter_array_bytes(&raw, FILE_DTYPE_F64, PackingId::DeltaShuffle as u8).unwrap();

        let mut expected_prev: u64 = 0;
        let mut expected = Vec::new();
        for delta in deltas {
            expected_prev = expected_prev.wrapping_add(delta);
            expected.extend_from_slice(&expected_prev.to_le_bytes());
        }

        assert_eq!(unfiltered.as_ref(), expected.as_slice());
    }

    #[test]
    fn unfilter_array_bytes_delta_shuffle_rejects_misaligned_f64_input() {
        let raw = [0u8; 7];
        let err = unfilter_array_bytes(&raw, FILE_DTYPE_F64, PackingId::DeltaShuffle as u8)
            .expect_err("7 bytes is not a multiple of the f64 word size");
        assert!(err.contains("not a multiple of the word size"));
    }

    #[test]
    fn unfilter_array_bytes_byte_shuffle_f64_round_trips_71() {
        use crate::ion::packing::{PackingInput, packing_by_id};

        let data: Vec<f64> = (0..600)
            .map(|i| 250.0 + (i as f64) * 0.0137 + ((i * 5 % 11) as f64) * 0.0011)
            .collect();
        let raw: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();

        let mut shuffled = Vec::new();
        packing_by_id(PackingId::ByteShuffle)
            .encode(PackingInput::F64(&data), &mut shuffled)
            .unwrap();

        assert_ne!(
            shuffled, raw,
            "shuffled layout must differ from raw for varying data"
        );

        let unfiltered =
            unfilter_array_bytes(&shuffled, FILE_DTYPE_F64, PackingId::ByteShuffle as u8).unwrap();

        assert_eq!(
            unfiltered.as_ref(),
            raw.as_slice(),
            "decode of ByteShuffle-tagged bytes must unshuffle back to raw"
        );

        let out: Vec<f64> = unfiltered
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(out.len(), data.len());
        for (a, b) in out.iter().zip(data.iter()) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn unfilter_array_bytes_delta_shuffle_rejects_unsupported_dtype() {
        let raw = [1u8, 2, 3, 4];
        let err = unfilter_array_bytes(&raw, FILE_DTYPE_I16, PackingId::DeltaShuffle as u8)
            .expect_err("delta shuffle is only valid for f32/f64");
        assert!(err.contains("is not valid for dtype"));
    }

    #[test]
    fn shuffle_filter_on_integer_dtype_is_rejected_4() {
        let raw = [0u8; 8];

        let err = unfilter_array_bytes(&raw, FILE_DTYPE_I32, PackingId::ByteShuffle as u8)
            .expect_err("byte shuffle is only valid for f32/f64, not i32");
        assert!(err.contains("is not valid for dtype"));

        let ok = unfilter_array_bytes(&raw, FILE_DTYPE_F64, PackingId::Raw as u8)
            .expect("raw filter must still work for any dtype");
        assert_eq!(ok.as_ref(), raw.as_slice());
    }

    mod array_roundtrips {
        use crate::{
            ArrayKind, IonReader, Range, ReadOptions, ScanQuery, Select, WriteOptions,
            format::{CURRENT_VERSION, HEADER_FORMAT_VERSION_OFFSET},
            ion::encoder::ion_writer::write_mzml_to_ion,
            mzml::structs::{
                BinaryDataArray, BinaryDataArrayList, Chromatogram, CvParam, FileContent,
                FileDescription, MzML, NumericArray, NumericType, Run, Scan, ScanList,
                SourceFileList, Spectrum, SpectrumList,
            },
        };
        use proptest::prelude::*;

        fn synthetic_ms_cv(accession: &str, value: Option<&str>) -> CvParam {
            CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(accession.to_string()),
                name: accession.to_string(),
                value: value.map(ToString::to_string),
                ..Default::default()
            }
        }

        fn precision_accession(numeric_type: NumericType) -> &'static str {
            match numeric_type {
                NumericType::Float64 => "MS:1000523",
                NumericType::Float32 => "MS:1000521",
                NumericType::Float16 => "MS:1000520",
                NumericType::Int64 => "MS:1000522",
                NumericType::Int32 => "MS:1000519",
                NumericType::Int16 => "MS:1000518",
            }
        }

        fn synthetic_binary_data_array(
            role_accession: &str,
            numeric_type: NumericType,
            binary: NumericArray,
            declared_length: Option<usize>,
        ) -> BinaryDataArray {
            BinaryDataArray {
                array_length: declared_length,
                cv_params: vec![
                    synthetic_ms_cv(role_accession, None),
                    synthetic_ms_cv(precision_accession(numeric_type), None),
                    synthetic_ms_cv("MS:1000576", None),
                ],
                numeric_type: Some(numeric_type),
                binary: Some(binary),
                ..Default::default()
            }
        }

        fn minimal_file_description() -> FileDescription {
            FileDescription {
                file_content: FileContent::default(),
                source_file_list: SourceFileList {
                    count: Some(0),
                    source_file: Vec::new(),
                },
                contacts: Vec::new(),
            }
        }

        fn mzml_with_single_array(
            numeric_type: NumericType,
            binary: NumericArray,
            len: usize,
        ) -> MzML {
            MzML {
                file_description: Some(minimal_file_description()),
                run: Run {
                    id: format!("array-test-{numeric_type:?}"),
                    spectrum_list: Some(SpectrumList {
                        count: Some(1),
                        spectra: vec![Spectrum {
                            id: "scan=1".to_string(),
                            index: Some(0),
                            default_array_length: Some(len),
                            binary_data_array_list: Some(BinaryDataArrayList {
                                count: Some(1),
                                binary_data_arrays: vec![synthetic_binary_data_array(
                                    "MS:1000515",
                                    numeric_type,
                                    binary,
                                    Some(len),
                                )],
                            }),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            }
        }

        fn build_mzml(spectra: Vec<Spectrum>, chromatograms: Vec<Chromatogram>) -> MzML {
            MzML {
                file_description: Some(minimal_file_description()),
                run: Run {
                    id: "test-run".to_string(),
                    spectrum_list: if spectra.is_empty() {
                        None
                    } else {
                        Some(SpectrumList {
                            count: Some(spectra.len()),
                            spectra,
                            ..Default::default()
                        })
                    },
                    chromatogram_list: if chromatograms.is_empty() {
                        None
                    } else {
                        Some(crate::mzml::structs::ChromatogramList {
                            count: Some(chromatograms.len()),
                            chromatograms,
                            ..Default::default()
                        })
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
        }

        fn make_spectrum_f64(id: &str, mz: Vec<f64>, intensity: Vec<f64>) -> Spectrum {
            let len = mz.len();
            Spectrum {
                id: id.to_string(),
                index: Some(0),
                default_array_length: Some(len),
                binary_data_array_list: Some(BinaryDataArrayList {
                    count: Some(2),
                    binary_data_arrays: vec![
                        synthetic_binary_data_array(
                            "MS:1000514",
                            NumericType::Float64,
                            NumericArray::F64(mz),
                            Some(len),
                        ),
                        synthetic_binary_data_array(
                            "MS:1000515",
                            NumericType::Float64,
                            NumericArray::F64(intensity),
                            Some(len),
                        ),
                    ],
                }),
                ..Default::default()
            }
        }

        fn make_chromatogram_f64(id: &str, time: Vec<f64>, intensity: Vec<f64>) -> Chromatogram {
            let len = time.len();
            Chromatogram {
                id: id.to_string(),
                index: Some(0),
                default_array_length: Some(len),
                binary_data_array_list: Some(BinaryDataArrayList {
                    count: Some(2),
                    binary_data_arrays: vec![
                        synthetic_binary_data_array(
                            "MS:1000595",
                            NumericType::Float64,
                            NumericArray::F64(time),
                            Some(len),
                        ),
                        synthetic_binary_data_array(
                            "MS:1000515",
                            NumericType::Float64,
                            NumericArray::F64(intensity),
                            Some(len),
                        ),
                    ],
                }),
                ..Default::default()
            }
        }

        fn encode_to_ion(mzml: &MzML, compression_level: u8, force_f32: bool) -> Vec<u8> {
            let mut out = Vec::new();
            write_mzml_to_ion(
                mzml,
                &WriteOptions {
                    compression_level,
                    force_f32,
                    ..Default::default()
                },
                &mut out,
            )
            .expect("encode should succeed");
            out
        }

        fn decode_ion(bytes: &[u8]) -> crate::IonResult<MzML> {
            let mut decoder = IonReader::from_bytes(bytes, &ReadOptions::default())?;
            decoder.to_mzml()
        }

        fn roundtrip(mzml: &MzML) -> MzML {
            decode_ion(&encode_to_ion(mzml, 0, false)).expect("decode should succeed")
        }

        fn first_spectrum_binary(mzml: &MzML) -> Option<&NumericArray> {
            mzml.run
                .spectrum_list
                .as_ref()?
                .spectra
                .first()?
                .binary_data_array_list
                .as_ref()?
                .binary_data_arrays
                .first()?
                .binary
                .as_ref()
        }

        fn first_chrom_array_values_by_accession(c: &Chromatogram, accession: &str) -> Vec<f64> {
            let bda = c
                .binary_data_array_list
                .as_ref()
                .expect("binary data array list present")
                .binary_data_arrays
                .iter()
                .find(|a| {
                    a.cv_params
                        .iter()
                        .any(|p| p.accession.as_deref() == Some(accession))
                })
                .expect("array with accession present");
            bda.binary.as_ref().expect("binary payload present").to_f64()
        }

        #[test]
        fn roundtrip_f64_array() {
            let values = vec![1.0_f64, -2.5, 0.0, f64::MAX, f64::MIN, std::f64::consts::PI];
            let len = values.len();
            let mzml =
                mzml_with_single_array(NumericType::Float64, NumericArray::F64(values.clone()), len);
            let out = roundtrip(&mzml);
            let bin = first_spectrum_binary(&out).expect("should have binary data");
            let got = bin.to_f64();
            assert_eq!(got, values);
        }

        #[test]
        fn roundtrip_f32_array() {
            let values = vec![1.0_f32, -2.5, 0.0, f32::MAX, f32::MIN, std::f32::consts::PI];
            let len = values.len();
            let mzml =
                mzml_with_single_array(NumericType::Float32, NumericArray::F32(values.clone()), len);
            let out = roundtrip(&mzml);
            let bin = first_spectrum_binary(&out).expect("should have binary data");
            let got = bin.to_f64();
            let expected: Vec<f64> = values.iter().map(|v| *v as f64).collect();
            assert_eq!(got, expected);
        }

        #[test]
        fn array_filter_label_matches_applied_transform() {
            let raw_filter = 0u8;
            let f64_dtype = 1u8;
            let f32_dtype = 2u8;

            let f32_values = vec![10.0_f32, 11.0, 12.0, 13.0];
            let f32_len = f32_values.len();
            let f32_mzml =
                mzml_with_single_array(NumericType::Float32, NumericArray::F32(f32_values), f32_len);

            let f64_values = vec![100.0_f64, 100.5, 101.0, 101.5];
            let f64_len = f64_values.len();
            let f64_mzml =
                mzml_with_single_array(NumericType::Float64, NumericArray::F64(f64_values), f64_len);

            let f32_bytes = encode_to_ion(&f32_mzml, 9, false);
            let f32_decoder =
                IonReader::from_bytes(&f32_bytes, &ReadOptions::default()).expect("open f32 ion");
            let f32_refs = f32_decoder
                .spectrum_array_addresses(0)
                .expect("f32 array refs");

            let f64_bytes = encode_to_ion(&f64_mzml, 9, false);
            let f64_decoder =
                IonReader::from_bytes(&f64_bytes, &ReadOptions::default()).expect("open f64 ion");
            let f64_refs = f64_decoder
                .spectrum_array_addresses(0)
                .expect("f64 array refs");

            assert!(
                f32_refs
                    .iter()
                    .any(|a| a.dtype() == f32_dtype && a.array_filter() == raw_filter),
                "f32 intensity must be tagged raw"
            );
            assert!(
                f64_refs
                    .iter()
                    .any(|a| a.dtype() == f64_dtype && a.array_filter() == raw_filter),
                "f64 intensity must be tagged raw (intensity is never delta-shuffled)"
            );
        }

        #[test]
        fn roundtrip_i64_array() {
            let values = vec![0_i64, 1, -1, i64::MAX, i64::MIN, 42];
            let len = values.len();
            let mzml =
                mzml_with_single_array(NumericType::Int64, NumericArray::I64(values.clone()), len);
            let out = roundtrip(&mzml);
            let bin = first_spectrum_binary(&out).expect("should have binary data");
            match bin {
                NumericArray::I64(got) => assert_eq!(got, &values),
                other => {
                    let got = other.to_f64();
                    let expected: Vec<f64> = values.iter().map(|v| *v as f64).collect();
                    assert_eq!(
                        got, expected,
                        "i64 roundtrip values differ (via f64 conversion)"
                    );
                }
            }
        }

        #[test]
        fn roundtrip_i32_array() {
            let values = vec![0_i32, 1, -1, i32::MAX, i32::MIN, 42];
            let len = values.len();
            let mzml =
                mzml_with_single_array(NumericType::Int32, NumericArray::I32(values.clone()), len);
            let out = roundtrip(&mzml);
            let bin = first_spectrum_binary(&out).expect("should have binary data");
            match bin {
                NumericArray::I32(got) => assert_eq!(got, &values),
                other => {
                    let got = other.to_f64();
                    let expected: Vec<f64> = values.iter().map(|v| *v as f64).collect();
                    assert_eq!(got, expected, "i32 roundtrip values differ");
                }
            }
        }

        #[test]
        fn roundtrip_i16_array() {
            let values = vec![0_i16, 1, -1, i16::MAX, i16::MIN, 42];
            let len = values.len();
            let mzml =
                mzml_with_single_array(NumericType::Int16, NumericArray::I16(values.clone()), len);
            let out = roundtrip(&mzml);
            let bin = first_spectrum_binary(&out).expect("should have binary data");
            match bin {
                NumericArray::I16(got) => assert_eq!(got, &values),
                other => {
                    let got = other.to_f64();
                    let expected: Vec<f64> = values.iter().map(|v| *v as f64).collect();
                    assert_eq!(got, expected, "i16 roundtrip values differ");
                }
            }
        }

        #[test]
        fn roundtrip_single_element_per_type() {
            let cases: Vec<(NumericType, NumericArray)> = vec![
                (NumericType::Float64, NumericArray::F64(vec![42.0])),
                (NumericType::Float32, NumericArray::F32(vec![42.0])),
                (NumericType::Int64, NumericArray::I64(vec![42])),
                (NumericType::Int32, NumericArray::I32(vec![42])),
                (NumericType::Int16, NumericArray::I16(vec![42])),
            ];

            for (nt, bin) in cases {
                let mzml = mzml_with_single_array(nt, bin.clone(), 1);
                let out = roundtrip(&mzml);
                let got = first_spectrum_binary(&out).expect("should have binary data");
                let expected_f64 = bin.to_f64();
                let got_f64 = got.to_f64();
                assert_eq!(
                    got_f64, expected_f64,
                    "single-element roundtrip failed for {nt:?}"
                );
            }
        }

        #[test]
        fn roundtrip_at_compression_levels() {
            let values = vec![100.0_f64, 200.0, 300.0, 400.0, 500.0];
            let len = values.len();
            let mzml =
                mzml_with_single_array(NumericType::Float64, NumericArray::F64(values.clone()), len);

            for level in [0, 3, 10, 22] {
                let mut buf = Vec::new();
                write_mzml_to_ion(
                    &mzml,
                    &WriteOptions {
                        compression_level: level,
                        force_f32: false,
                        ..Default::default()
                    },
                    &mut buf,
                )
                .unwrap_or_else(|e| panic!("encode at level {level} failed: {e}"));
                let mut decoder = IonReader::from_bytes(&buf, &ReadOptions::default())
                    .unwrap_or_else(|e| panic!("decoder open at level {level} failed: {e}"));
                let decoded = decoder
                    .to_mzml()
                    .unwrap_or_else(|e| panic!("to_mzml at level {level} failed: {e}"));
                let got = first_spectrum_binary(&decoded).expect("should have binary data");
                assert_eq!(
                    got.to_f64(),
                    values,
                    "values differ at compression level {level}"
                );
            }
        }

        #[test]
        fn roundtrip_force_f32_downcasts() {
            let values = vec![100.0_f64, 200.0, 300.0];
            let len = values.len();
            let mzml =
                mzml_with_single_array(NumericType::Float64, NumericArray::F64(values.clone()), len);

            let mut buf = Vec::new();
            write_mzml_to_ion(
                &mzml,
                &WriteOptions {
                    compression_level: 0,
                    force_f32: true,
                    ..Default::default()
                },
                &mut buf,
            )
            .expect("encode with force_f32");
            let mut decoder =
                IonReader::from_bytes(&buf, &ReadOptions::default()).expect("decoder open");
            let decoded = decoder.to_mzml().expect("to_mzml");

            let bin = first_spectrum_binary(&decoded).expect("should have binary data");
            let got = bin.to_f64();
            for (i, (g, e)) in got.iter().zip(values.iter()).enumerate() {
                let expected_f32 = *e as f32 as f64;
                assert!(
                    (g - expected_f32).abs() < 1e-6,
                    "index {i}: force_f32 value mismatch: {g} vs {expected_f32}"
                );
            }
        }

        fn encode_decode_mz_compressed(mz: Vec<f64>) -> Vec<f64> {
            let len = mz.len();
            let mzml = mzml_with_single_array(NumericType::Float64, NumericArray::F64(mz), len);
            let buf = encode_to_ion(&mzml, 3, false);
            let decoded = decode_ion(&buf).unwrap();
            first_spectrum_binary(&decoded).unwrap().to_f64()
        }

        #[test]
        fn delta_mz_single_element_is_bit_exact() {
            let input = vec![503.42f64];
            let got = encode_decode_mz_compressed(input.clone());
            assert_eq!(got[0].to_bits(), input[0].to_bits());
        }

        #[test]
        fn delta_mz_two_elements_are_bit_exact() {
            let input = vec![100.0f64, 200.5];
            let got = encode_decode_mz_compressed(input.clone());
            for (a, b) in got.iter().zip(input.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }

        #[test]
        fn delta_mz_monotonic_array_is_bit_exact() {
            let input: Vec<f64> = (0..10_000).map(|i| 100.0 + i as f64 * 0.01).collect();
            let got = encode_decode_mz_compressed(input.clone());
            for (a, b) in got.iter().zip(input.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }

        #[test]
        fn delta_mz_special_values_are_bit_exact() {
            let input = vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0f64, 0.0f64];
            let got = encode_decode_mz_compressed(input.clone());
            for (a, b) in got.iter().zip(input.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }

        #[test]
        fn delta_mz_via_for_each_scan_is_bit_exact() {
            let input: Vec<f64> = (0..500).map(|i| 100.0 + i as f64 * 0.05).collect();
            let intensity: Vec<f64> = vec![1.0; input.len()];
            let mut spectrum = make_spectrum_f64("scan=1", input.clone(), intensity);
            spectrum.scan_list = Some(ScanList {
                count: Some(1),
                scans: vec![Scan {
                    cv_params: vec![CvParam {
                        cv_ref: Some("MS".to_string()),
                        accession: Some("MS:1000016".to_string()),
                        name: "scan start time".to_string(),
                        value: Some("1.0".to_string()),
                        unit_accession: Some("UO:0000031".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            });
            let mzml = build_mzml(vec![spectrum], vec![]);
            let buf = encode_to_ion(&mzml, 3, false);
            let mut ion = IonReader::from_bytes(&buf, &ReadOptions::default()).unwrap();
            let mut got_mz: Vec<f64> = Vec::new();
            let query = ScanQuery {
                mz: Range {
                    from: 0.0,
                    to: f64::MAX,
                },
                select: Select::All,
                ms_level: None,
            };
            ion.scans_in(&query, |window| {
                got_mz = window.mz.to_vec();
            })
            .unwrap();
            assert_eq!(got_mz.len(), input.len());
            for (a, b) in got_mz.iter().zip(input.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }

        #[test]
        fn delta_on_mz_raw_on_intensity() {
            let mz: Vec<f64> = (0..100).map(|i| 100.0 + i as f64).collect();
            let intensity: Vec<f64> = (0..100).map(|i| (i * 10) as f64).collect();
            let mzml = build_mzml(
                vec![make_spectrum_f64("scan=1", mz.clone(), intensity.clone())],
                vec![],
            );
            let buf = encode_to_ion(&mzml, 3, false);
            let mut decoder = IonReader::from_bytes(&buf, &ReadOptions::default()).unwrap();
            let refs = decoder.spectrum_array_addresses(0).unwrap();
            let mz_address = refs.iter().find(|r| r.array_type() == 1_000_514).unwrap();
            let int_ref = refs.iter().find(|r| r.array_type() == 1_000_515).unwrap();
            assert_eq!(
                mz_address.array_filter(),
                2,
                "m/z must use DeltaShuffle filter"
            );
            assert_eq!(
                int_ref.array_filter(),
                0,
                "intensity must use raw filter (intensity is never delta-shuffled)"
            );
            let mut got_mz = Vec::new();
            decoder
                .read_spectrum_values(mz_address, &mut got_mz)
                .unwrap();
            let mut got_int = Vec::new();
            decoder.read_spectrum_values(int_ref, &mut got_int).unwrap();
            for (a, b) in got_mz.iter().zip(mz.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
            for (a, b) in got_int.iter().zip(intensity.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }

        #[test]
        fn format_version_always_matches_current() {
            let values = vec![1.0_f64, 2.0, 3.0, 4.0, 5.0];
            let len = values.len();
            let mzml =
                mzml_with_single_array(NumericType::Float64, NumericArray::F64(values.clone()), len);
            for level in [0u8, 3, 22] {
                let buf = encode_to_ion(&mzml, level, false);
                let format_version = u16::from_le_bytes(
                    buf[HEADER_FORMAT_VERSION_OFFSET..HEADER_FORMAT_VERSION_OFFSET + 2]
                        .try_into()
                        .unwrap(),
                );
                assert_eq!(
                    format_version, CURRENT_VERSION,
                    "format_version must match CURRENT_VERSION at compression level {level}"
                );
            }
            let out = roundtrip(&mzml);
            let bin = first_spectrum_binary(&out).expect("binary data");
            for (a, b) in bin.to_f64().iter().zip(values.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }

        #[test]
        fn delta_not_applied_without_compression() {
            let mz: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();
            let intensity = vec![1.0_f64; mz.len()];
            let mzml = build_mzml(
                vec![make_spectrum_f64("scan=1", mz.clone(), intensity)],
                vec![],
            );
            let buf = encode_to_ion(&mzml, 0, false);
            let mut decoder = IonReader::from_bytes(&buf, &ReadOptions::default()).unwrap();
            let refs = decoder.spectrum_array_addresses(0).unwrap();
            let mz_address = refs.iter().find(|r| r.array_type() == 1_000_514).unwrap();
            assert_eq!(mz_address.array_filter(), 0, "no delta without compression");
            let mut got = Vec::new();
            decoder.read_spectrum_values(mz_address, &mut got).unwrap();
            for (a, b) in got.iter().zip(mz.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }

        #[test]
        fn delta_shuffle_applied_to_time_array() {
            let time: Vec<f64> = (0..100).map(|i| i as f64 * 0.1).collect();
            let intensity: Vec<f64> = vec![1.0; time.len()];
            let mzml = build_mzml(
                vec![],
                vec![make_chromatogram_f64("tic", time.clone(), intensity)],
            );
            let buf = encode_to_ion(&mzml, 3, false);
            let mut decoder = IonReader::from_bytes(&buf, &ReadOptions::default()).unwrap();
            let refs = decoder.chromatogram_array_addresses(0).unwrap();
            let time_ref = refs.iter().find(|r| r.array_type() == 1_000_595).unwrap();
            assert_eq!(
                time_ref.array_filter(),
                2,
                "time must use DeltaShuffle filter"
            );
            let mut got = Vec::new();
            decoder
                .read_chromatogram_values(time_ref, &mut got)
                .unwrap();
            for (a, b) in got.iter().zip(time.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }

        #[test]
        fn read_chromatogram_array_keeps_native_f32_width_33() {
            let time: Vec<f64> = (0..64).map(|i| i as f64 * 0.5).collect();
            let intensity: Vec<f64> = (0..64).map(|i| i as f64).collect();
            let mzml = build_mzml(vec![], vec![make_chromatogram_f64("tic", time, intensity)]);
            let buf = encode_to_ion(&mzml, 3, true);
            let mut decoder = IonReader::from_bytes(&buf, &ReadOptions::default()).unwrap();
            let refs = decoder.chromatogram_array_addresses(0).unwrap();
            let intensity_ref = refs.iter().find(|r| r.array_type() == 1_000_515).unwrap();
            let native = decoder.read_chromatogram_array(intensity_ref).unwrap();
            assert!(
                matches!(native, NumericArray::F32(_)),
                "read_chromatogram_array must keep the stored native f32 width"
            );
            let mut widened = Vec::new();
            decoder
                .read_chromatogram_values(intensity_ref, &mut widened)
                .unwrap();
            assert_eq!(
                widened.len(),
                64,
                "read_chromatogram_values must still widen to f64"
            );
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(32))]

            #[test]
            fn proptest_arbitrary_f64_roundtrip(
                values in prop::collection::vec(prop::num::f64::ANY, 1..64)
            ) {
                let len = values.len();
                let mzml = mzml_with_single_array(
                    NumericType::Float64,
                    NumericArray::F64(values.clone()),
                    len,
                );
                let out = roundtrip(&mzml);
                let bin = first_spectrum_binary(&out).expect("should have binary data");
                let got = bin.to_f64();
                prop_assert_eq!(got.len(), values.len());
                for (i, (g, e)) in got.iter().zip(values.iter()).enumerate() {
                    if e.is_nan() {
                        prop_assert!(g.is_nan(), "index {}: expected NaN", i);
                    } else {
                        prop_assert_eq!(g.to_bits(), e.to_bits(), "index {}: bit mismatch", i);
                    }
                }
            }

            #[test]
            fn proptest_delta2vbyte_mz_roundtrip(
                mz in prop::collection::vec(0.0f64..100_000.0, 3..256)
            ) {
                let mut sorted_mz = mz.clone();
                sorted_mz.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let intensity = vec![1.0f64; sorted_mz.len()];
                let mzml = build_mzml(
                    vec![make_spectrum_f64("scan=1", sorted_mz.clone(), intensity)],
                    vec![],
                );
                let buf = encode_to_ion(&mzml, 3, false);
                let got = decode_ion(&buf).unwrap();
                let bin = first_spectrum_binary(&got).expect("should have binary data");
                let decoded = bin.to_f64();
                prop_assert_eq!(decoded.len(), sorted_mz.len());
                for (i, (g, e)) in decoded.iter().zip(sorted_mz.iter()).enumerate() {
                    if e.is_nan() {
                        prop_assert!(g.is_nan(), "index {}: expected NaN", i);
                    } else {
                        prop_assert_eq!(g.to_bits(), e.to_bits(), "index {}: bit mismatch", i);
                    }
                }
            }

            #[test]
            fn proptest_chimp_time_roundtrip(
                time in prop::collection::vec(prop::num::f64::ANY, 2..256)
            ) {
                let intensity = vec![1.0f64; time.len()];
                let mzml = build_mzml(
                    vec![],
                    vec![make_chromatogram_f64("tic", time.clone(), intensity)],
                );
                let buf = encode_to_ion(&mzml, 3, false);
                let got = decode_ion(&buf).unwrap();
                let chrom_list = got.run.chromatogram_list.unwrap();
                let decoded = first_chrom_array_values_by_accession(&chrom_list.chromatograms[0], "MS:1000595");
                prop_assert_eq!(decoded.len(), time.len());
                for (i, (g, e)) in decoded.iter().zip(time.iter()).enumerate() {
                    if e.is_nan() {
                        prop_assert!(g.is_nan(), "index {}: expected NaN", i);
                    } else {
                        prop_assert_eq!(g.to_bits(), e.to_bits(), "index {}: bit mismatch", i);
                    }
                }
            }

            #[test]
            fn proptest_arbitrary_i32_roundtrip(
                values in prop::collection::vec(prop::num::i32::ANY, 1..64)
            ) {
                let len = values.len();
                let mzml = mzml_with_single_array(
                    NumericType::Int32,
                    NumericArray::I32(values.clone()),
                    len,
                );
                let out = roundtrip(&mzml);
                let bin = first_spectrum_binary(&out).expect("should have binary data");
                match bin {
                    NumericArray::I32(got) => prop_assert_eq!(got, &values),
                    other => {
                        let got = other.to_f64();
                        let expected: Vec<f64> = values.iter().map(|v| *v as f64).collect();
                        prop_assert_eq!(got, expected);
                    }
                }
            }

            #[test]
            fn proptest_arbitrary_i16_roundtrip(
                values in prop::collection::vec(prop::num::i16::ANY, 1..64)
            ) {
                let len = values.len();
                let mzml = mzml_with_single_array(
                    NumericType::Int16,
                    NumericArray::I16(values.clone()),
                    len,
                );
                let out = roundtrip(&mzml);
                let bin = first_spectrum_binary(&out).expect("should have binary data");
                match bin {
                    NumericArray::I16(got) => prop_assert_eq!(got, &values),
                    other => {
                        let got = other.to_f64();
                        let expected: Vec<f64> = values.iter().map(|v| *v as f64).collect();
                        prop_assert_eq!(got, expected);
                    }
                }
            }
        }
    }
}
