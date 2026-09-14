use crate::{
    accessions::{
        FLOAT_32BIT, FLOAT_64BIT, HIGHEST_ION_MOBILITY, HIGHEST_OBSERVED_MZ,
        HIGHEST_OBSERVED_WAVELENGTH, INTENSITY_ARRAY, LOWEST_ION_MOBILITY, LOWEST_OBSERVED_MZ,
        LOWEST_OBSERVED_WAVELENGTH, NEGATIVE_SCAN, POSITION_X, POSITION_Y, POSITION_Z,
        POSITIVE_SCAN,
    },
    ion::{
        IonError, IonResult,
        encoder::utilities::{
            BlockWriter, CompressionMode, DefaultCompressor,
            le_writers::{
                write_f32_le, write_f32_slice_le, write_f64_le, write_f64_slice_le,
                write_i16_slice_le, write_i32_slice_le, write_i64_slice_le, write_u16_slice_le,
            },
            meta_collector::{
                ArrayPolicy, array_type_accession_from_binary_data_array,
                array_type_cv_code_from_binary_data_array, parse_accession_tail_raw,
            },
            output::SectionStorage,
        },
        filter_summary::{ChromatogramSummary, SpectrumSummary},
        packing::{Dtype, Packing, PackingId, PackingInput, packing_for, raw::RAW as RAW_PACKING},
        utilities::spectrum_source::{f16_bits_to_f64, summary_from_spectrum},
        windowing::{WindowRange, Windowing},
    },
    mzml::structs::{BinaryDataArray, Chromatogram, CvParam, NumericArray, NumericType, Spectrum},
};
pub const TARGET_BLOCK_UNCOMPRESSED_BYTES: usize = 1024 * 1024;
pub const DEFAULT_MZ_WINDOW: f64 = 20.0;
pub(crate) const SPEC_SUMMARY_SIZE: usize = 80;
pub(crate) const CHROM_SUMMARY_SIZE: usize = 80;

pub(crate) const FILE_DTYPE_F64: u8 = Dtype::F64 as u8;
pub(crate) const FILE_DTYPE_F32: u8 = Dtype::F32 as u8;
pub(crate) const FILE_DTYPE_F16: u8 = Dtype::F16 as u8;
pub(crate) const FILE_DTYPE_I16: u8 = Dtype::I16 as u8;
pub(crate) const FILE_DTYPE_I32: u8 = Dtype::I32 as u8;
pub(crate) const FILE_DTYPE_I64: u8 = Dtype::I64 as u8;

const POLARITY_UNKNOWN: u8 = 0;
const POLARITY_POSITIVE: u8 = 1;
const POLARITY_NEGATIVE: u8 = 2;

impl ChromatogramSummary {
    fn unknown() -> Self {
        Self {
            lowest_mz: f64::NAN,
            highest_mz: f64::NAN,
            lowest_wavelength: f64::NAN,
            highest_wavelength: f64::NAN,
            lowest_ion_mobility: f64::NAN,
            highest_ion_mobility: f64::NAN,
            polarity: POLARITY_UNKNOWN,
        }
    }
}

pub(crate) fn spec_summary_from_spectrum(spectrum: &Spectrum) -> SpectrumSummary {
    let summary = summary_from_spectrum(spectrum);
    let (position_x, position_y, position_z) = get_spectrum_position(spectrum);
    SpectrumSummary {
        rt: summary.rt,
        rt_unit: summary.rt_unit.code(),
        base_peak_mz: summary.base_peak_mz,
        selected_ion_mz: summary.selected_ion_mz,
        base_peak_int: summary.base_peak_int,
        total_ion_current: summary.total_ion_current,
        ms_level: summary.ms_level,
        polarity: summary.polarity,
        position_x,
        position_y,
        position_z,
    }
}

fn get_spectrum_position(spectrum: &Spectrum) -> (u32, u32, u32) {
    let mut x = 0;
    let mut y = 0;
    let mut z = 0;
    let scan_list = spectrum.scan_list.as_ref().or_else(|| {
        spectrum
            .spectrum_description
            .as_ref()
            .and_then(|description| description.scan_list.as_ref())
    });
    let Some(scan_list) = scan_list else {
        return (x, y, z);
    };
    for scan in &scan_list.scans {
        for param in &scan.cv_params {
            let Some(accession) = param.accession.as_deref() else {
                continue;
            };
            if !accession.starts_with("IMS:") {
                continue;
            }
            let value = param
                .value
                .as_deref()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            match parse_accession_tail_raw(Some(accession)) {
                POSITION_X => x = value,
                POSITION_Y => y = value,
                POSITION_Z => z = value,
                _ => {}
            }
        }
    }
    (x, y, z)
}

