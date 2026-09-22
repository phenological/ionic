use std::sync::Arc;

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use crate::ion::decoder::utilities::byte_source::FileSource;
use crate::{
    accessions::{
        FLOAT_16BIT, FLOAT_32BIT, FLOAT_64BIT, INT_16BIT, INT_32BIT, INT_64BIT, format_accession,
    },
    ion::{
        ByteRange, IonError, IonResult, Range,
        attr_meta::{
            ACC_ATTR_DEFAULT_INSTRUMENT_CONFIGURATION_REF, ACC_ATTR_DEFAULT_SOURCE_FILE_REF,
            ACC_ATTR_ID, ACC_ATTR_INSTRUMENT_CONFIGURATION_REF, ACC_ATTR_REF, ACC_ATTR_SAMPLE_REF,
            ACC_ATTR_START_TIME_STAMP, AccessionTail, parse_accession_tail,
        },
        decoder::utilities::{
            byte_source::{BytesSource, ReadBytes, SourceBytes},
            common::decompress_zstd,
        },
        encoder::encode::{
            CHROM_SUMMARY_SIZE, FILE_DTYPE_F16, FILE_DTYPE_F32, FILE_DTYPE_F64, FILE_DTYPE_I16,
            FILE_DTYPE_I32, FILE_DTYPE_I64, SPEC_SUMMARY_SIZE,
        },
        filter_summary::{ChromatogramSummary, SpectrumSummary},
        format::{CODEC_NONE, CODEC_ZSTD, FILE_TRAILER},
        meta_groups::meta_directory_range,
        packing::PackingId,
        utilities::{
            Header, MetaGroupReader,
            block_reader::{
                BlockReader, ContainerAccess, DefaultBlockProcessor, container_directory_range,
            },
            check_section_layout,
            children_lookup::{ChildrenLookup, DefaultMetadataPolicy, OwnerRows},
            common::get_attr_text,
            decompression_limit::DecompressionLimit,
            meta_column_layout::MetaColumnLayout,
            parse_chromatogram, parse_chromatogram_list, parse_chromatogram_list_header,
            parse_cv_and_user_params, parse_cv_list, parse_data_processing_list,
            parse_file_description, parse_global_metadata::parse_global_metadata,
            parse_instrument_list, parse_referenceable_param_group_list, parse_sample_list,
            parse_scan_settings_list, parse_software_list, parse_spectrum, parse_spectrum_list,
            parse_spectrum_list_header,
            spectrum_source::{ScanSummary, TimeUnit, f16_bits_to_f64},
            window_directory::WindowDirectory,
        },
    },
    mzml::{schema::TagId, structs::*},
};

#[cfg(test)]
pub(crate) const ACC_MZ: u32 = 1_000_514;
pub(crate) const ACC_INT: u32 = 1_000_515;
pub(crate) const INDEX_ENTRY_BYTES: usize = 16;
pub(crate) const ARRAY_ADDRESS_BYTES: usize = 32;
const DEFAULT_MAX_CACHED_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const INLINE_ARRAY_ADDRESS_CAP: usize = 8;

fn check_crc(bytes: &[u8], expected: u32, name: &str) -> IonResult<()> {
    let computed = crc32fast::hash(bytes);
    if computed != expected {
        return Err(IonError::from(format!("{name}: crc mismatch")));
    }
    Ok(())
}

fn max_address_index(entries_buf: &[u8]) -> u64 {
    let mut total = 0u64;
    for entry in entries_buf.as_chunks::<INDEX_ENTRY_BYTES>().0 {
        let first = u64::from_le_bytes(entry[0..8].try_into().unwrap());
        let count = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        total = total.max(first.saturating_add(count));
    }
    total
}

fn check_address_table(entries_buf: &[u8], table: &[u8], name: &str) -> IonResult<()> {
    let expected = max_address_index(entries_buf)
        .checked_mul(ARRAY_ADDRESS_BYTES as u64)
        .ok_or_else(|| IonError::from(format!("{name}: address count overflows the table size")))?;
    if table.len() as u64 != expected {
        return Err(IonError::from(format!(
            "{name}: record size is not {ARRAY_ADDRESS_BYTES} bytes (old format); re-encode the file from its mzML source with `ionic convert`"
        )));
    }
    Ok(())
}

