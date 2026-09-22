use cosmoz::Encoder;

pub(crate) struct SectionPlacement {
    pub(crate) offset: u64,
    pub(crate) length: u64,
    pub(crate) plain_len: u64,
    pub(crate) crc32: u32,
}

use crate::{
    accessions::{INTENSITY_ARRAY, MZ_ARRAY, TIME_ARRAY},
    ion::{
        IonError, IonResult,
        encoder::{
            encode::{
                CHROM_SUMMARY_SIZE, EncodedArrayAddress, SPEC_SUMMARY_SIZE, WriteOptions,
                allow_compression_level, check_spectrum_mz_order, check_spectrum_rt_order,
                encode_single_array, extract_chrom_summary, spec_summary_from_spectrum,
                window_ranges_for_item, write_array_windows,
            },
            scan_stream::ScanStream,
            utilities::{
                BlockWriter, ContainerSummary, DefaultCompressor, SectionChunk,
                meta_collector::{
                    ArrayPolicy, GroupedSection, LOCAL_LIST_NODE_ID, MetaCollector, MetaGrouper,
                    MzmlListItem, array_type_accession_from_binary_data_array,
                    compress_bytes_if_enabled, new_meta_encoder, serialize_global_meta_with_counts,
                },
                output::{SectionStorage, WriteBytes},
                tables::{
                    ArrayAddressTable, IndexTable, SummaryTable, WindowDirectory, WindowEntry,
                    write_aligned,
                },
            },
        },
        format::{FILE_TRAILER, HEADER_SIZE},
        header::Header,
        meta_groups::METADATA_GROUP_SIZE,
        utilities::EmitAttributes,
        windowing::WindowRange,
    },
    mzml::structs::{
        BinaryDataArray, BinaryDataArrayList, Chromatogram, ChromatogramList, MzML, Spectrum,
        SpectrumList,
    },
};

fn spec_summary_bytes(spec: &Spectrum) -> [u8; SPEC_SUMMARY_SIZE] {
    let s = spec_summary_from_spectrum(spec);
    let mut buf = [0u8; SPEC_SUMMARY_SIZE];
    buf[0..8].copy_from_slice(&s.rt.to_le_bytes());
    buf[8..16].copy_from_slice(&s.base_peak_mz.to_le_bytes());
    buf[16..24].copy_from_slice(&s.selected_ion_mz.to_le_bytes());
    buf[24..32].copy_from_slice(&s.base_peak_int.to_le_bytes());
    buf[32..40].copy_from_slice(&s.total_ion_current.to_le_bytes());
    buf[40] = s.ms_level;
    buf[41] = s.polarity;
    buf[42..46].copy_from_slice(&s.position_x.to_le_bytes());
    buf[46..50].copy_from_slice(&s.position_y.to_le_bytes());
    buf[50..54].copy_from_slice(&s.position_z.to_le_bytes());
    buf[54] = s.rt_unit;
    buf
}

fn chrom_summary_bytes(chrom: &Chromatogram) -> [u8; CHROM_SUMMARY_SIZE] {
    let s = extract_chrom_summary(chrom);
    let mut buf = [0u8; CHROM_SUMMARY_SIZE];
    buf[0..8].copy_from_slice(&s.lowest_mz.to_le_bytes());
    buf[8..16].copy_from_slice(&s.highest_mz.to_le_bytes());
    buf[16..24].copy_from_slice(&s.lowest_wavelength.to_le_bytes());
    buf[24..32].copy_from_slice(&s.highest_wavelength.to_le_bytes());
    buf[32..40].copy_from_slice(&s.lowest_ion_mobility.to_le_bytes());
    buf[40..48].copy_from_slice(&s.highest_ion_mobility.to_le_bytes());
    buf[48] = s.polarity;
    buf
}

struct ArrayWriteState<'a> {
    addresses: &'a mut ArrayAddressTable,
    cursor: &'a mut u64,
    seen: &'a mut Vec<u32>,
}

impl ArrayWriteState<'_> {
    fn emit(&mut self, address: &EncodedArrayAddress) -> IonResult<()> {
        if address.accession != 0 && !self.seen.contains(&address.accession) {
            self.seen.push(address.accession);
        }
        self.addresses.push(
            address.element_offset,
            address.element_count,
            address.block_id,
            address.accession,
            address.dtype,
            address.array_filter,
            address.encoded_len,
            address.continues_previous_segment,
            address.array_cv_code,
        )?;
        *self.cursor += 1;
        Ok(())
    }
}