#[inline]
fn cv_f64(cv: &CvParam) -> Option<f64> {
    cv.value.as_deref().and_then(|s| s.parse::<f64>().ok())
}

pub(crate) fn extract_chrom_summary(chrom: &Chromatogram) -> ChromatogramSummary {
    let mut summary = ChromatogramSummary::unknown();
    for cv in &chrom.cv_params {
        match parse_accession_tail_raw(cv.accession.as_deref()) {
            LOWEST_OBSERVED_MZ => {
                if let Some(v) = cv_f64(cv) {
                    summary.lowest_mz = v;
                }
            }
            HIGHEST_OBSERVED_MZ => {
                if let Some(v) = cv_f64(cv) {
                    summary.highest_mz = v;
                }
            }
            LOWEST_OBSERVED_WAVELENGTH => {
                if let Some(v) = cv_f64(cv) {
                    summary.lowest_wavelength = v;
                }
            }
            HIGHEST_OBSERVED_WAVELENGTH => {
                if let Some(v) = cv_f64(cv) {
                    summary.highest_wavelength = v;
                }
            }
            LOWEST_ION_MOBILITY => {
                if let Some(v) = cv_f64(cv) {
                    summary.lowest_ion_mobility = v;
                }
            }
            HIGHEST_ION_MOBILITY => {
                if let Some(v) = cv_f64(cv) {
                    summary.highest_ion_mobility = v;
                }
            }
            POSITIVE_SCAN => summary.polarity = POLARITY_POSITIVE,
            NEGATIVE_SCAN => summary.polarity = POLARITY_NEGATIVE,
            _ => {}
        }
    }
    summary
}