fn check_array_dtypes(address_bytes: &[u8], name: &'static str) -> IonResult<()> {
    for record in address_bytes.as_chunks::<ARRAY_ADDRESS_BYTES>().0 {
        crate::ion::packing::Dtype::from_byte(record[24]).map_err(|_| IonError::BadDtype {
            dtype: record[24],
            kind: name,
        })?;
    }
    Ok(())
}

fn decompress_fixed_section(
    stored: &[u8],
    plain_len: usize,
    budget: DecompressionLimit,
    name: &str,
) -> IonResult<Arc<[u8]>> {
    let plain = decompress_zstd(stored, plain_len, budget)
        .map_err(|e| IonError::from(format!("{name}: decompression failed: {e}")))?;
    Ok(Arc::from(plain))
}

type FixedSectionBuffers = (Arc<[u8]>, Arc<[u8]>, Arc<[u8]>, Arc<[u8]>, Arc<[u8]>, Arc<[u8]>);

struct FixedSections {
    spec_summary: Arc<[u8]>,
    spec_entries: Arc<[u8]>,
    spec_array_addresses: Arc<[u8]>,
    chrom_summary: Arc<[u8]>,
    chrom_entries: Arc<[u8]>,
    chrom_array_addresses: Arc<[u8]>,
}

fn check_integrity(header: &Header, source: &Arc<dyn ReadBytes>) -> IonResult<()> {
    let layout_failures = check_section_layout(header);
    if !layout_failures.is_empty() {
        return Err(IonError::from(format!(
            "header: file integrity validation failed ({} check(s)):\n{}",
            layout_failures.len(),
            layout_failures.join("\n")
        )));
    }
    let trailer_length = FILE_TRAILER.len() as u64;
    let trailer = source.read(ByteRange {
        offset: header.total_file_size.saturating_sub(trailer_length),
        length: trailer_length,
    })?;
    if trailer[..] != FILE_TRAILER {
        return Err(IonError::from("header: missing or invalid file trailer"));
    }
    if let Some(actual_len) = source.total_len()
        && actual_len != header.total_file_size
    {
        return Err(IonError::from(format!(
            "header: total_file_size ({}) does not match source length ({actual_len})",
            header.total_file_size
        )));
    }
    Ok(())
}

