use super::*;
use crate::ion::windowing::window_index;

fn window_span(width: f64, from: f64, to: f64, window_count: usize) -> Option<(usize, usize)> {
    if window_count == 0 {
        return None;
    }
    let low = window_index(width, from) as usize;
    if low >= window_count {
        return None;
    }
    Some((
        low,
        (window_index(width, to) as usize).min(window_count - 1),
    ))
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct DataXY {
    pub x: NumericArray,
    pub y: NumericArray,
}

impl DataXY {
    pub(crate) fn empty() -> Self {
        Self {
            x: NumericArray::F64(Vec::new()),
            y: NumericArray::F64(Vec::new()),
        }
    }
}

impl NumericArray {
    pub fn to_f64(&self) -> Vec<f64> {
        match self {
            NumericArray::F64(values) => values.clone(),
            NumericArray::F32(values) => values.iter().map(|&value| value as f64).collect(),
            NumericArray::F16(values) => values.iter().copied().map(f16_bits_to_f64).collect(),
            NumericArray::I16(values) => values.iter().map(|&value| value as f64).collect(),
            NumericArray::I32(values) => values.iter().map(|&value| value as f64).collect(),
            NumericArray::I64(values) => values.iter().map(|&value| value as f64).collect(),
        }
    }

    pub(crate) fn extend_f64(&self, out: &mut Vec<f64>) {
        match self {
            NumericArray::F64(values) => out.extend_from_slice(values),
            NumericArray::F32(values) => out.extend(values.iter().map(|&value| value as f64)),
            NumericArray::F16(values) => out.extend(values.iter().copied().map(f16_bits_to_f64)),
            NumericArray::I16(values) => out.extend(values.iter().map(|&value| value as f64)),
            NumericArray::I32(values) => out.extend(values.iter().map(|&value| value as f64)),
            NumericArray::I64(values) => out.extend(values.iter().map(|&value| value as f64)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pixel {
    pub(crate) x: Range,
    pub(crate) y: Range,
    pub(crate) z: Range,
}

#[non_exhaustive]
#[allow(private_interfaces)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Select {
    #[default]
    All,
    Rt(Range),
    Area(Pixel),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScanQuery {
    pub mz: Range,
    pub select: Select,
    pub ms_level: Option<u8>,
}

#[non_exhaustive]
pub struct Window<'a> {
    pub index: usize,
    pub summary: &'a ScanSummary,
    pub mz: &'a [f64],
    pub intensity: &'a [f64],
}

fn position_in_range(value: u32, range: Range) -> bool {
    let value = value as f64;
    value >= range.from && value <= range.to
}

fn scan_is_selected(select: &Select, summary: &ScanSummary) -> bool {
    match select {
        Select::All => true,
        Select::Rt(range) => summary.rt >= range.from && summary.rt <= range.to,
        Select::Area(pixel) => {
            position_in_range(summary.position_x, pixel.x)
                && position_in_range(summary.position_y, pixel.y)
                && position_in_range(summary.position_z, pixel.z)
        }
    }
}

fn scan_summary_from_record(record: &SpectrumSummary) -> ScanSummary {
    ScanSummary {
        rt: record.rt,
        rt_unit: TimeUnit::from_code(record.rt_unit),
        ms_level: record.ms_level,
        polarity: record.polarity,
        selected_ion_mz: record.selected_ion_mz,
        base_peak_mz: record.base_peak_mz,
        base_peak_int: record.base_peak_int,
        total_ion_current: record.total_ion_current,
        position_x: record.position_x,
        position_y: record.position_y,
        position_z: record.position_z,
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemKind {
    Spectrum,
    Chromatogram,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct ItemSlice {
    pub(crate) item_index: u64,
    pub(crate) array_address_index: u64,
    pub(crate) intensity_address_index: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowRead {
    pub(crate) mz_address: ArrayAddress,
    pub(crate) intensity_address: ArrayAddress,
}

fn empty_array(dtype: u8) -> NumericArray {
    match dtype {
        FILE_DTYPE_F32 => NumericArray::F32(Vec::new()),
        FILE_DTYPE_F16 => NumericArray::F16(Vec::new()),
        FILE_DTYPE_I16 => NumericArray::I16(Vec::new()),
        FILE_DTYPE_I32 => NumericArray::I32(Vec::new()),
        FILE_DTYPE_I64 => NumericArray::I64(Vec::new()),
        _ => NumericArray::F64(Vec::new()),
    }
}

fn group_dtype(group: &ArrayGroup) -> u8 {
    group
        .refs
        .first()
        .map(|array_address| array_address.dtype)
        .unwrap_or(FILE_DTYPE_F64)
}

fn value_at(array: &NumericArray, index: usize) -> f64 {
    match array {
        NumericArray::F64(values) => values[index],
        NumericArray::F32(values) => values[index] as f64,
        NumericArray::F16(values) => f16_bits_to_f64(values[index]),
        NumericArray::I16(values) => values[index] as f64,
        NumericArray::I32(values) => values[index] as f64,
        NumericArray::I64(values) => values[index] as f64,
    }
}

fn append_range(dst: &mut NumericArray, src: &NumericArray, start: usize, end: usize) {
    match (dst, src) {
        (NumericArray::F64(d), NumericArray::F64(s)) => d.extend_from_slice(&s[start..end]),
        (NumericArray::F32(d), NumericArray::F32(s)) => d.extend_from_slice(&s[start..end]),
        (NumericArray::F16(d), NumericArray::F16(s)) => d.extend_from_slice(&s[start..end]),
        (NumericArray::I16(d), NumericArray::I16(s)) => d.extend_from_slice(&s[start..end]),
        (NumericArray::I32(d), NumericArray::I32(s)) => d.extend_from_slice(&s[start..end]),
        (NumericArray::I64(d), NumericArray::I64(s)) => d.extend_from_slice(&s[start..end]),
        _ => {}
    }
}

fn first_index_at_or_after(
    x: &NumericArray,
    positions: std::ops::Range<usize>,
    target: f64,
) -> usize {
    let (mut low, mut high) = (positions.start, positions.end);
    while low < high {
        let middle = low + (high - low) / 2;
        if value_at(x, middle) < target {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

fn first_index_after(x: &NumericArray, positions: std::ops::Range<usize>, target: f64) -> usize {
    let (mut low, mut high) = (positions.start, positions.end);
    while low < high {
        let middle = low + (high - low) / 2;
        if value_at(x, middle) <= target {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

fn range_in_sorted(x: &NumericArray, low: f64, high: f64, paired: usize) -> (usize, usize) {
    (
        first_index_at_or_after(x, 0..paired, low),
        first_index_after(x, 0..paired, high),
    )
}

fn is_sorted(x: &NumericArray, len: usize) -> bool {
    (1..len).all(|index| value_at(x, index - 1) <= value_at(x, index))
}

pub(crate) fn keep_pairs_sorted(
    x: &NumericArray,
    y: &NumericArray,
    low: f64,
    high: f64,
    x_out: &mut NumericArray,
    y_out: &mut NumericArray,
) {
    let paired = x.len().min(y.len());
    let (start, end) = range_in_sorted(x, low, high, paired);
    append_range(x_out, x, start, end);
    append_range(y_out, y, start, end);
}

pub(crate) fn keep_pairs(
    x: &NumericArray,
    y: &NumericArray,
    low: f64,
    high: f64,
    x_out: &mut NumericArray,
    y_out: &mut NumericArray,
) {
    let paired = x.len().min(y.len());
    if is_sorted(x, paired) {
        keep_pairs_sorted(x, y, low, high, x_out, y_out);
        return;
    }
    for index in 0..paired {
        let value = value_at(x, index);
        if value >= low && value <= high {
            append_range(x_out, x, index, index + 1);
            append_range(y_out, y, index, index + 1);
        }
    }
}

impl IonReader {
    pub fn read_spectrum_window(
        &mut self,
        index: usize,
        x_array_accession: impl Into<u32>,
        y_array_accession: impl Into<u32>,
        low: f64,
        high: f64,
    ) -> IonResult<DataXY> {
        let x_array_accession = x_array_accession.into();
        let y_array_accession = y_array_accession.into();
        self.read_spectrum_window_inner(index, x_array_accession, y_array_accession, low, high)
    }

    pub fn require_bounds(&mut self) -> IonResult<()> {
        self.ensure_spec_window_directory()
    }

    pub(crate) fn get_spectrum_mz_windows(
        &mut self,
        scan_index: usize,
        mz_from: f64,
        mz_to: f64,
    ) -> IonResult<Vec<WindowRead>> {
        if !mz_from.is_finite() || !mz_to.is_finite() {
            return Err("mz window bounds must be finite".into());
        }
        if mz_from > mz_to {
            return Err("mz window: from is greater than to".into());
        }
        if scan_index >= self.header.spectrum_count as usize {
            return Err("spectrum index out of range".into());
        }

        self.require_bounds()?;

        let width = self.header.target_mz_window as f64;
        let rows = {
            let window_directory = match &self.spec_window_directory {
                WindowDirectoryCache::Loaded(index) => index,
                _ => return Err(IonError::MissingSpectrumBounds),
            };
            let Some((window_low, window_high)) =
                window_span(width, mz_from, mz_to, window_directory.window_count())
            else {
                return Ok(Vec::new());
            };
            let mut rows = Vec::new();
            for window in window_low..=window_high {
                if let Some(row) = window_directory.find_in_window(window, scan_index as u32) {
                    rows.push(row);
                }
            }
            rows
        };

        let mut windows = Vec::with_capacity(rows.len());
        for row in rows {
            windows.push(WindowRead {
                mz_address: self.array_address_at(row.mz_address)?,
                intensity_address: self.array_address_at(row.intensity_address)?,
            });
        }
        Ok(windows)
    }

    fn array_address_at(&self, ref_index: u32) -> IonResult<ArrayAddress> {
        let start = ref_index as usize * ARRAY_ADDRESS_BYTES;
        let bytes = self
            .spec_array_addresses
            .get(start..start + ARRAY_ADDRESS_BYTES)
            .ok_or_else(|| IonError::from("window directory: array ref index out of range"))?;
        Ok(parse_array_address(bytes))
    }

    pub(crate) fn spec_block_byte_range(&self, block_id: u32) -> IonResult<ByteRange> {
        self.spec_container
            .block_byte_range(block_id)
            .ok_or_else(|| IonError::from("spectrum block id out of range"))
    }

    pub fn eic_byte_ranges(
        &mut self,
        mz: Range,
        rt: Option<Range>,
        gap: u64,
    ) -> IonResult<Vec<ByteRange>> {
        if !mz.from.is_finite() || !mz.to.is_finite() {
            return Err("eic byte ranges: mz bounds must be finite".into());
        }
        if mz.from > mz.to {
            return Err("eic byte ranges: mz from is greater than to".into());
        }
        self.ensure_spec_window_directory()?;

        let width = self.header.target_mz_window as f64;
        let rt_is_searchable = self.spec_rt_is_finite_ascending();

        let address_indices = {
            let WindowDirectoryCache::Loaded(window_directory) = &self.spec_window_directory else {
                return Err(IonError::MissingSpectrumBounds);
            };
            let Some((window_low, window_high)) =
                window_span(width, mz.from, mz.to, window_directory.window_count())
            else {
                return Ok(Vec::new());
            };
            let mut address_indices = Vec::new();
            for window in window_low..=window_high {
                for position in window_directory.window_range(window) {
                    let row = window_directory.row(position);
                    if let Some(wanted) = rt {
                        let found = self.spectrum_rt(row.spectrum_index as usize);
                        if found < wanted.from || found > wanted.to {
                            if rt_is_searchable && found > wanted.to {
                                break;
                            }
                            continue;
                        }
                    }
                    address_indices.push(row.mz_address);
                    address_indices.push(row.intensity_address);
                }
            }
            address_indices
        };

        let mut block_ids = Vec::with_capacity(address_indices.len());
        for index in address_indices {
            block_ids.push(self.array_address_at(index)?.block_id);
        }
        block_ids.sort_unstable();
        block_ids.dedup();

        let mut ranges = Vec::with_capacity(block_ids.len());
        for block_id in block_ids {
            ranges.push(self.spec_block_byte_range(block_id)?);
        }
        merge_ranges(&mut ranges, gap);
        Ok(ranges)
    }

    pub fn byte_ranges(&mut self, scan_index: usize, mz: Range) -> IonResult<Vec<ByteRange>> {
        let windows = self.get_spectrum_mz_windows(scan_index, mz.from, mz.to)?;

        let mut block_ids = Vec::with_capacity(windows.len() * 2);
        for window in &windows {
            block_ids.push(window.mz_address.block_id);
            block_ids.push(window.intensity_address.block_id);
        }
        block_ids.sort_unstable();
        block_ids.dedup();

        let mut ranges = Vec::with_capacity(block_ids.len());
        for block_id in block_ids {
            ranges.push(self.spec_block_byte_range(block_id)?);
        }
        ranges.sort_unstable_by_key(|range| (range.offset, range.length));
        Ok(ranges)
    }

    pub fn read_window(&mut self, index: usize, mz: Range) -> IonResult<DataXY> {
        let windows = self.get_spectrum_mz_windows(index, mz.from, mz.to)?;

        let mz_dtype = windows
            .first()
            .map(|s| s.mz_address.dtype)
            .unwrap_or(FILE_DTYPE_F64);
        let intensity_dtype = windows
            .first()
            .map(|s| s.intensity_address.dtype)
            .unwrap_or(FILE_DTYPE_F64);
        let mut mz_out = empty_array(mz_dtype);
        let mut intensity_out = empty_array(intensity_dtype);
        for window in windows {
            let mz_segment = self.read_spectrum_array(&window.mz_address)?;
            let intensity_segment = self.read_spectrum_array(&window.intensity_address)?;
            keep_pairs_sorted(
                &mz_segment,
                &intensity_segment,
                mz.from,
                mz.to,
                &mut mz_out,
                &mut intensity_out,
            );
        }

        Ok(DataXY {
            x: mz_out,
            y: intensity_out,
        })
    }

    pub fn scans_in(
        &mut self,
        query: &ScanQuery,
        mut visit: impl FnMut(&Window),
    ) -> IonResult<()> {
        self.require_bounds()?;
        let count = self.header.spectrum_count as usize;
        let mut mz_out: Vec<f64> = Vec::new();
        let mut intensity_out: Vec<f64> = Vec::new();
        for index in 0..count {
            let Some(record) = self.spectrum_summary(index) else {
                continue;
            };
            if let Some(level) = query.ms_level
                && record.ms_level != level
            {
                continue;
            }
            let summary = scan_summary_from_record(&record);
            if !scan_is_selected(&query.select, &summary) {
                continue;
            }
            let Some(refs) = self.spectrum_array_addresses(index) else {
                continue;
            };
            let has_mz = refs
                .iter()
                .any(|r| r.array_type == crate::accessions::MZ_ARRAY);
            let has_intensity = refs.iter().any(|r| r.array_type == ACC_INT);
            if !has_mz || !has_intensity {
                continue;
            }
            let data = self.read_window(index, query.mz)?;
            mz_out.clear();
            data.x.extend_f64(&mut mz_out);
            intensity_out.clear();
            data.y.extend_f64(&mut intensity_out);
            let window = Window {
                index,
                summary: &summary,
                mz: &mz_out,
                intensity: &intensity_out,
            };
            visit(&window);
        }
        Ok(())
    }

    pub(crate) fn read_spectrum_window_inner(
        &mut self,
        index: usize,
        x_array_accession: u32,
        y_array_accession: u32,
        low: f64,
        high: f64,
    ) -> IonResult<DataXY> {
        if index >= self.header.spectrum_count as usize {
            return Err("spectrum index out of range".into());
        }
        if low > high {
            return Ok(DataXY::empty());
        }
        if x_array_accession == ACC_MZ && y_array_accession == ACC_INT {
            return self.read_window(
                index,
                Range {
                    from: low,
                    to: high,
                },
            );
        }

        let Some(array_addresses) = read_array_addresses_from_buffers(
            &self.spec_entries_buf,
            &self.spec_array_addresses,
            index,
        ) else {
            return Ok(DataXY::empty());
        };
        let groups = group_arrays(array_addresses.as_slice())?;

        let mut x_group = None;
        let mut y_group = None;
        for group in &groups {
            if group.array_type == x_array_accession && x_group.is_none() {
                x_group = Some(group);
            } else if group.array_type == y_array_accession && y_group.is_none() {
                y_group = Some(group);
            }
        }

        let (Some(x_group), Some(y_group)) = (x_group, y_group) else {
            return Ok(DataXY::empty());
        };

        self.read_full_window(x_group, y_group, low, high)
    }

    pub(crate) fn read_full_window(
        &mut self,
        x_group: &ArrayGroup,
        y_group: &ArrayGroup,
        low: f64,
        high: f64,
    ) -> IonResult<DataXY> {
        let x = self.read_group_typed(x_group)?;
        let y = self.read_group_typed(y_group)?;

        let mut x_out = empty_array(group_dtype(x_group));
        let mut y_out = empty_array(group_dtype(y_group));
        keep_pairs(&x, &y, low, high, &mut x_out, &mut y_out);
        Ok(DataXY { x: x_out, y: y_out })
    }

    fn read_group_typed(&mut self, group: &ArrayGroup) -> IonResult<NumericArray> {
        let mut out = empty_array(group_dtype(group));
        for array_address in &group.refs {
            let window = self.read_spectrum_array(array_address)?;
            append_range(&mut out, &window, 0, window.len());
        }
        Ok(out)
    }

    #[cfg(test)]
    pub(crate) fn candidate_items(
        &mut self,
        target: ItemKind,
        axis_accession: u32,
        lo: f64,
        hi: f64,
    ) -> IonResult<Vec<ItemSlice>> {
        let is_axis = match target {
            ItemKind::Spectrum => axis_accession == ACC_MZ,
            ItemKind::Chromatogram => axis_accession == crate::accessions::TIME_ARRAY,
        };
        if !is_axis {
            return Ok(Vec::new());
        }

        if !lo.is_finite() || !hi.is_finite() {
            return Err("candidate_items: m/z range must be finite".into());
        }
        if lo > hi {
            return Err("candidate_items: m/z range must be ordered".into());
        }

        match target {
            ItemKind::Spectrum => {
                let _ = self.ensure_spec_window_directory();
            }
            ItemKind::Chromatogram => self.ensure_chrom_window_directory(),
        }

        let width = self.header.target_mz_window as f64;
        let window_directory = match target {
            ItemKind::Spectrum => &self.spec_window_directory,
            ItemKind::Chromatogram => &self.chrom_window_directory,
        };
        let WindowDirectoryCache::Loaded(window_directory) = window_directory else {
            return Ok(Vec::new());
        };

        let Some((window_low, window_high)) =
            window_span(width, lo, hi, window_directory.window_count())
        else {
            return Ok(Vec::new());
        };

        let mut result = Vec::new();
        for window in window_low..=window_high {
            for position in window_directory.window_range(window) {
                let row = window_directory.row(position);
                result.push(ItemSlice {
                    item_index: row.spectrum_index as u64,
                    array_address_index: row.mz_address as u64,
                    intensity_address_index: row.intensity_address as u64,
                });
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::window_span;

    #[test]
    fn window_span_clamps_to_available_windows() {
        assert_eq!(window_span(250.0, 260.0, 600.0, 4), Some((1, 2)));
        assert_eq!(window_span(250.0, 0.0, 100000.0, 4), Some((0, 3)));
        assert_eq!(window_span(250.0, 100000.0, 100000.0, 4), None);
        assert_eq!(window_span(250.0, 0.0, 100.0, 0), None);
    }
}