pub(crate) fn allow_compression_level(compression_level: u8) -> IonResult<()> {
    if compression_level > 22 {
        return Err(format!("compression_level must be 0-22, got {compression_level}").into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct WriteOptions {
    pub compression_level: u8,
    pub force_f32: bool,
    pub block_size: usize,
    pub parallel: bool,
    pub section_storage: SectionStorage,
    pub mz_window: f64,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            compression_level: 22,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: true,
            section_storage: SectionStorage::Disk,
            mz_window: DEFAULT_MZ_WINDOW,
        }
    }
}

impl WriteOptions {
    pub(crate) fn compression_is_enabled(self) -> bool {
        self.compression_level != 0
    }

    pub(crate) fn codec_id(self) -> u8 {
        self.compression_is_enabled() as u8
    }

    fn block_shuffle_is_enabled(self) -> bool {
        if !self.compression_is_enabled() {
            return false;
        }
        !matches!(
            std::env::var("IONIC_BLOCK_SHUFFLE").as_deref(),
            Ok("off") | Ok("raw") | Ok("0") | Ok("none")
        )
    }

    pub(crate) fn array_filter_id(self) -> u8 {
        if self.block_shuffle_is_enabled() {
            PackingId::ByteShuffle as u8
        } else {
            PackingId::Raw as u8
        }
    }

    pub(crate) fn compression_mode(self) -> IonResult<CompressionMode<DefaultCompressor>> {
        if self.compression_is_enabled() {
            Ok(CompressionMode::Compressed(DefaultCompressor::new(
                self.compression_level as i32,
            )?))
        } else {
            Ok(CompressionMode::Raw)
        }
    }

    pub(crate) fn block_packing_id(self) -> PackingId {
        if self.block_shuffle_is_enabled() {
            PackingId::ByteShuffle
        } else {
            PackingId::Raw
        }
    }

    pub(crate) fn array_policy(self, x_array_accession: u32) -> ArrayPolicy {
        ArrayPolicy {
            x_array_accession,
            y_array_accession: INTENSITY_ARRAY,
            force_f32: self.force_f32,
        }
    }

    pub fn fast() -> Self {
        Self {
            compression_level: 3,
            ..Default::default()
        }
    }

    pub fn max_ratio() -> Self {
        Self {
            compression_level: 22,
            ..Default::default()
        }
    }
}

#[derive(Copy, Clone)]
enum ArrayData<'a> {
    F16(&'a [u16]),
    F32(&'a [f32]),
    F64(&'a [f64]),
    I16(&'a [i16]),
    I32(&'a [i32]),
    I64(&'a [i64]),
}

impl<'a> ArrayData<'a> {
    fn element_count(self) -> usize {
        match self {
            Self::F16(e) => e.len(),
            Self::F32(e) => e.len(),
            Self::F64(e) => e.len(),
            Self::I16(e) => e.len(),
            Self::I32(e) => e.len(),
            Self::I64(e) => e.len(),
        }
    }

    fn is_empty(self) -> bool {
        self.element_count() == 0
    }

    fn slice(self, start: usize, end: usize) -> ArrayData<'a> {
        match self {
            Self::F16(e) => Self::F16(&e[start..end]),
            Self::F32(e) => Self::F32(&e[start..end]),
            Self::F64(e) => Self::F64(&e[start..end]),
            Self::I16(e) => Self::I16(&e[start..end]),
            Self::I32(e) => Self::I32(&e[start..end]),
            Self::I64(e) => Self::I64(&e[start..end]),
        }
    }

    fn value_at(self, index: usize) -> f64 {
        match self {
            Self::F16(e) => f16_bits_to_f64(e[index]),
            Self::F32(e) => e[index] as f64,
            Self::F64(e) => e[index],
            Self::I16(e) => e[index] as f64,
            Self::I32(e) => e[index] as f64,
            Self::I64(e) => e[index] as f64,
        }
    }

    fn is_monotonic_non_decreasing(self) -> bool {
        let count = self.element_count();
        if count < 2 {
            return true;
        }
        let mut previous = self.value_at(0);
        for index in 1..count {
            let current = self.value_at(index);
            if current < previous {
                return false;
            }
            previous = current;
        }
        true
    }
}

fn array_data_from_binary_data_array(bda: &BinaryDataArray) -> Option<ArrayData<'_>> {
    match bda.binary.as_ref()? {
        NumericArray::F16(e) => Some(ArrayData::F16(e)),
        NumericArray::I16(e) => Some(ArrayData::I16(e)),
        NumericArray::I32(e) => Some(ArrayData::I32(e)),
        NumericArray::I64(e) => Some(ArrayData::I64(e)),
        NumericArray::F32(e) => Some(ArrayData::F32(e)),
        NumericArray::F64(e) => Some(ArrayData::F64(e)),
    }
}

fn element_byte_size_for_dtype(dtype: u8) -> usize {
    match dtype {
        FILE_DTYPE_F16 | FILE_DTYPE_I16 => 2,
        FILE_DTYPE_F32 | FILE_DTYPE_I32 => 4,
        FILE_DTYPE_F64 | FILE_DTYPE_I64 => 8,
        _ => 1,
    }
}

fn resolve_array_dtype(bda: &BinaryDataArray, data: ArrayData<'_>, force_f32: bool) -> u8 {
    match data {
        ArrayData::F16(_) => FILE_DTYPE_F16,
        ArrayData::I16(_) => FILE_DTYPE_I16,
        ArrayData::I32(_) => FILE_DTYPE_I32,
        ArrayData::I64(_) => FILE_DTYPE_I64,
        ArrayData::F32(_) | ArrayData::F64(_) => {
            if float_data_should_be_written_as_f64(bda, data, force_f32) {
                FILE_DTYPE_F64
            } else {
                FILE_DTYPE_F32
            }
        }
    }
}

fn float_data_should_be_written_as_f64(
    bda: &BinaryDataArray,
    data: ArrayData<'_>,
    force_f32: bool,
) -> bool {
    if force_f32 {
        return false;
    }
    if matches!(data, ArrayData::F64(_)) {
        return true;
    }
    declared_float_precision_is_64bit(bda).unwrap_or(false)
}

fn declared_float_precision_is_64bit(bda: &BinaryDataArray) -> Option<bool> {
    if let Some(nt) = bda.numeric_type.as_ref() {
        return match nt {
            NumericType::Float64 => Some(true),
            NumericType::Float32 => Some(false),
            _ => None,
        };
    }
    let (mut saw32, mut saw64) = (false, false);
    for cv in &bda.cv_params {
        match parse_accession_tail_raw(cv.accession.as_deref()) {
            FLOAT_32BIT => saw32 = true,
            FLOAT_64BIT => saw64 = true,
            _ => {}
        }
        if saw32 && saw64 {
            break;
        }
    }
    match (saw32, saw64) {
        (true, false) => Some(false),
        (false, true) => Some(true),
        _ => None,
    }
}

fn validate_array_dtype(data: ArrayData<'_>, dtype: u8) -> IonResult<()> {
    let ok = matches!(
        (dtype, data),
        (FILE_DTYPE_F16, ArrayData::F16(_))
            | (FILE_DTYPE_F32, ArrayData::F32(_))
            | (FILE_DTYPE_F32, ArrayData::F64(_))
            | (FILE_DTYPE_F64, ArrayData::F64(_))
            | (FILE_DTYPE_F64, ArrayData::F32(_))
            | (FILE_DTYPE_I16, ArrayData::I16(_))
            | (FILE_DTYPE_I32, ArrayData::I32(_))
            | (FILE_DTYPE_I64, ArrayData::I64(_))
    );
    if ok {
        Ok(())
    } else {
        Err(format!(
            "write_array_data: incompatible dtype {dtype} for the given array data variant"
        )
        .into())
    }
}

fn write_array_data(buf: &mut Vec<u8>, data: ArrayData<'_>, dtype: u8) {
    match (dtype, data) {
        (FILE_DTYPE_F16, ArrayData::F16(e)) => write_u16_slice_le(buf, e),
        (FILE_DTYPE_F32, ArrayData::F32(e)) => write_f32_slice_le(buf, e),
        (FILE_DTYPE_F32, ArrayData::F64(e)) => {
            for &v in e {
                write_f32_le(buf, v as f32);
            }
        }
        (FILE_DTYPE_F64, ArrayData::F64(e)) => write_f64_slice_le(buf, e),
        (FILE_DTYPE_F64, ArrayData::F32(e)) => {
            for &v in e {
                write_f64_le(buf, v as f64);
            }
        }
        (FILE_DTYPE_I16, ArrayData::I16(e)) => write_i16_slice_le(buf, e),
        (FILE_DTYPE_I32, ArrayData::I32(e)) => write_i32_slice_le(buf, e),
        (FILE_DTYPE_I64, ArrayData::I64(e)) => write_i64_slice_le(buf, e),
        // SAFETY: `validate_array_dtype` is always called before this function.
        _ => unreachable!("write_array_data called with unvalidated dtype/data combination"),
    }
}

fn narrow_f64_to_f32(values: &[f64]) -> Vec<f32> {
    values.iter().map(|&value| value as f32).collect()
}

fn widen_f32_to_f64(values: &[f32]) -> Vec<f64> {
    values.iter().map(|&value| value as f64).collect()
}

fn encode_floats_through_packing(
    buf: &mut Vec<u8>,
    data: ArrayData<'_>,
    dtype_enum: Dtype,
    packing: &'static dyn Packing,
) -> IonResult<()> {
    match (dtype_enum, data) {
        (Dtype::F64, ArrayData::F64(slice)) => packing.encode(PackingInput::F64(slice), buf),
        (Dtype::F32, ArrayData::F32(slice)) => packing.encode(PackingInput::F32(slice), buf),
        (Dtype::F32, ArrayData::F64(slice)) => {
            let narrowed = narrow_f64_to_f32(slice);
            packing.encode(PackingInput::F32(&narrowed), buf)
        }
        (Dtype::F64, ArrayData::F32(slice)) => {
            let widened = widen_f32_to_f64(slice);
            packing.encode(PackingInput::F64(&widened), buf)
        }
        _ => Err(IonError::from(
            "shuffle packing requires f32 or f64 array data",
        )),
    }
}

pub(crate) struct EncodedArrayAddress {
    pub(crate) element_offset: u64,
    pub(crate) element_count: u64,
    pub(crate) block_id: u32,
    pub(crate) accession: u32,
    pub(crate) dtype: u8,
    pub(crate) array_filter: u8,
    pub(crate) encoded_len: u32,
    pub(crate) continues_previous_segment: u8,
    pub(crate) array_cv_code: u8,
}

fn encode_variable_length_array(
    array_type: u32,
    data: ArrayData<'_>,
    dtype: u8,
    dtype_enum: Dtype,
    packing: &'static dyn Packing,
    container: &mut BlockWriter<'_, DefaultCompressor>,
) -> IonResult<(u32, u64, u32)> {
    let mut encoded = Vec::new();
    match packing.id() {
        PackingId::Raw => write_array_data(&mut encoded, data, dtype),
        _ => encode_floats_through_packing(&mut encoded, data, dtype_enum, packing)?,
    }
    let enc_len =
        u32::try_from(encoded.len()).map_err(|_| IonError::from("encoded array exceeds 4 GiB"))?;
    let (bid, eoff) = container.add_item_to_box(array_type, encoded.len(), 1, |buf| {
        buf.extend_from_slice(&encoded);
        Ok(())
    })?;
    Ok((bid, eoff, enc_len))
}

fn write_fixed_array_payload(
    buf: &mut Vec<u8>,
    data: ArrayData<'_>,
    dtype: u8,
    dtype_enum: Dtype,
    packing: &'static dyn Packing,
) -> IonResult<()> {
    match packing.id() {
        PackingId::Raw => {
            write_array_data(buf, data, dtype);
            Ok(())
        }
        _ => encode_floats_through_packing(buf, data, dtype_enum, packing),
    }
}

fn encode_fixed_length_array(
    array_type: u32,
    data: ArrayData<'_>,
    dtype: u8,
    dtype_enum: Dtype,
    elem_bytes: usize,
    packing: &'static dyn Packing,
    container: &mut BlockWriter<'_, DefaultCompressor>,
) -> IonResult<(u32, u64, u32)> {
    let (bid, eoff) = container.add_item_to_box(
        array_type,
        data.element_count() * elem_bytes,
        elem_bytes,
        |buf| write_fixed_array_payload(buf, data, dtype, dtype_enum, packing),
    )?;
    Ok((bid, eoff, 0u32))
}

struct ArrayEncoding<'a> {
    data: ArrayData<'a>,
    accession: u32,
    array_cv_code: u8,
    dtype: u8,
    dtype_enum: Dtype,
    elem_bytes: usize,
    packing: &'static dyn Packing,
}

fn resolve_array_encoding<'a>(
    bda: &'a BinaryDataArray,
    config: WriteOptions,
    policy: ArrayPolicy,
) -> IonResult<Option<ArrayEncoding<'a>>> {
    let Some(data) = array_data_from_binary_data_array(bda) else {
        return Ok(None);
    };
    if data.is_empty() {
        return Ok(None);
    }
    let accession = array_type_accession_from_binary_data_array(bda);
    let array_cv_code = array_type_cv_code_from_binary_data_array(bda);
    let dtype = resolve_array_dtype(bda, data, policy.should_force_f32(accession));
    validate_array_dtype(data, dtype)?;
    let elem_bytes = element_byte_size_for_dtype(dtype);
    let dtype_enum = Dtype::from_byte(dtype).unwrap_or(Dtype::F64);
    let requested: &'static dyn Packing = if config.compression_is_enabled() {
        packing_for(accession, dtype_enum, data.element_count())
    } else {
        &RAW_PACKING
    };
    let packing: &'static dyn Packing = if requested.supports(dtype_enum) {
        requested
    } else {
        &RAW_PACKING
    };
    Ok(Some(ArrayEncoding {
        data,
        accession,
        array_cv_code,
        dtype,
        dtype_enum,
        elem_bytes,
        packing,
    }))
}

