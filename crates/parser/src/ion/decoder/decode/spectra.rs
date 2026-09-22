use super::*;

#[inline]
pub(crate) fn slice_summary(
    bytes: &[u8],
    off: u64,
    index: usize,
    size: usize,
    count: u64,
) -> Option<&[u8]> {
    if index >= count as usize {
        return None;
    }
    let base = usize::try_from(off)
        .ok()
        .and_then(|o| index.checked_mul(size).and_then(|d| o.checked_add(d)))?;
    bytes.get(base..base.checked_add(size)?)
}

#[inline]
pub(crate) fn parse_spec_summary(bytes: &[u8]) -> SpectrumSummary {
    SpectrumSummary {
        rt: f64::from_le_bytes(bytes[0..8].try_into().unwrap()),
        rt_unit: bytes[54],
        base_peak_mz: f64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        selected_ion_mz: f64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        base_peak_int: f64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        total_ion_current: f64::from_le_bytes(bytes[32..40].try_into().unwrap()),
        ms_level: bytes[40],
        polarity: bytes[41],
        position_x: u32::from_le_bytes(bytes[42..46].try_into().unwrap()),
        position_y: u32::from_le_bytes(bytes[46..50].try_into().unwrap()),
        position_z: u32::from_le_bytes(bytes[50..54].try_into().unwrap()),
    }
}

pub(crate) fn parse_chrom_summary(bytes: &[u8]) -> ChromatogramSummary {
    ChromatogramSummary {
        lowest_mz: f64::from_le_bytes(bytes[0..8].try_into().unwrap()),
        highest_mz: f64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        lowest_wavelength: f64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        highest_wavelength: f64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        lowest_ion_mobility: f64::from_le_bytes(bytes[32..40].try_into().unwrap()),
        highest_ion_mobility: f64::from_le_bytes(bytes[40..48].try_into().unwrap()),
        polarity: bytes[48],
    }
}

pub(crate) fn build_one_spectrum(rows: &[Metadatum], fallback_index: usize) -> Option<Spectrum> {
    let children_lookup = ChildrenLookup::new(rows);
    let spectrum_id = children_lookup.all_ids(TagId::Spectrum).first().copied()?;
    let mut owner_rows = OwnerRows::with_capacity(rows.len());
    for row in rows {
        owner_rows.insert(row.id, row);
    }
    let policy = DefaultMetadataPolicy;
    let mut param_buffer = Vec::new();
    Some(parse_spectrum(
        &owner_rows,
        &children_lookup,
        spectrum_id,
        fallback_index as u32,
        &policy,
        &mut param_buffer,
    ))
}

pub(crate) fn build_one_chromatogram(
    rows: &[Metadatum],
    fallback_index: usize,
) -> Option<Chromatogram> {
    let children_lookup = ChildrenLookup::new(rows);
    let chromatogram_id = children_lookup
        .all_ids(TagId::Chromatogram)
        .first()
        .copied()?;
    let mut owner_rows = OwnerRows::with_capacity(rows.len());
    for row in rows {
        owner_rows.insert(row.id, row);
    }
    let policy = DefaultMetadataPolicy;
    let mut param_buffer = Vec::new();
    Some(parse_chromatogram(
        &owner_rows,
        &children_lookup,
        chromatogram_id,
        fallback_index as u32,
        &policy,
        &mut param_buffer,
    ))
}

#[cfg(test)]
pub(crate) struct ScanIterator<'a, 'd> {
    pub(crate) summary_chunks: std::slice::ChunksExact<'a, u8>,
    pub(crate) entry_chunks: std::slice::ChunksExact<'a, u8>,
    pub(crate) address_bytes: &'a [u8],
    pub(crate) container: &'d mut dyn ContainerAccess,
    pub(crate) mz_values: &'d mut Vec<f64>,
    pub(crate) int_values: &'d mut Vec<f64>,
    pub(crate) rt_min: f64,
    pub(crate) rt_max: f64,
    pub(crate) ms_level: u8,
}

