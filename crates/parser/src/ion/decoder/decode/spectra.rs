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
            .as_chunks::<SPEC_SUMMARY_SIZE>()
            .0
            .iter()
            .map(|c| parse_spec_summary(c))
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
            .as_chunks::<CHROM_SUMMARY_SIZE>()
            .0
            .iter()
            .map(|c| parse_chrom_summary(c))
            .collect())
    }

    fn global_meta_bytes(&self) -> IonResult<&[u8]> {
        if let Some(bytes) = self.global_meta.get() {
            return Ok(bytes);
        }
        let bytes = self.source.read(ByteRange {
            offset: self.header.off_global_meta,
            length: self.header.len_global_meta,
        })?;
        if self.options.verify_checksums {
            check_crc(&bytes, self.header.global_meta_crc32, "global_meta")?;
        }
        Ok(self.global_meta.get_or_init(|| bytes))
    }

    pub(crate) fn global_metadata_rows(&self) -> IonResult<Vec<Metadatum>> {
        parse_global_metadata(
            self.global_meta_bytes()?,
            0,
            self.header.global_meta_count,
            self.header.global_meta_numeric_count,
            self.header.global_meta_string_count,
            self.header.compression_codec,
            self.header.global_meta_uncompressed_bytes,
            self.options.decompression_limit,
            MetaColumnLayout::new().without_ids_reset(),
        )
    }

    pub(crate) fn spectrum_metadata_grouped(&self) -> IonResult<Vec<Vec<Metadatum>>> {
        self.spec_meta.read_all_grouped()
    }

    pub(crate) fn chromatogram_metadata_grouped(&self) -> IonResult<Vec<Vec<Metadatum>>> {
        self.chrom_meta.read_all_grouped()
    }

    pub(crate) fn spectrum_metadata_rows_at(&mut self, index: usize) -> IonResult<Vec<Metadatum>> {
        self.spec_meta.read_item(index as u64)
    }

    pub(crate) fn chromatogram_metadata_rows_at(
        &mut self,
        index: usize,
    ) -> IonResult<Vec<Metadatum>> {
        self.chrom_meta.read_item(index as u64)
    }

    pub fn spectrum_metadata_at(&mut self, index: usize) -> IonResult<Spectrum> {
        if index >= self.header.spectrum_count as usize {
            return Err(IonError::OutOfRange {
                index,
                count: self.header.spectrum_count,
            });
        }
        let rows = self.spectrum_metadata_rows_at(index)?;
        build_one_spectrum(&rows, index).ok_or_else(|| {
            IonError::from(format!(
                "spectrum {index}: metadata is missing the spectrum tag"
            ))
        })
    }

    pub fn chromatogram_metadata_at(&mut self, index: usize) -> IonResult<Chromatogram> {
        if index >= self.header.chrom_count as usize {
            return Err(IonError::OutOfRange {
                index,
                count: self.header.chrom_count,
            });
        }
        let rows = self.chromatogram_metadata_rows_at(index)?;
        build_one_chromatogram(&rows, index).ok_or_else(|| {
            IonError::from(format!(
                "chromatogram {index}: metadata is missing the chromatogram tag"
            ))
        })
    }
}

#[cfg(test)]
impl IonReader {
    pub(crate) fn spectrum_metadata(&self) -> IonResult<Vec<Metadatum>> {
        self.spec_meta.read_all()
    }

    pub(crate) fn chromatogram_metadata(&self) -> IonResult<Vec<Metadatum>> {
        self.chrom_meta.read_all()
    }
}