pub(crate) fn encode_single_array(
    bda: &BinaryDataArray,
    config: WriteOptions,
    policy: ArrayPolicy,
    container: &mut BlockWriter<'_, DefaultCompressor>,
) -> IonResult<Option<EncodedArrayAddress>> {
    let Some(encoding) = resolve_array_encoding(bda, config, policy)? else {
        return Ok(None);
    };
    let ArrayEncoding {
        data,
        accession,
        array_cv_code,
        dtype,
        dtype_enum,
        elem_bytes,
        packing,
    } = encoding;
    let (block_id, element_offset, encoded_len) = if packing.is_variable_length() {
        encode_variable_length_array(accession, data, dtype, dtype_enum, packing, container)?
    } else {
        encode_fixed_length_array(
            accession, data, dtype, dtype_enum, elem_bytes, packing, container,
        )?
    };
    Ok(Some(EncodedArrayAddress {
        element_offset,
        element_count: data.element_count() as u64,
        block_id,
        accession,
        dtype,
        array_filter: packing.id() as u8,
        encoded_len,
        continues_previous_segment: 0,
        array_cv_code,
    }))
}

pub(crate) fn array_is_fixed_width_splittable(
    bda: &BinaryDataArray,
    config: WriteOptions,
    policy: ArrayPolicy,
) -> IonResult<Option<(usize, usize)>> {
    let Some(encoding) = resolve_array_encoding(bda, config, policy)? else {
        return Ok(None);
    };
    if encoding.packing.is_variable_length() {
        return Ok(None);
    }
    Ok(Some((encoding.data.element_count(), encoding.elem_bytes)))
}