#[cfg(test)]
impl<'a, 'd> ScanIterator<'a, 'd> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        summary_bytes: &'a [u8],
        entry_bytes: &'a [u8],
        address_bytes: &'a [u8],
        container: &'d mut dyn ContainerAccess,
        mz_values: &'d mut Vec<f64>,
        int_values: &'d mut Vec<f64>,
        rt_min: f64,
        rt_max: f64,
        ms_level: u8,
    ) -> Self {
        Self {
            summary_chunks: summary_bytes.chunks_exact(SPEC_SUMMARY_SIZE),
            entry_chunks: entry_bytes.chunks_exact(INDEX_ENTRY_BYTES),
            address_bytes,
            container,
            mz_values,
            int_values,
            rt_min,
            rt_max,
            ms_level,
        }
    }

    pub(crate) fn run<F>(&mut self, callback: &mut F)
    where
        F: FnMut(&ScanSummary, &[f64], &[f64]),
    {
        for (summary_bytes, entry_bytes) in
            self.summary_chunks.by_ref().zip(self.entry_chunks.by_ref())
        {
            let summary = parse_spec_summary(summary_bytes);
            if !summary.rt.is_finite() || summary.rt < self.rt_min || summary.rt > self.rt_max {
                continue;
            }
            if self.ms_level != 0 && summary.ms_level != self.ms_level {
                continue;
            }
            if !read_scan_arrays(
                self.container,
                entry_bytes,
                self.address_bytes,
                self.mz_values,
                self.int_values,
            ) {
                continue;
            }
            let len = self.mz_values.len().min(self.int_values.len());
            if len == 0 {
                continue;
            }
            let summary = ScanSummary {
                rt: summary.rt,
                rt_unit: TimeUnit::from_code(summary.rt_unit),
                ms_level: summary.ms_level,
                polarity: summary.polarity,
                base_peak_mz: summary.base_peak_mz,
                selected_ion_mz: summary.selected_ion_mz,
                base_peak_int: summary.base_peak_int,
                total_ion_current: summary.total_ion_current,
                position_x: summary.position_x,
                position_y: summary.position_y,
                position_z: summary.position_z,
            };
            callback(&summary, &self.mz_values[..len], &self.int_values[..len]);
        }
    }
}

impl IonReader {
    pub fn spectrum_summary(&self, index: usize) -> Option<SpectrumSummary> {
        let b = slice_summary(
            &self.spec_summary_buf,
            0,
            index,
            SPEC_SUMMARY_SIZE,
            self.header.spectrum_count,
        )?;
        Some(parse_spec_summary(b))
    }

    pub(crate) fn spectrum_rt(&self, index: usize) -> f64 {
        let start = index * SPEC_SUMMARY_SIZE;
        let Some(bytes) = self.spec_summary_buf.get(start..start + 8) else {
            return f64::NAN;
        };
        let mut rt = [0u8; 8];
        rt.copy_from_slice(bytes);
        f64::from_le_bytes(rt)
    }

    pub(crate) fn spec_rt_is_finite_ascending(&self) -> bool {
        if let Some(known) = self.spec_rt_finite_ascending.get() {
            return known;
        }
        let mut previous = f64::NEG_INFINITY;
        let mut ascending = true;
        for index in 0..self.header.spectrum_count as usize {
            let rt = self.spectrum_rt(index);
            if !rt.is_finite() || rt < previous {
                ascending = false;
                break;
            }
            previous = rt;
        }
        self.spec_rt_finite_ascending.set(Some(ascending));
        ascending
    }