fn write_single_array(
    output: &mut dyn WriteBytes,
    bda: &BinaryDataArray,
    config: WriteOptions,
    policy: ArrayPolicy,
    container: &mut BlockWriter<DefaultCompressor>,
    state: &mut ArrayWriteState<'_>,
) -> IonResult<()> {
    let Some(address) = encode_single_array(output, bda, config, policy, container)? else {
        return Ok(());
    };
    state.emit(&address)?;
    Ok(())
}

fn write_windowed_array(
    output: &mut dyn WriteBytes,
    bda: &BinaryDataArray,
    config: WriteOptions,
    policy: ArrayPolicy,
    windows: &[WindowRange],
    container: &mut BlockWriter<DefaultCompressor>,
    state: &mut ArrayWriteState<'_>,
) -> IonResult<()> {
    let addresses = write_array_windows(output, bda, config, policy, windows, container)?;
    for address in &addresses {
        state.emit(address)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_arrays_for<T>(
    output: &mut dyn WriteBytes,
    item: &T,
    spectrum_index: u32,
    config: WriteOptions,
    policy: ArrayPolicy,
    windowable: bool,
    container: &mut BlockWriter<DefaultCompressor>,
    index: &mut IndexTable,
    state: &mut ArrayWriteState<'_>,
    window_directory: &mut WindowDirectory,
) -> IonResult<()>
where
    T: HasArrayList,
{
    let address_start = *state.cursor;

    if let Some(list) = item.array_list() {
        let windows = if windowable {
            window_ranges_for_item(&list.binary_data_arrays, config, policy, config.mz_window)?
        } else {
            None
        };

        let mut mz_address_start: Option<u64> = None;
        let mut intensity_address_start: Option<u64> = None;

        for bda in &list.binary_data_arrays {
            let accession = array_type_accession_from_binary_data_array(bda);
            let address_start = *state.cursor;
            match windows.as_ref() {
                Some(windows) => {
                    write_windowed_array(output, bda, config, policy, windows, container, state)?
                }
                None => write_single_array(output, bda, config, policy, container, state)?,
            }
            if *state.cursor > address_start {
                if accession == policy.x_array_accession {
                    mz_address_start = Some(address_start);
                } else if accession == INTENSITY_ARRAY {
                    intensity_address_start = Some(address_start);
                }
            }
        }

        if let (Some(mz_address_start), Some(intensity_address_start)) =
            (mz_address_start, intensity_address_start)
        {
            push_window_entries(
                window_directory,
                spectrum_index,
                mz_address_start,
                intensity_address_start,
                windows.as_deref(),
            );
        }
    }

    let address_count = *state.cursor - address_start;
    index.push(address_start, address_count)?;
    Ok(())
}

fn push_window_entries(
    window_directory: &mut WindowDirectory,
    spectrum_index: u32,
    mz_address_start: u64,
    intensity_address_start: u64,
    windows: Option<&[WindowRange]>,
) {
    match windows {
        Some(windows) => {
            for (offset, window) in windows.iter().enumerate() {
                window_directory.push(
                    window.window_index,
                    WindowEntry {
                        spectrum_index,
                        mz_address: (mz_address_start + offset as u64) as u32,
                        intensity_address: (intensity_address_start + offset as u64) as u32,
                    },
                );
            }
        }
        None => window_directory.push(
            0,
            WindowEntry {
                spectrum_index,
                mz_address: mz_address_start as u32,
                intensity_address: intensity_address_start as u32,
            },
        ),
    }
}

pub(crate) trait HasArrayList {
    fn array_list(&self) -> Option<&BinaryDataArrayList>;
}

impl HasArrayList for Spectrum {
    fn array_list(&self) -> Option<&BinaryDataArrayList> {
        self.binary_data_array_list.as_ref()
    }
}

impl HasArrayList for Chromatogram {
    fn array_list(&self) -> Option<&BinaryDataArrayList> {
        self.binary_data_array_list.as_ref()
    }
}

struct ItemStream {
    summary: SummaryTable,
    index: IndexTable,
    addresses: ArrayAddressTable,
    grouper: MetaGrouper,
    window_directory: WindowDirectory,
    windowable: bool,
    count: usize,
    address_cursor: u64,
    seen: Vec<u32>,
    container: BlockWriter<DefaultCompressor>,
    container_offset: Option<u64>,
    container_summary: Option<ContainerSummary>,
}

struct StreamParts {
    grouped: GroupedSection,
    summary: SectionChunk,
    index: SectionChunk,
    addresses: SectionChunk,
    window_directory: SectionChunk,
    count: usize,
    type_count: usize,
    container_offset: u64,
    block_count: u64,
    container_total: u64,
    directory_crc32: u32,
}

impl ItemStream {
    fn new(
        summary_hint: usize,
        summary_size: usize,
        table_hint: usize,
        config: WriteOptions,
        windowable: bool,
    ) -> IonResult<Self> {
        let table = |capacity: usize| -> IonResult<SectionChunk> {
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            if config.section_storage == SectionStorage::Disk && config.compression_level > 0 {
                return SectionChunk::spilled(config.compression_level);
            }
            Ok(SectionChunk::memory(capacity))
        };

        let compressor = config.compression_mode()?;
        let builder = BlockWriter::new(config.block_size, compressor, config.block_packing_id());
        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        let builder = builder.window_major()?;
        let container = if config.parallel {
            builder
        } else {
            builder.force_sequential()
        };

        Ok(Self {
            summary: SummaryTable::new(table(summary_hint * summary_size)?),
            index: IndexTable::new(table(table_hint * 16)?),
            addresses: ArrayAddressTable::new(table(table_hint * 64)?),
            grouper: MetaGrouper::new(
                METADATA_GROUP_SIZE,
                config.compression_level,
                SectionChunk::memory(0),
            )?,
            window_directory: WindowDirectory::new(),
            windowable,
            count: 0,
            address_cursor: 0,
            seen: Vec::with_capacity(8),
            container,
            container_offset: None,
            container_summary: None,
        })
    }

    #[cfg(test)]
    fn pending_bytes(&self) -> usize {
        self.container.pending_bytes()
    }

    #[cfg(test)]
    fn set_max_pending_bytes(&mut self, value: usize) {
        self.container.set_max_pending_bytes(value);
    }

    fn is_sealed(&self) -> bool {
        self.container_summary.is_some()
    }

    fn ensure_container_offset(&mut self, output: &mut dyn WriteBytes) -> IonResult<()> {
        if self.container_offset.is_none() {
            self.container_offset = Some(write_aligned(output, &[])?);
        }
        Ok(())
    }

    fn seal_container(&mut self, output: &mut dyn WriteBytes) -> IonResult<()> {
        if self.container_summary.is_some() {
            return Ok(());
        }
        self.ensure_container_offset(output)?;
        self.container_summary = Some(self.container.finish(output)?);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn add<T, L>(
        &mut self,
        output: &mut dyn WriteBytes,
        item: &T,
        config: WriteOptions,
        policy: ArrayPolicy,
        list_id: u32,
        list_schema: Option<&L>,
        collector: &mut MetaCollector,
        summary: &[u8],
    ) -> IonResult<()>
    where
        T: HasArrayList + MzmlListItem,
        L: EmitAttributes,
    {
        self.ensure_container_offset(output)?;
        let mut state = ArrayWriteState {
            addresses: &mut self.addresses,
            cursor: &mut self.address_cursor,
            seen: &mut self.seen,
        };
        encode_arrays_for(
            output,
            item,
            self.count as u32,
            config,
            policy,
            self.windowable,
            &mut self.container,
            &mut self.index,
            &mut state,
            &mut self.window_directory,
        )?;
        self.summary.push(summary)?;
        collector.add_item(
            item,
            self.count,
            list_id,
            list_schema,
            policy,
            &mut self.grouper,
        )?;
        self.count += 1;
        Ok(())
    }

    fn finish(mut self, output: &mut dyn WriteBytes) -> IonResult<StreamParts> {
        self.seal_container(output)?;
        let summary = self
            .container_summary
            .expect("seal_container always sets container_summary");
        Ok(StreamParts {
            grouped: self.grouper.finish()?,
            summary: self.summary.finish(),
            index: self.index.finish(),
            addresses: self.addresses.finish(),
            window_directory: self.window_directory.finish()?,
            count: self.count,
            type_count: self.seen.len(),
            container_offset: self.container_offset.unwrap_or(0),
            block_count: summary.block_count as u64,
            container_total: summary.total_bytes,
            directory_crc32: summary.directory_crc32,
        })
    }
}

fn write_chunk(output: &mut dyn WriteBytes, section: SectionChunk) -> IonResult<(u64, u64, u32)> {
    let bytes = section.into_vec()?;
    let crc32 = crc32fast::hash(&bytes);
    let offset = write_aligned(output, &bytes)?;
    Ok((offset, bytes.len() as u64, crc32))
}

fn write_chunk_maybe_compressed(
    output: &mut dyn WriteBytes,
    section: SectionChunk,
    compression_level: u8,
    encoder: &mut Encoder,
) -> IonResult<(u64, u64, u32)> {
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    if section.is_spilled() {
        return section.copy_into(output);
    }
    let raw = section.into_vec()?;
    if raw.is_empty() {
        let offset = write_aligned(output, &raw)?;
        return Ok((offset, 0, crc32fast::hash(&raw)));
    }
    let stored = compress_bytes_if_enabled(raw, compression_level, encoder);
    let crc32 = crc32fast::hash(&stored);
    let offset = write_aligned(output, &stored)?;
    Ok((offset, stored.len() as u64, crc32))
}

fn schema_of_spectrum_list(list: &SpectrumList) -> SpectrumList {
    SpectrumList {
        count: list.count,
        default_data_processing_ref: list.default_data_processing_ref.clone(),
        spectra: Vec::new(),
    }
}

fn schema_of_chromatogram_list(list: &ChromatogramList) -> ChromatogramList {
    ChromatogramList {
        count: list.count,
        default_data_processing_ref: list.default_data_processing_ref.clone(),
        chromatograms: Vec::new(),
    }
}

pub(crate) struct IonEncoder {
    config: WriteOptions,
    compressor: Encoder,
    collector: MetaCollector,
    spec_list_id: u32,
    chrom_list_id: u32,
    spec_stream: ItemStream,
    chrom_stream: ItemStream,
    spec_schema: Option<SpectrumList>,
    chrom_schema: Option<ChromatogramList>,
    last_spec_rt: f64,
}

impl IonEncoder {
    pub(crate) fn begin(
        output: &mut dyn WriteBytes,
        metadata: &MzML,
        options: &WriteOptions,
    ) -> IonResult<Self> {
        let options = *options;
        allow_compression_level(options.compression_level)?;
        output.write(&[0u8; HEADER_SIZE])?;

        let compressor = new_meta_encoder(options.compression_level)?;

        let mut writer = Self {
            config: options,
            compressor,
            collector: MetaCollector::new(),
            spec_list_id: LOCAL_LIST_NODE_ID,
            chrom_list_id: LOCAL_LIST_NODE_ID,
            spec_stream: ItemStream::new(256, SPEC_SUMMARY_SIZE, 256, options, true)?,
            chrom_stream: ItemStream::new(32, CHROM_SUMMARY_SIZE, 32, options, false)?,
            spec_schema: None,
            chrom_schema: None,
            last_spec_rt: f64::NEG_INFINITY,
        };
        writer.set_metadata(metadata);
        Ok(writer)
    }

    fn set_metadata(&mut self, metadata: &MzML) {
        self.spec_schema = metadata.run.spectrum_list.as_ref().map(schema_of_spectrum_list);
        self.chrom_schema = metadata
            .run
            .chromatogram_list
            .as_ref()
            .map(schema_of_chromatogram_list);
    }

    pub(crate) fn push_spectrum(
        &mut self,
        output: &mut dyn WriteBytes,
        spectrum: &Spectrum,
    ) -> IonResult<()> {
        if self.spec_stream.is_sealed() {
            return Err(IonError::from(
                "cannot push a spectrum after a chromatogram has been written",
            ));
        }
        if let Some(list) = spectrum.array_list() {
            check_spectrum_mz_order(&list.binary_data_arrays, self.spec_stream.count)?;
        }
        let summary = spec_summary_bytes(spectrum);
        check_spectrum_rt_order(&summary, self.spec_stream.count, &mut self.last_spec_rt)?;
        self.spec_stream.add(
            output,
            spectrum,
            self.config,
            self.config.array_policy(MZ_ARRAY),
            self.spec_list_id,
            self.spec_schema.as_ref(),
            &mut self.collector,
            &summary,
        )
    }

    pub(crate) fn push_chromatogram(
        &mut self,
        output: &mut dyn WriteBytes,
        chromatogram: &Chromatogram,
    ) -> IonResult<()> {
        self.spec_stream.seal_container(output)?;
        let summary = chrom_summary_bytes(chromatogram);
        self.chrom_stream.add(
            output,
            chromatogram,
            self.config,
            self.config.array_policy(TIME_ARRAY),
            self.chrom_list_id,
            self.chrom_schema.as_ref(),
            &mut self.collector,
            &summary,
        )
    }

    pub(crate) fn write_mzml(&mut self, output: &mut dyn WriteBytes, mzml: &MzML) -> IonResult<()> {
        if let Some(list) = &mzml.run.spectrum_list {
            for spectrum in &list.spectra {
                self.push_spectrum(output, spectrum)?;
            }
        }
        if let Some(list) = &mzml.run.chromatogram_list {
            for chromatogram in &list.chromatograms {
                self.push_chromatogram(output, chromatogram)?;
            }
        }
        Ok(())
    }

    pub(crate) fn write_stream(
        &mut self,
        output: &mut dyn WriteBytes,
        scans: &mut dyn ScanStream,
    ) -> IonResult<()> {
        let metadata = scans.metadata()?;
        self.set_metadata(&metadata);
        while let Some(spectrum) = scans.next_spectrum()? {
            self.push_spectrum(output, &spectrum)?;
        }

        let metadata = scans.metadata()?;
        self.set_metadata(&metadata);
        while let Some(chromatogram) = scans.next_chromatogram()? {
            self.push_chromatogram(output, &chromatogram)?;
        }

        let metadata = scans.metadata()?;
        self.finish(output, &metadata)
    }

    fn write_window_directory(
        &mut self,
        output: &mut dyn WriteBytes,
        bounds: SectionChunk,
    ) -> IonResult<SectionPlacement> {
        let raw = bounds.into_vec()?;
        let plain_len = raw.len() as u64;
        if raw.is_empty() {
            return Ok(SectionPlacement {
                offset: 0,
                length: 0,
                plain_len: 0,
                crc32: crc32fast::hash(&[]),
            });
        }
        let stored =
            compress_bytes_if_enabled(raw, self.config.compression_level, &mut self.compressor);
        let crc32 = crc32fast::hash(&stored);
        let offset = write_aligned(output, &stored)?;
        let length = stored.len() as u64;
        Ok(SectionPlacement {
            offset,
            length,
            plain_len,
            crc32,
        })
    }

    pub(crate) fn finish(&mut self, output: &mut dyn WriteBytes, mzml: &MzML) -> IonResult<()> {
        let (global_meta, global_counts) = self.collector.collect_global_meta(mzml);
        let raw_global = serialize_global_meta_with_counts(&global_counts, &global_meta)?;
        let global_uncompressed = raw_global.len() as u64;
        let global_bytes =
            compress_bytes_if_enabled(raw_global, self.config.compression_level, &mut self.compressor);

        let spec = std::mem::replace(
            &mut self.spec_stream,
            ItemStream::new(1, SPEC_SUMMARY_SIZE, 1, self.config, true)?,
        )
        .finish(output)?;
        let chrom = std::mem::replace(
            &mut self.chrom_stream,
            ItemStream::new(1, CHROM_SUMMARY_SIZE, 1, self.config, false)?,
        )
        .finish(output)?;

        let spec_meta_crc32 = spec.grouped.crc32;
        let chrom_meta_crc32 = chrom.grouped.crc32;
        let global_meta_crc32 = crc32fast::hash(&global_bytes);

        let spec_meta_len = spec.grouped.byte_len;
        let chrom_meta_len = chrom.grouped.byte_len;

        let compression_level = self.config.compression_level;

        let (off_spec_summary, len_spec_summary_stored, spec_summary_crc32) =
            write_chunk_maybe_compressed(
                output,
                spec.summary,
                compression_level,
                &mut self.compressor,
            )?;
        let (off_spec_entries, len_spec_entries_stored, spec_entries_crc32) =
            write_chunk_maybe_compressed(
                output,
                spec.index,
                compression_level,
                &mut self.compressor,
            )?;
        let (off_spec_array_addresses, len_spec_array_addresses_stored, spec_array_addresses_crc32) =
            write_chunk_maybe_compressed(
                output,
                spec.addresses,
                compression_level,
                &mut self.compressor,
            )?;
        let (off_chrom_summary, len_chrom_summary_stored, chrom_summary_crc32) =
            write_chunk_maybe_compressed(
                output,
                chrom.summary,
                compression_level,
                &mut self.compressor,
            )?;
        let (off_chrom_entries, len_chrom_entries_stored, chrom_entries_crc32) =
            write_chunk_maybe_compressed(
                output,
                chrom.index,
                compression_level,
                &mut self.compressor,
            )?;
        let (
            off_chrom_array_addresses,
            len_chrom_array_addresses_stored,
            chrom_array_addresses_crc32,
        ) = write_chunk_maybe_compressed(
            output,
            chrom.addresses,
            compression_level,
            &mut self.compressor,
        )?;
        let off_spec_meta = write_chunk(output, spec.grouped.section)?.0;
        let off_chrom_meta = write_chunk(output, chrom.grouped.section)?.0;
        let off_global_meta = write_aligned(output, &global_bytes)?;

        let a1 = self.write_window_directory(output, spec.window_directory)?;
        let b1 = self.write_window_directory(output, chrom.window_directory)?;

        output.write(&FILE_TRAILER)?;
        let total_file_size = output.position()?;

        let header = Header {
            compression_codec: self.config.codec_id(),
            compression_level: self.config.compression_level,
            default_array_filter: self.config.array_filter_id(),
            target_block_uncompressed_bytes: self.config.block_size as u64,

            off_spec_entries,
            len_spec_entries: len_spec_entries_stored,
            off_spec_array_addresses,
            len_spec_array_addresses: len_spec_array_addresses_stored,
            off_chrom_entries,
            len_chrom_entries: len_chrom_entries_stored,
            off_chrom_array_addresses,
            len_chrom_array_addresses: len_chrom_array_addresses_stored,
            off_spec_meta,
            len_spec_meta: spec_meta_len,
            off_chrom_meta,
            len_chrom_meta: chrom_meta_len,
            off_global_meta,
            len_global_meta: global_bytes.len() as u64,
            off_spec_container: spec.container_offset,
            len_spec_container: spec.container_total,
            off_chrom_container: chrom.container_offset,
            len_chrom_container: chrom.container_total,

            spec_block_count: spec.block_count,
            chrom_block_count: chrom.block_count,
            spectrum_count: spec.count as u64,
            chrom_count: chrom.count as u64,

            spec_meta_count: spec.grouped.row_count,
            spec_meta_numeric_count: spec.grouped.numeric_count,
            spec_meta_string_count: spec.grouped.string_count,
            chrom_meta_count: chrom.grouped.row_count,
            chrom_meta_numeric_count: chrom.grouped.numeric_count,
            chrom_meta_string_count: chrom.grouped.string_count,
            global_meta_count: global_meta.ref_codes.len() as u64,
            global_meta_numeric_count: global_meta.numeric_values.len() as u64,
            global_meta_string_count: global_meta.string_offsets.len() as u64,
            spec_array_type_count: spec.type_count as u64,
            chrom_array_type_count: chrom.type_count as u64,

            spec_meta_uncompressed_bytes: spec.grouped.uncompressed_size,
            chrom_meta_uncompressed_bytes: chrom.grouped.uncompressed_size,
            global_meta_uncompressed_bytes: global_uncompressed,

            meta_group_size: METADATA_GROUP_SIZE,
            spec_meta_group_count: spec.grouped.group_count,
            chrom_meta_group_count: chrom.grouped.group_count,

            off_spec_summary,
            len_spec_summary: len_spec_summary_stored,
            off_chrom_summary,
            len_chrom_summary: len_chrom_summary_stored,

            total_file_size,

            spec_directory_crc32: spec.directory_crc32,
            chrom_directory_crc32: chrom.directory_crc32,

            off_spec_window_directory: a1.offset,
            len_spec_window_directory: a1.length,
            off_chrom_window_directory: b1.offset,
            len_chrom_window_directory: b1.length,
            plain_len_spec_window_directory: a1.plain_len,
            plain_len_chrom_window_directory: b1.plain_len,
            spec_window_directory_crc32: a1.crc32,
            chrom_window_directory_crc32: b1.crc32,

            spec_summary_crc32,
            spec_entries_crc32,
            spec_array_addresses_crc32,
            chrom_summary_crc32,
            chrom_entries_crc32,
            chrom_array_addresses_crc32,
            spec_meta_crc32,
            chrom_meta_crc32,
            global_meta_crc32,
            target_mz_window: self.config.mz_window.round() as u32,
            header_crc32: 0,
            ..Header::default()
        };

        let mut header_bytes = [0u8; HEADER_SIZE];
        header.write(&mut header_bytes);
        let crc = crc32fast::hash(&header_bytes[0..1020]);
        header_bytes[1020..1024].copy_from_slice(&crc.to_le_bytes());
        output.patch(0, &header_bytes)
    }
}

pub(crate) fn write_mzml_to_ion(
    mzml: &MzML,
    options: &WriteOptions,
    output: &mut dyn WriteBytes,
) -> IonResult<()> {
    let mut writer = IonEncoder::begin(output, mzml, options)?;
    writer.write_mzml(output, mzml)?;
    writer.finish(output, mzml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ion::decoder::decode::IonReader,
        mzml::structs::{BinaryDataArrayList, CvParam, NumericArray},
    };

    fn make_bda(accession: &str, name: &str, data: Vec<f64>) -> BinaryDataArray {
        BinaryDataArray {
            cv_params: vec![CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(accession.to_string()),
                name: name.to_string(),
                value: None,
                unit_cv_ref: None,
                unit_name: None,
                unit_accession: None,
            }],
            binary: Some(NumericArray::F64(data)),
            ..Default::default()
        }
    }

    fn make_spectrum(id: usize, rt: f64) -> Spectrum {
        let mz = vec![100.0 + id as f64, 101.0 + id as f64, 102.0 + id as f64];
        let intensity = vec![1.0, 2.0, 3.0];
        Spectrum {
            id: format!("scan={id}"),
            index: Some(id as u32),
            cv_params: vec![CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some("MS:1000016".to_string()),
                name: "scan start time".to_string(),
                value: Some(rt.to_string()),
                unit_cv_ref: Some("UO".to_string()),
                unit_accession: Some("UO:0000010".to_string()),
                unit_name: Some("second".to_string()),
            }],
            binary_data_array_list: Some(BinaryDataArrayList {
                count: Some(2),
                binary_data_arrays: vec![
                    make_bda("MS:1000514", "m/z array", mz),
                    make_bda("MS:1000515", "intensity array", intensity),
                ],
            }),
            ..Default::default()
        }
    }

    #[test]
    fn push_spectrum_streams_instead_of_buffering_the_whole_file() {
        let block_size = 256;
        let config = WriteOptions {
            compression_level: 0,
            block_size,
            parallel: false,
            ..Default::default()
        };
        let mut output = Vec::new();
        let mut writer = IonEncoder::begin(&mut output, &MzML::default(), &config).unwrap();
        writer.spec_stream.set_max_pending_bytes(1);

        let spectrum_count = 500;
        let mut max_pending_bytes_seen = 0usize;
        for i in 0..spectrum_count {
            writer
                .push_spectrum(&mut output, &make_spectrum(i, i as f64))
                .unwrap();
            max_pending_bytes_seen = max_pending_bytes_seen.max(writer.spec_stream.pending_bytes());
        }

        assert!(
            max_pending_bytes_seen <= block_size,
            "pending bytes ({max_pending_bytes_seen}) exceeded one block ({block_size}) \
             after pushing {spectrum_count} spectra"
        );

        writer.finish(&mut output, &MzML::default()).unwrap();

        let mut reader =
            IonReader::from_bytes(&output, &crate::ion::decoder::decode::ReadOptions::default())
                .unwrap();
        assert_eq!(reader.spectrum_count(), spectrum_count as u64);
        for i in [0usize, spectrum_count / 2, spectrum_count - 1] {
            let spectrum = reader.spectrum_metadata_at(i).unwrap();
            let mz = reader.spectrum_array(i, crate::ion::ArrayKind::Mz).unwrap();
            assert_eq!(spectrum.index, Some(i as u32));
            assert_eq!(mz, vec![100.0 + i as f64, 101.0 + i as f64, 102.0 + i as f64]);
        }
    }
}