pub(crate) fn check_spectrum_rt_order(
    summary: &[u8],
    spectrum_index: usize,
    last_rt: &mut f64,
) -> IonResult<()> {
    let rt = f64::from_le_bytes(summary[0..8].try_into().unwrap());
    if !rt.is_finite() {
        return Ok(());
    }
    let previous = *last_rt;
    if rt < previous {
        return Err(format!(
            "spectrum {spectrum_index}: retention time {rt} is below the previous spectrum's \
             ({previous}); spectra must be stored in ascending rt order"
        )
        .into());
    }
    *last_rt = rt;
    Ok(())
}

pub(crate) fn check_spectrum_mz_order(
    arrays: &[BinaryDataArray],
    spectrum_index: usize,
) -> IonResult<()> {
    use crate::accessions::MZ_ARRAY;
    for bda in arrays {
        if array_type_accession_from_binary_data_array(bda) != MZ_ARRAY {
            continue;
        }
        let Some(data) = array_data_from_binary_data_array(bda) else {
            return Ok(());
        };
        if !data.is_monotonic_non_decreasing() {
            return Err(format!(
                "spectrum {spectrum_index}: m/z array must be sorted ascending (non-decreasing) for range reads"
            )
            .into());
        }
        return Ok(());
    }
    Ok(())
}