fn read_fixed_sections(
    header: &Header,
    source: &Arc<dyn ReadBytes>,
    options: &ReadOptions,
) -> IonResult<FixedSections> {
    let spec_summary_buf = source.read(ByteRange {
        offset: header.off_spec_summary,
        length: header.len_spec_summary,
    })?;
    let chrom_summary_buf = source.read(ByteRange {
        offset: header.off_chrom_summary,
        length: header.len_chrom_summary,
    })?;
    let spec_entries_buf = source.read(ByteRange {
        offset: header.off_spec_entries,
        length: header.len_spec_entries,
    })?;
    let spec_array_addresses = source.read(ByteRange {
        offset: header.off_spec_array_addresses,
        length: header.len_spec_array_addresses,
    })?;
    let chrom_entries_buf = source.read(ByteRange {
        offset: header.off_chrom_entries,
        length: header.len_chrom_entries,
    })?;
    let chrom_array_addresses = source.read(ByteRange {
        offset: header.off_chrom_array_addresses,
        length: header.len_chrom_array_addresses,
    })?;

    if options.verify_checksums {
        check_crc(&spec_summary_buf, header.spec_summary_crc32, "spec_summary")?;
        check_crc(&spec_entries_buf, header.spec_entries_crc32, "spec_entries")?;
        check_crc(
            &spec_array_addresses,
            header.spec_array_addresses_crc32,
            "spec_array_addresses",
        )?;
        check_crc(
            &chrom_summary_buf,
            header.chrom_summary_crc32,
            "chrom_summary",
        )?;
        check_crc(
            &chrom_entries_buf,
            header.chrom_entries_crc32,
            "chrom_entries",
        )?;
        check_crc(
            &chrom_array_addresses,
            header.chrom_array_addresses_crc32,
            "chrom_array_addresses",
        )?;
    }

    let (
        spec_summary_plain,
        spec_entries_plain,
        spec_array_addresses_plain,
        chrom_summary_plain,
        chrom_entries_plain,
        chrom_array_addresses_plain,
    ): FixedSectionBuffers = if header.compression_codec == CODEC_ZSTD {
        let spec_summary_plain = decompress_fixed_section(
            &spec_summary_buf,
            header.spectrum_count as usize * SPEC_SUMMARY_SIZE,
            options.decompression_limit,
            "spec_summary",
        )?;
        let spec_entries_plain = decompress_fixed_section(
            &spec_entries_buf,
            header.spectrum_count as usize * INDEX_ENTRY_BYTES,
            options.decompression_limit,
            "spec_entries",
        )?;
        let spec_array_addresses_plain = decompress_fixed_section(
            &spec_array_addresses,
            max_address_index(&spec_entries_plain) as usize * ARRAY_ADDRESS_BYTES,
            options.decompression_limit,
            "spec_array_addresses",
        )?;
        let chrom_summary_plain = decompress_fixed_section(
            &chrom_summary_buf,
            header.chrom_count as usize * CHROM_SUMMARY_SIZE,
            options.decompression_limit,
            "chrom_summary",
        )?;
        let chrom_entries_plain = decompress_fixed_section(
            &chrom_entries_buf,
            header.chrom_count as usize * INDEX_ENTRY_BYTES,
            options.decompression_limit,
            "chrom_entries",
        )?;
        let chrom_array_addresses_plain = decompress_fixed_section(
            &chrom_array_addresses,
            max_address_index(&chrom_entries_plain) as usize * ARRAY_ADDRESS_BYTES,
            options.decompression_limit,
            "chrom_array_addresses",
        )?;
        (
            spec_summary_plain,
            spec_entries_plain,
            spec_array_addresses_plain,
            chrom_summary_plain,
            chrom_entries_plain,
            chrom_array_addresses_plain,
        )
    } else {
        (
            spec_summary_buf.into_arc(),
            spec_entries_buf.into_arc(),
            spec_array_addresses.into_arc(),
            chrom_summary_buf.into_arc(),
            chrom_entries_buf.into_arc(),
            chrom_array_addresses.into_arc(),
        )
    };

    check_address_table(
        &spec_entries_plain,
        &spec_array_addresses_plain,
        "spec_array_addresses",
    )?;
    check_address_table(
        &chrom_entries_plain,
        &chrom_array_addresses_plain,
        "chrom_array_addresses",
    )?;

    check_array_dtypes(&spec_array_addresses_plain, "spec array dtype")?;
    check_array_dtypes(&chrom_array_addresses_plain, "chrom array dtype")?;

    Ok(FixedSections {
        spec_summary: spec_summary_plain,
        spec_entries: spec_entries_plain,
        spec_array_addresses: spec_array_addresses_plain,
        chrom_summary: chrom_summary_plain,
        chrom_entries: chrom_entries_plain,
        chrom_array_addresses: chrom_array_addresses_plain,
    })
}

mod arrays;
mod spectra;
mod to_mzml;
mod windows;

pub(crate) use arrays::ArrayAddress;
pub(crate) use arrays::{
    ArrayGroup, address_read_params, dtype_stride, group_arrays, parse_array_address,
    read_array_addresses_from_buffers, unfilter_array_bytes,
};
pub use to_mzml::{Metadatum, MetadatumValue};
pub use windows::{DataXY, ScanQuery, Select, Window};
#[cfg(test)]
pub(crate) use windows::ItemKind;

#[derive(Debug, Clone)]
pub struct ReadOptions {
    pub max_cached_bytes: usize,
    pub verify_checksums: bool,
    pub parallel: bool,
    pub decompression_limit: DecompressionLimit,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            max_cached_bytes: DEFAULT_MAX_CACHED_BYTES,
            verify_checksums: true,
            parallel: true,
            decompression_limit: DecompressionLimit::default(),
        }
    }
}

pub(crate) enum WindowDirectoryCache {
    Unloaded,
    Missing,
    BadChecksum,
    Malformed(String),
    Loaded(WindowDirectory),
}