    pub fn spectrum_summaries(&self) -> IonResult<Vec<SpectrumSummary>> {
        let count = usize::try_from(self.header.spectrum_count)
            .map_err(|_| IonError::from("spec summary: out of bounds"))?;
        let len = self.spec_summary_buf.len();
        if len != count * SPEC_SUMMARY_SIZE {
            return Err(
                format!("spec summary: len={len} != count={count} × {SPEC_SUMMARY_SIZE}").into(),
            );
        }
        Ok(self
            .spec_summary_buf
            .chunks_exact(SPEC_SUMMARY_SIZE)
            .map(parse_spec_summary)
            .collect())
    }

    pub fn chromatogram_summaries(&self) -> IonResult<Vec<ChromatogramSummary>> {
        let count = usize::try_from(self.header.chrom_count)
            .map_err(|_| IonError::from("chrom summary: out of bounds"))?;
        let len = self.chrom_summary_buf.len();
        if len != count * CHROM_SUMMARY_SIZE {
            return Err(format!(
                "chrom summary: len={len} != count={count} × {CHROM_SUMMARY_SIZE}"
            )
            .into());
        }
        Ok(self
            .chrom_summary_buf
            .chunks_exact(CHROM_SUMMARY_SIZE)
            .map(parse_chrom_summary)
            .collect())
    }

    pub(crate) fn global_metadata(&self) -> IonResult<Vec<Metadatum>> {
        parse_global_metadata(
            &self.global_meta_buf,
            0,
            self.header.global_meta_count,
            self.header.global_meta_numeric_count,
            self.header.global_meta_string_count,
            self.header.compression_codec,
            self.header.global_meta_uncompressed_bytes,
            self.decompression_limit,
            MetaColumnLayout::new().without_ids_reset(),
        )
    }

    pub fn spectrum_metadata(&self) -> IonResult<Vec<Metadatum>> {
        self.spec_meta_reader.read_all()
    }

    pub fn chromatogram_metadata(&self) -> IonResult<Vec<Metadatum>> {
        self.chrom_meta_reader.read_all()
    }

    pub(crate) fn spectrum_metadata_grouped(&self) -> IonResult<Vec<Vec<Metadatum>>> {
        self.spec_meta_reader.read_all_grouped()
    }

    pub(crate) fn chromatogram_metadata_grouped(&self) -> IonResult<Vec<Vec<Metadatum>>> {
        self.chrom_meta_reader.read_all_grouped()
    }

    pub fn spectrum_metadata_at(&mut self, index: usize) -> IonResult<Vec<Metadatum>> {
        self.spec_meta_reader.read_item(index as u64)
    }

    pub fn chromatogram_metadata_at(&mut self, index: usize) -> IonResult<Vec<Metadatum>> {
        self.chrom_meta_reader.read_item(index as u64)
    }

    pub fn spectrum(&mut self, index: usize) -> IonResult<Spectrum> {
        if index >= self.header.spectrum_count as usize {
            return Err(IonError::OutOfRange {
                index,
                count: self.header.spectrum_count,
            });
        }
        let rows = self.spec_meta_reader.read_item(index as u64)?;
        let Some(mut spectrum) = build_one_spectrum(&rows, index) else {
            return Err(IonError::from(format!(
                "spectrum {index}: metadata is missing the spectrum tag"
            )));
        };

        if let Some(array_addresses) = read_array_addresses_from_buffers(
            &self.spec_entries_buf,
            &self.spec_array_addresses,
            index,
        ) {
            let groups = group_arrays(array_addresses.as_slice())?;
            let bd_list = spectrum
                .binary_data_array_list
                .get_or_insert_with(BinaryDataArrayList::default);
            for group in groups {
                let decoded = read_group_decoded_bytes(&group, &mut self.spec_container)?;
                attach_logical_array(
                    bd_list,
                    group.array_type,
                    group.array_cv_code,
                    group.dtype,
                    &decoded,
                )?;
            }
            bd_list.count = Some(bd_list.binary_data_arrays.len());
        }
        Ok(spectrum)
    }