pub(crate) fn window_ranges_for_item(
    arrays: &[BinaryDataArray],
    config: WriteOptions,
    policy: ArrayPolicy,
    mz_window: f64,
) -> IonResult<Option<Vec<WindowRange>>> {
    let windowing = Windowing::new(mz_window.round());
    if !windowing.is_enabled() {
        return Ok(None);
    }
    let Some(x_array) = arrays
        .iter()
        .find(|bda| array_type_accession_from_binary_data_array(bda) == policy.x_array_accession)
    else {
        return Ok(None);
    };
    let Some((x_count, _)) = array_is_fixed_width_splittable(x_array, config, policy)? else {
        return Ok(None);
    };
    if x_count == 0 {
        return Ok(None);
    }
    for bda in arrays {
        match array_is_fixed_width_splittable(bda, config, policy)? {
            Some((count, _)) if count == x_count => {}
            _ => return Ok(None),
        }
    }
    let Some(x_data) = array_data_from_binary_data_array(x_array) else {
        return Ok(None);
    };
    let ranges = windowing.split_sorted(x_count, |index| x_data.value_at(index));
    const MAX_WINDOW_INDEX: u32 = 1 << 20;
    if ranges
        .last()
        .is_some_and(|range| range.window_index > MAX_WINDOW_INDEX)
    {
        return Err(format!(
            "m/z window index exceeds limit {MAX_WINDOW_INDEX}; m/z value too large for window width {}",
            mz_window.round()
        )
        .into());
    }
    Ok(Some(ranges))
}