pub struct IonReader {
    pub(crate) header: Header,
    pub(crate) source: Arc<dyn ReadBytes>,
    pub(crate) spec_window_directory: WindowDirectoryCache,
    #[allow(dead_code)]
    pub(crate) chrom_window_directory: WindowDirectoryCache,
    pub(crate) spec_summary_buf: Arc<[u8]>,
    pub(crate) chrom_summary_buf: Arc<[u8]>,
    pub(crate) spec_entries_buf: Arc<[u8]>,
    pub(crate) spec_array_addresses: Arc<[u8]>,
    pub(crate) chrom_entries_buf: Arc<[u8]>,
    pub(crate) chrom_array_addresses: Arc<[u8]>,
    pub(crate) global_meta: std::cell::OnceCell<SourceBytes>,
    pub(crate) spec_container: BlockReader<DefaultBlockProcessor>,
    pub(crate) chrom_container: Option<BlockReader<DefaultBlockProcessor>>,
    pub(crate) spec_meta: MetaGroupReader,
    pub(crate) chrom_meta: MetaGroupReader,
    pub(crate) options: ReadOptions,
    pub(crate) spec_rt_finite_ascending: std::cell::Cell<Option<bool>>,
}

impl IonReader {
    pub fn from_bytes(bytes: &[u8], options: &ReadOptions) -> IonResult<Self> {
        let source = Arc::new(BytesSource::new(Arc::from(bytes))) as Arc<dyn ReadBytes>;
        Self::new(source, options)
    }

    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    pub fn open(path: impl AsRef<std::path::Path>, options: &ReadOptions) -> IonResult<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|e| IonError::from(e.to_string()))?;
        let map = unsafe { memmap2::Mmap::map(&file).map_err(|e| IonError::from(e.to_string()))? };
        let source = Arc::new(FileSource::new(map)) as Arc<dyn ReadBytes>;
        Self::new(source, options)
    }

    pub fn new(source: Arc<dyn ReadBytes>, options: &ReadOptions) -> IonResult<Self> {
        let header_buf = source.read(ByteRange {
            offset: 0,
            length: 1024,
        })?;
        let header = Header::parse(&header_buf)?;
        let block_packing_id = PackingId::from_byte(header.default_array_filter)?;

        if options.verify_checksums {
            check_integrity(&header, &source)?;
        }

        let fixed = read_fixed_sections(&header, &source, options)?;
        let meta_column_layout = MetaColumnLayout::new();
        let spec_meta = MetaGroupReader::open(
            source.clone(),
            header.spec_meta_section(),
            header.compression_codec,
            options,
            meta_column_layout,
        )?;
        let chrom_meta = MetaGroupReader::open(
            source.clone(),
            header.chrom_meta_section(),
            header.compression_codec,
            options,
            meta_column_layout,
        )?;

        let spec_container = BlockReader::new(
            source.clone(),
            header.off_spec_container,
            header.len_spec_container,
            header.spec_block_count,
            header.spec_directory_crc32,
            header.compression_codec,
            block_packing_id,
            options.verify_checksums,
            "spec",
            DefaultBlockProcessor,
            options.max_cached_bytes,
            options.decompression_limit,
        )?;

        let chrom_container = if header.chrom_block_count > 0 && header.len_chrom_container > 0 {
            Some(BlockReader::new(
                source.clone(),
                header.off_chrom_container,
                header.len_chrom_container,
                header.chrom_block_count,
                header.chrom_directory_crc32,
                header.compression_codec,
                block_packing_id,
                options.verify_checksums,
                "chrom",
                DefaultBlockProcessor,
                options.max_cached_bytes,
                options.decompression_limit,
            )?)
        } else {
            None
        };

        Ok(Self {
            header,
            source,
            spec_window_directory: WindowDirectoryCache::Unloaded,
            chrom_window_directory: WindowDirectoryCache::Unloaded,
            spec_summary_buf: fixed.spec_summary,
            chrom_summary_buf: fixed.chrom_summary,
            spec_entries_buf: fixed.spec_entries,
            spec_array_addresses: fixed.spec_array_addresses,
            chrom_entries_buf: fixed.chrom_entries,
            chrom_array_addresses: fixed.chrom_array_addresses,
            global_meta: std::cell::OnceCell::new(),
            spec_container,
            chrom_container,
            spec_meta,
            chrom_meta,
            options: options.clone(),
            spec_rt_finite_ascending: std::cell::Cell::new(None),
        })
    }

    #[inline]
    pub fn format_version(&self) -> u16 {
        self.header.format_version
    }

    #[inline]
    pub fn spectrum_count(&self) -> u64 {
        self.header.spectrum_count
    }

    #[inline]
    pub fn chromatogram_count(&self) -> u64 {
        self.header.chrom_count
    }

    pub(crate) fn ensure_spec_window_directory(&mut self) -> IonResult<()> {
        if matches!(self.spec_window_directory, WindowDirectoryCache::Unloaded) {
            self.spec_window_directory = match self.load_spec_window_directory() {
                Ok(index) => WindowDirectoryCache::Loaded(index),
                Err(IonError::MissingSpectrumBounds) => WindowDirectoryCache::Missing,
                Err(IonError::BadSpectrumBoundsChecksum) => WindowDirectoryCache::BadChecksum,
                Err(IonError::MalformedSpectrumBounds(reason)) => {
                    WindowDirectoryCache::Malformed(reason)
                }
                Err(other) => WindowDirectoryCache::Malformed(other.to_string()),
            };
        }
        match &self.spec_window_directory {
            WindowDirectoryCache::Loaded(_) => Ok(()),
            WindowDirectoryCache::Missing => Err(IonError::MissingSpectrumBounds),
            WindowDirectoryCache::BadChecksum => Err(IonError::BadSpectrumBoundsChecksum),
            WindowDirectoryCache::Malformed(reason) => {
                Err(IonError::MalformedSpectrumBounds(reason.clone()))
            }
            WindowDirectoryCache::Unloaded => Err(IonError::MissingSpectrumBounds),
        }
    }

    #[cfg(test)]
    pub(crate) fn ensure_chrom_window_directory(&mut self) {
        if !matches!(self.chrom_window_directory, WindowDirectoryCache::Unloaded) {
            return;
        }
        self.chrom_window_directory = match self.load_chrom_window_directory() {
            Ok(index) => WindowDirectoryCache::Loaded(index),
            Err(IonError::MissingChromatogramBounds) => WindowDirectoryCache::Missing,
            Err(IonError::BadChromatogramBoundsChecksum) => WindowDirectoryCache::BadChecksum,
            Err(IonError::MalformedChromatogramBounds(reason)) => {
                WindowDirectoryCache::Malformed(reason)
            }
            Err(other) => WindowDirectoryCache::Malformed(other.to_string()),
        };
    }

    fn load_spec_window_directory(&self) -> IonResult<WindowDirectory> {
        if self.header.len_spec_window_directory == 0 {
            return Err(IonError::MissingSpectrumBounds);
        }
        let spec_array_address_count =
            self.spec_array_addresses.len() as u64 / ARRAY_ADDRESS_BYTES as u64;
        let bytes = self
            .source
            .read(ByteRange {
                offset: self.header.off_spec_window_directory,
                length: self.header.len_spec_window_directory,
            })
            .map_err(|error| IonError::MalformedSpectrumBounds(format!("read failed: {error}")))?;
        if crc32fast::hash(&bytes) != self.header.spec_window_directory_crc32 {
            return Err(IonError::BadSpectrumBoundsChecksum);
        }
        let decompressed = self
            .decompress_window_directory(
                &bytes,
                self.header.plain_len_spec_window_directory as usize,
            )
            .map_err(|error| {
                IonError::MalformedSpectrumBounds(format!("decompression failed: {error}"))
            })?;
        WindowDirectory::from_bytes(
            &decompressed,
            spec_array_address_count,
            self.header.spectrum_count,
        )
        .map_err(|error| IonError::MalformedSpectrumBounds(error.to_string()))
    }

    #[cfg(test)]
    fn load_chrom_window_directory(&self) -> IonResult<WindowDirectory> {
        if self.header.len_chrom_window_directory == 0 {
            return Err(IonError::MissingChromatogramBounds);
        }
        let chrom_array_address_count =
            self.chrom_array_addresses.len() as u64 / ARRAY_ADDRESS_BYTES as u64;
        let bytes = self
            .source
            .read(ByteRange {
                offset: self.header.off_chrom_window_directory,
                length: self.header.len_chrom_window_directory,
            })
            .map_err(|error| {
                IonError::MalformedChromatogramBounds(format!("read failed: {error}"))
            })?;
        if crc32fast::hash(&bytes) != self.header.chrom_window_directory_crc32 {
            return Err(IonError::BadChromatogramBoundsChecksum);
        }
        let decompressed = self
            .decompress_window_directory(
                &bytes,
                self.header.plain_len_chrom_window_directory as usize,
            )
            .map_err(|error| {
                IonError::MalformedChromatogramBounds(format!("decompression failed: {error}"))
            })?;
        WindowDirectory::from_bytes(
            &decompressed,
            chrom_array_address_count,
            self.header.chrom_count,
        )
        .map_err(|error| IonError::MalformedChromatogramBounds(error.to_string()))
    }

    fn decompress_window_directory(&self, bytes: &[u8], plain_len: usize) -> IonResult<Vec<u8>> {
        match self.header.compression_codec {
            CODEC_NONE => {
                if bytes.len() != plain_len {
                    return Err("window bounds: uncompressed length mismatch".into());
                }
                Ok(bytes.to_vec())
            }
            CODEC_ZSTD => decompress_zstd(bytes, plain_len, self.options.decompression_limit),
            _ => Err("window bounds: unsupported codec".into()),
        }
    }
}