    pub fn chromatogram(&mut self, index: usize) -> IonResult<Chromatogram> {
        if index >= self.header.chrom_count as usize {
            return Err(IonError::OutOfRange {
                index,
                count: self.header.chrom_count,
            });
        }
        let rows = self.chrom_meta_reader.read_item(index as u64)?;
        let Some(mut chromatogram) = build_one_chromatogram(&rows, index) else {
            return Err(IonError::from(format!(
                "chromatogram {index}: metadata is missing the chromatogram tag"
            )));
        };

        if let (Some(array_addresses), Some(container)) = (
            read_array_addresses_from_buffers(
                &self.chrom_entries_buf,
                &self.chrom_array_addresses,
                index,
            ),
            self.chrom_container.as_mut(),
        ) {
            let groups = group_arrays(array_addresses.as_slice())?;
            let bd_list = chromatogram
                .binary_data_array_list
                .get_or_insert_with(BinaryDataArrayList::default);
            for group in groups {
                let decoded = read_group_decoded_bytes(&group, container)?;
                attach_logical_array(
                    bd_list,
                    group.array_type,
                    group.array_cv_code,
                    group.dtype,
                    &decoded,
                )?;
            }
            bd_list.count = Some(bd_list.binary_data_arrays.len());
        }
        Ok(chromatogram)
    }
}

#[cfg(test)]
impl ScanSource for IonReader {
    fn for_each_summary(&mut self, callback: &mut dyn FnMut(usize, ScanSummary)) {
        for (index, chunk) in self
            .spec_summary_buf
            .chunks_exact(SPEC_SUMMARY_SIZE)
            .enumerate()
        {
            let summary = parse_spec_summary(chunk);
            callback(
                index,
                ScanSummary {
                    rt: summary.rt,
                    rt_unit: TimeUnit::from_code(summary.rt_unit),
                    base_peak_mz: summary.base_peak_mz,
                    selected_ion_mz: summary.selected_ion_mz,
                    base_peak_int: summary.base_peak_int,
                    total_ion_current: summary.total_ion_current,
                    ms_level: summary.ms_level,
                    polarity: summary.polarity,
                    position_x: summary.position_x,
                    position_y: summary.position_y,
                    position_z: summary.position_z,
                },
            );
        }
    }

    fn load_scan(&mut self, index: usize, mz: &mut Vec<f64>, intensity: &mut Vec<f64>) -> bool {
        let count = match usize::try_from(self.header.spectrum_count) {
            Ok(c) => c,
            Err(_) => return false,
        };
        if index >= count {
            return false;
        }
        let all_entries = self.spec_entries_buf.as_ref();
        let array_address_bytes = self.spec_array_addresses.as_ref();
        let entry_start = index * INDEX_ENTRY_BYTES;
        let Some(entry) = all_entries.get(entry_start..entry_start + INDEX_ENTRY_BYTES) else {
            return false;
        };
        read_scan_arrays(
            &mut self.spec_container,
            entry,
            array_address_bytes,
            mz,
            intensity,
        )
    }

    fn for_each_in_range<F>(&mut self, rt_min: f64, rt_max: f64, ms_level: u8, mut callback: F)
    where
        Self: Sized,
        F: FnMut(&ScanSummary, &[f64], &[f64]),
    {
        let summary_bytes = self.spec_summary_buf.as_ref();
        let _count = match usize::try_from(self.header.spectrum_count) {
            Ok(count) => count,
            Err(_) => return,
        };
        let entry_bytes = self.spec_entries_buf.as_ref();
        let array_address_bytes = self.spec_array_addresses.as_ref();
        let (container, mz_values, int_values) = (
            &mut self.spec_container as &mut dyn ContainerAccess,
            &mut self.mz_values,
            &mut self.int_values,
        );
        ScanIterator::new(
            summary_bytes,
            entry_bytes,
            array_address_bytes,
            container,
            mz_values,
            int_values,
            rt_min,
            rt_max,
            ms_level,
        )
        .run(&mut callback);
    }
}