pub(crate) fn write_array_windows(
    bda: &BinaryDataArray,
    config: WriteOptions,
    policy: ArrayPolicy,
    windows: &[WindowRange],
    container: &mut BlockWriter<'_, DefaultCompressor>,
) -> IonResult<Vec<EncodedArrayAddress>> {
    let Some(encoding) = resolve_array_encoding(bda, config, policy)? else {
        return Ok(Vec::new());
    };
    let ArrayEncoding {
        data,
        accession,
        array_cv_code,
        dtype,
        dtype_enum,
        elem_bytes,
        packing,
    } = encoding;

    if packing.is_variable_length() {
        return Err("write_array_windows: variable-length arrays cannot be windowed".into());
    }

    let mut addresses = Vec::with_capacity(windows.len());
    for (segment_index, window) in windows.iter().enumerate() {
        let segment_data = data.slice(window.start, window.end);
        let element_count = window.element_count();
        let (block_id, element_offset) = container.add_item_to_window(
            accession,
            window.window_index,
            element_count * elem_bytes,
            elem_bytes,
            |buf| write_fixed_array_payload(buf, segment_data, dtype, dtype_enum, packing),
        )?;
        addresses.push(EncodedArrayAddress {
            element_offset,
            element_count: element_count as u64,
            block_id,
            accession,
            dtype,
            array_filter: packing.id() as u8,
            encoded_len: 0,
            continues_previous_segment: (segment_index > 0) as u8,
            array_cv_code,
        });
    }
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ion::{
            encoder::{ion_writer::write_mzml_to_ion, utilities::output::WriteBytes},
            format::{FILE_SIGNATURE, FILE_TRAILER, HEADER_SIZE},
            header::{
                HEADER_CHROM_BLOCK_COUNT, HEADER_SPECTRUM_BLOCK_COUNT, HEADER_TARGET_BLOCK_SIZE,
                HEADER_TARGET_MZ_WINDOW, HEADER_TOTAL_FILE_SIZE,
            },
        },
        mzml::structs::MzML,
    };

    fn summary_with_rt(rt: f64) -> [u8; SPEC_SUMMARY_SIZE] {
        let mut summary = [0u8; SPEC_SUMMARY_SIZE];
        summary[0..8].copy_from_slice(&rt.to_le_bytes());
        summary
    }

    #[test]
    fn byte_shuffle_array_encode_arm_shuffles_not_raw_71() {
        use crate::ion::packing::packing_by_id;

        let data: Vec<f64> = (0..600)
            .map(|i| 100.0 + (i as f64) * 0.0137 + ((i * 7 % 13) as f64) * 0.0009)
            .collect();

        let byte_shuffle = packing_by_id(PackingId::ByteShuffle);

        let mut shuffled = Vec::new();
        write_fixed_array_payload(
            &mut shuffled,
            ArrayData::F64(&data),
            FILE_DTYPE_F64,
            Dtype::F64,
            byte_shuffle,
        )
        .unwrap();

        let mut raw = Vec::new();
        write_array_data(&mut raw, ArrayData::F64(&data), FILE_DTYPE_F64);

        assert_eq!(shuffled.len(), raw.len());
        assert_ne!(
            shuffled, raw,
            "ByteShuffle encode arm must shuffle, not write raw bytes"
        );

        let mut expected = Vec::new();
        byte_shuffle
            .encode(PackingInput::F64(&data), &mut expected)
            .unwrap();
        assert_eq!(
            shuffled, expected,
            "ByteShuffle encode arm must produce exactly the SIMD byte-transpose output"
        );
    }

    #[test]
    fn rt_order_allows_ascending_and_missing_but_rejects_a_drop() {
        let mut last_rt = f64::NEG_INFINITY;
        check_spectrum_rt_order(&summary_with_rt(1.0), 0, &mut last_rt)
            .expect("first rt is allowed");
        check_spectrum_rt_order(&summary_with_rt(1.0), 1, &mut last_rt)
            .expect("equal rt is allowed");
        check_spectrum_rt_order(&summary_with_rt(2.0), 2, &mut last_rt)
            .expect("rising rt is allowed");

        check_spectrum_rt_order(&summary_with_rt(f64::NAN), 3, &mut last_rt)
            .expect("missing rt is allowed");
        assert_eq!(last_rt, 2.0, "missing rt must not move the last seen rt");

        let dropped = check_spectrum_rt_order(&summary_with_rt(1.5), 4, &mut last_rt);
        let message = dropped
            .expect_err("rt below the previous spectrum must be rejected")
            .to_string();
        assert!(
            message.contains("spectrum 4"),
            "error names the spectrum: {message}"
        );
        assert!(
            message.contains("ascending rt order"),
            "error explains the rule: {message}"
        );
    }

    #[test]
    fn encode_starts_with_magic_and_ends_with_trailer() {
        let mzml = MzML::default();
        let mut buf = Vec::new();
        write_mzml_to_ion(
            &mzml,
            WriteOptions {
                compression_level: 0,
                force_f32: false,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        assert!(buf.len() >= HEADER_SIZE + FILE_TRAILER.len());
        assert_eq!(&buf[..FILE_SIGNATURE.len()], &FILE_SIGNATURE);
        assert_eq!(&buf[buf.len() - FILE_TRAILER.len()..], &FILE_TRAILER);
    }

    #[test]
    fn encode_total_file_size_is_correct() {
        let mzml = MzML::default();
        let mut buf = Vec::new();
        write_mzml_to_ion(
            &mzml,
            WriteOptions {
                compression_level: 0,
                force_f32: false,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        let total = u64::from_le_bytes(
            buf[HEADER_TOTAL_FILE_SIZE..HEADER_TOTAL_FILE_SIZE + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(total, buf.len() as u64);
    }

    #[test]
    fn encode_header_target_block_size_matches_default() {
        let mzml = MzML::default();
        let mut buf = Vec::new();
        write_mzml_to_ion(
            &mzml,
            WriteOptions {
                compression_level: 0,
                force_f32: false,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        let size = u64::from_le_bytes(
            buf[HEADER_TARGET_BLOCK_SIZE..HEADER_TARGET_BLOCK_SIZE + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(size, TARGET_BLOCK_UNCOMPRESSED_BYTES as u64);
    }

    #[test]
    fn encode_header_target_mz_window_matches_default() {
        let mzml = MzML::default();
        let mut buf = Vec::new();
        write_mzml_to_ion(
            &mzml,
            WriteOptions {
                compression_level: 0,
                force_f32: false,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        let window = u32::from_le_bytes(
            buf[HEADER_TARGET_MZ_WINDOW..HEADER_TARGET_MZ_WINDOW + 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(window, DEFAULT_MZ_WINDOW.round() as u32);
    }

    #[test]
    fn encode_header_crc32_is_valid() {
        let mzml = MzML::default();
        let mut buf = Vec::new();
        write_mzml_to_ion(
            &mzml,
            WriteOptions {
                compression_level: 0,
                force_f32: false,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        let stored = u32::from_le_bytes(buf[1020..1024].try_into().unwrap());
        assert_eq!(stored, crc32fast::hash(&buf[0..1020]));
    }

    #[test]
    fn encode_header_block_counts_are_zero_for_empty_mzml() {
        let mzml = MzML::default();
        let mut buf = Vec::new();
        write_mzml_to_ion(
            &mzml,
            WriteOptions {
                compression_level: 0,
                force_f32: false,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        let spec_blocks = u64::from_le_bytes(
            buf[HEADER_SPECTRUM_BLOCK_COUNT..HEADER_SPECTRUM_BLOCK_COUNT + 8]
                .try_into()
                .unwrap(),
        );
        let chrom_blocks = u64::from_le_bytes(
            buf[HEADER_CHROM_BLOCK_COUNT..HEADER_CHROM_BLOCK_COUNT + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(spec_blocks, 0);
        assert_eq!(chrom_blocks, 0);
    }

    #[test]
    fn vec_patch_in_bounds() {
        let mut output = vec![0u8; 16];
        output.patch(4, &[1u8, 2, 3, 4]).unwrap();
        assert_eq!(&output[4..8], &[1u8, 2, 3, 4]);
    }

    #[test]
    fn vec_patch_out_of_bounds_errors() {
        let mut output = vec![0u8; 4];
        assert!(output.patch(3, &[1u8, 2, 3]).is_err());
    }

    #[test]
    fn declared_float_precision_prefers_numeric_type_field() {
        let bda = BinaryDataArray {
            numeric_type: Some(NumericType::Float64),
            ..Default::default()
        };
        assert_eq!(declared_float_precision_is_64bit(&bda), Some(true));
        let bda = BinaryDataArray {
            numeric_type: Some(NumericType::Float32),
            ..Default::default()
        };
        assert_eq!(declared_float_precision_is_64bit(&bda), Some(false));
    }

    #[test]
    fn resolve_array_dtype_force_f32_overrides_f64_data() {
        let bda = BinaryDataArray::default();
        assert_eq!(
            resolve_array_dtype(&bda, ArrayData::F64(&[1.0f64]), true),
            FILE_DTYPE_F32
        );
    }

    #[test]
    fn resolve_array_dtype_keeps_f64_data_lossless_even_when_declared_32bit() {
        let bda = BinaryDataArray {
            numeric_type: Some(NumericType::Float32),
            ..Default::default()
        };
        assert_eq!(
            resolve_array_dtype(&bda, ArrayData::F64(&[1.0f64]), false),
            FILE_DTYPE_F64
        );
        assert_eq!(
            resolve_array_dtype(&bda, ArrayData::F64(&[1.0f64]), true),
            FILE_DTYPE_F32
        );
    }

    #[test]
    fn resolve_array_dtype_integer_types_unchanged_by_force_f32() {
        let bda = BinaryDataArray::default();
        assert_eq!(
            resolve_array_dtype(&bda, ArrayData::I32(&[1i32]), true),
            FILE_DTYPE_I32
        );
    }

    #[test]
    fn element_byte_size_for_dtype_returns_correct_sizes() {
        assert_eq!(element_byte_size_for_dtype(FILE_DTYPE_F64), 8);
        assert_eq!(element_byte_size_for_dtype(FILE_DTYPE_F32), 4);
        assert_eq!(element_byte_size_for_dtype(FILE_DTYPE_F16), 2);
        assert_eq!(element_byte_size_for_dtype(FILE_DTYPE_I16), 2);
        assert_eq!(element_byte_size_for_dtype(FILE_DTYPE_I32), 4);
        assert_eq!(element_byte_size_for_dtype(FILE_DTYPE_I64), 8);
    }

    #[test]
    fn encoder_config_compression_disabled_at_level_zero() {
        let config = WriteOptions {
            compression_level: 0,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: true,
            section_storage: SectionStorage::Memory,
            mz_window: 0.0,
        };
        assert!(!config.compression_is_enabled());
        assert_eq!(config.codec_id(), 0);
        assert_eq!(config.array_filter_id(), PackingId::Raw as u8);
        assert!(matches!(config.block_packing_id(), PackingId::Raw));
    }

    #[test]
    fn encoder_config_compression_enabled_at_nonzero_level() {
        let config = WriteOptions {
            compression_level: 3,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: true,
            section_storage: SectionStorage::Memory,
            mz_window: 0.0,
        };
        assert!(config.compression_is_enabled());
        assert_eq!(config.codec_id(), 1);
        assert_eq!(config.array_filter_id(), PackingId::ByteShuffle as u8);
        assert!(matches!(config.block_packing_id(), PackingId::ByteShuffle));
    }
}