pub fn merge_ranges(ranges: &mut Vec<ByteRange>, gap: u64) {
    ranges.retain(|range| range.length > 0);
    ranges.sort_unstable_by_key(|range| (range.offset, range.length));
    let mut kept = 0;
    for index in 0..ranges.len() {
        let range = ranges[index];
        if kept == 0 {
            ranges[0] = range;
            kept = 1;
            continue;
        }
        let last = ranges[kept - 1];
        let last_end = last.offset.saturating_add(last.length);
        let range_end = range.offset.saturating_add(range.length);
        if range.offset <= last_end.saturating_add(gap) {
            ranges[kept - 1].length = range_end.max(last_end) - last.offset;
            continue;
        }
        ranges[kept] = range;
        kept += 1;
    }
    ranges.truncate(kept);
}

pub fn header_ranges(header_bytes: &[u8]) -> IonResult<Vec<ByteRange>> {
    if header_bytes.len() < 1024 {
        return Err("header_ranges: needs at least 1024 header bytes".into());
    }
    let header = Header::parse(&header_bytes[..1024])?;
    open_byte_ranges(&header)
}

pub(crate) fn open_byte_ranges(header: &Header) -> IonResult<Vec<ByteRange>> {
    let mut ranges = vec![
        ByteRange {
            offset: 0,
            length: 1024,
        },
        ByteRange {
            offset: header.off_spec_summary,
            length: header.len_spec_summary,
        },
        ByteRange {
            offset: header.off_chrom_summary,
            length: header.len_chrom_summary,
        },
        ByteRange {
            offset: header.off_spec_entries,
            length: header.len_spec_entries,
        },
        ByteRange {
            offset: header.off_spec_array_addresses,
            length: header.len_spec_array_addresses,
        },
        ByteRange {
            offset: header.off_chrom_entries,
            length: header.len_chrom_entries,
        },
        ByteRange {
            offset: header.off_chrom_array_addresses,
            length: header.len_chrom_array_addresses,
        },
        meta_directory_range(
            ByteRange {
                offset: header.off_spec_meta,
                length: header.len_spec_meta,
            },
            header.spec_meta_group_count,
        )?,
        meta_directory_range(
            ByteRange {
                offset: header.off_chrom_meta,
                length: header.len_chrom_meta,
            },
            header.chrom_meta_group_count,
        )?,
    ];

    if header.len_spec_window_directory > 0 {
        ranges.push(ByteRange {
            offset: header.off_spec_window_directory,
            length: header.len_spec_window_directory,
        });
    }
    if header.len_chrom_window_directory > 0 {
        ranges.push(ByteRange {
            offset: header.off_chrom_window_directory,
            length: header.len_chrom_window_directory,
        });
    }

    let spec_blocks = usize::try_from(header.spec_block_count)
        .map_err(|_| IonError::from("spec: block count too large for this platform"))?;
    ranges.push(container_directory_range(
        header.off_spec_container,
        header.len_spec_container,
        spec_blocks,
        "spec",
    )?);

    if header.chrom_block_count > 0 && header.len_chrom_container > 0 {
        let chrom_blocks = usize::try_from(header.chrom_block_count)
            .map_err(|_| IonError::from("chrom: block count too large for this platform"))?;
        ranges.push(container_directory_range(
            header.off_chrom_container,
            header.len_chrom_container,
            chrom_blocks,
            "chrom",
        )?);
    }

    let trailer_length = FILE_TRAILER.len() as u64;
    if header.total_file_size > trailer_length {
        ranges.push(ByteRange {
            offset: header.total_file_size - trailer_length,
            length: trailer_length,
        });
    }

    Ok(ranges)
}

#[cfg(test)]
mod tests;
