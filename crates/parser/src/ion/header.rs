use crate::ion::{
    IonResult,
    decoder::decode::INDEX_ENTRY_BYTES,
    encoder::encode::{CHROM_SUMMARY_SIZE, SPEC_SUMMARY_SIZE},
    format::{
        CODEC_ZSTD, CURRENT_VERSION, FILE_SIGNATURE, HEADER_SIZE, allow_compression, allow_version,
    },
};

const BLOCK_DIRECTORY_ENTRY_SIZE_U64: u64 = 32;

pub const HEADER_FORMAT_VERSION_OFFSET: usize = 9;

pub(crate) const HEADER_CODEC_ID: usize = 11;
pub(crate) const HEADER_COMPRESSION_LEVEL: usize = 12;
pub(crate) const HEADER_ARRAY_FILTER_ID: usize = 13;
pub(crate) const HEADER_TARGET_BLOCK_SIZE: usize = 16;
pub(crate) const HEADER_TARGET_MZ_WINDOW: usize = 24;

pub(crate) const HEADER_OFFSET_SPEC_WINDOW_DIRECTORY: usize = 32;
pub(crate) const HEADER_LEN_SPEC_WINDOW_DIRECTORY: usize = 40;
pub(crate) const HEADER_OFFSET_SPEC_SUMMARY: usize = 48;
pub(crate) const HEADER_LEN_SPEC_SUMMARY: usize = 56;
pub(crate) const HEADER_OFFSET_SPEC_ENTRIES: usize = 64;
pub(crate) const HEADER_LEN_SPEC_ENTRIES: usize = 72;
pub(crate) const HEADER_OFFSET_SPEC_ARRAY_ADDRESSES: usize = 80;
pub(crate) const HEADER_LEN_SPEC_ARRAY_ADDRESSES: usize = 88;

pub(crate) const HEADER_OFFSET_CHROM_WINDOW_DIRECTORY: usize = 96;
pub(crate) const HEADER_LEN_CHROM_WINDOW_DIRECTORY: usize = 104;
pub(crate) const HEADER_OFFSET_CHROM_SUMMARY: usize = 112;
pub(crate) const HEADER_LEN_CHROM_SUMMARY: usize = 120;
pub(crate) const HEADER_OFFSET_CHROM_ENTRIES: usize = 128;
pub(crate) const HEADER_LEN_CHROM_ENTRIES: usize = 136;
pub(crate) const HEADER_OFFSET_CHROM_ARRAY_ADDRESSES: usize = 144;
pub(crate) const HEADER_LEN_CHROM_ARRAY_ADDRESSES: usize = 152;

pub(crate) const HEADER_OFFSET_SPEC_META: usize = 160;
pub(crate) const HEADER_LEN_SPEC_META: usize = 168;
pub(crate) const HEADER_OFFSET_CHROM_META: usize = 176;
pub(crate) const HEADER_LEN_CHROM_META: usize = 184;
pub(crate) const HEADER_OFFSET_GLOBAL_META: usize = 192;
pub(crate) const HEADER_LEN_GLOBAL_META: usize = 200;

pub(crate) const HEADER_OFFSET_PACKED_SPECTRA: usize = 208;
pub(crate) const HEADER_LEN_PACKED_SPECTRA: usize = 216;
pub(crate) const HEADER_OFFSET_PACKED_CHROMS: usize = 224;
pub(crate) const HEADER_LEN_PACKED_CHROMS: usize = 232;

pub(crate) const HEADER_SPECTRUM_BLOCK_COUNT: usize = 240;
pub(crate) const HEADER_CHROM_BLOCK_COUNT: usize = 248;
pub(crate) const HEADER_SPECTRUM_COUNT: usize = 256;
pub(crate) const HEADER_CHROM_COUNT: usize = 264;

pub(crate) const HEADER_SPEC_ARRAY_TYPE_COUNT: usize = 272;
pub(crate) const HEADER_CHROM_ARRAY_TYPE_COUNT: usize = 280;

pub(crate) const HEADER_SPEC_META_ROW_COUNT: usize = 288;
pub(crate) const HEADER_SPEC_META_NUMERIC_COUNT: usize = 296;
pub(crate) const HEADER_SPEC_META_STRING_COUNT: usize = 304;
pub(crate) const HEADER_CHROM_META_ROW_COUNT: usize = 312;
pub(crate) const HEADER_CHROM_META_NUMERIC_COUNT: usize = 320;
pub(crate) const HEADER_CHROM_META_STRING_COUNT: usize = 328;
pub(crate) const HEADER_GLOBAL_META_ROW_COUNT: usize = 336;
pub(crate) const HEADER_GLOBAL_META_NUMERIC_COUNT: usize = 344;
pub(crate) const HEADER_GLOBAL_META_STRING_COUNT: usize = 352;

pub(crate) const HEADER_SPEC_META_UNCOMPRESSED_SIZE: usize = 360;
pub(crate) const HEADER_CHROM_META_UNCOMPRESSED_SIZE: usize = 368;
pub(crate) const HEADER_GLOBAL_META_UNCOMPRESSED_SIZE: usize = 376;

pub(crate) const HEADER_PLAIN_LEN_SPEC_WINDOW_DIRECTORY: usize = 384;
pub(crate) const HEADER_PLAIN_LEN_CHROM_WINDOW_DIRECTORY: usize = 392;

pub(crate) const HEADER_TOTAL_FILE_SIZE: usize = 400;
pub(crate) const HEADER_META_GROUP_SIZE: usize = 408;
pub(crate) const HEADER_SPEC_META_GROUP_COUNT: usize = 416;
pub(crate) const HEADER_CHROM_META_GROUP_COUNT: usize = 424;

pub(crate) const RESERVED_BLOCK_START: usize = 432;
pub(crate) const RESERVED_BLOCK_SIZE: usize = 536;

pub(crate) const HEADER_SPEC_WINDOW_DIRECTORY_CRC32: usize = 968;
pub(crate) const HEADER_SPEC_SUMMARY_CRC32: usize = 972;
pub(crate) const HEADER_SPEC_ENTRIES_CRC32: usize = 976;
pub(crate) const HEADER_SPEC_ARRAY_ADDRESSES_CRC32: usize = 980;
pub(crate) const HEADER_CHROM_WINDOW_DIRECTORY_CRC32: usize = 984;
pub(crate) const HEADER_CHROM_SUMMARY_CRC32: usize = 988;
pub(crate) const HEADER_CHROM_ENTRIES_CRC32: usize = 992;
pub(crate) const HEADER_CHROM_ARRAY_ADDRESSES_CRC32: usize = 996;
pub(crate) const HEADER_SPEC_DIRECTORY_CRC32: usize = 1000;
pub(crate) const HEADER_CHROM_DIRECTORY_CRC32: usize = 1004;
pub(crate) const HEADER_SPEC_META_CRC32: usize = 1008;
pub(crate) const HEADER_CHROM_META_CRC32: usize = 1012;
pub(crate) const HEADER_GLOBAL_META_CRC32: usize = 1016;
pub(crate) const HEADER_CRC32: usize = 1020;

const _: () = assert!(HEADER_TARGET_MZ_WINDOW == 24);
const _: () = assert!(HEADER_OFFSET_SPEC_WINDOW_DIRECTORY == 32);
const _: () = assert!(HEADER_OFFSET_SPEC_SUMMARY == 48);
const _: () = assert!(HEADER_OFFSET_SPEC_ENTRIES == 64);
const _: () = assert!(HEADER_OFFSET_SPEC_ARRAY_ADDRESSES == 80);
const _: () = assert!(HEADER_OFFSET_CHROM_WINDOW_DIRECTORY == 96);
const _: () = assert!(HEADER_OFFSET_CHROM_SUMMARY == 112);
const _: () = assert!(HEADER_OFFSET_CHROM_ENTRIES == 128);
const _: () = assert!(HEADER_OFFSET_CHROM_ARRAY_ADDRESSES == 144);
const _: () = assert!(HEADER_PLAIN_LEN_SPEC_WINDOW_DIRECTORY == 384);
const _: () = assert!(HEADER_PLAIN_LEN_CHROM_WINDOW_DIRECTORY == 392);
const _: () = assert!(RESERVED_BLOCK_START == 432);
const _: () = assert!(RESERVED_BLOCK_SIZE == 536);
const _: () = assert!(RESERVED_BLOCK_START + RESERVED_BLOCK_SIZE == 968);
const _: () = assert!(HEADER_SPEC_WINDOW_DIRECTORY_CRC32 == 968);
const _: () = assert!(HEADER_SPEC_SUMMARY_CRC32 == 972);
const _: () = assert!(HEADER_SPEC_ENTRIES_CRC32 == 976);
const _: () = assert!(HEADER_SPEC_ARRAY_ADDRESSES_CRC32 == 980);
const _: () = assert!(HEADER_CHROM_WINDOW_DIRECTORY_CRC32 == 984);
const _: () = assert!(HEADER_CHROM_SUMMARY_CRC32 == 988);
const _: () = assert!(HEADER_CHROM_ENTRIES_CRC32 == 992);
const _: () = assert!(HEADER_CHROM_ARRAY_ADDRESSES_CRC32 == 996);
const _: () = assert!(HEADER_SPEC_DIRECTORY_CRC32 == 1000);
const _: () = assert!(HEADER_CHROM_DIRECTORY_CRC32 == 1004);
const _: () = assert!(HEADER_SPEC_META_CRC32 == 1008);
const _: () = assert!(HEADER_CHROM_META_CRC32 == 1012);
const _: () = assert!(HEADER_GLOBAL_META_CRC32 == 1016);
const _: () = assert!(HEADER_CRC32 == 1020);

#[derive(Debug, Clone)]
pub(crate) struct Header {
    #[allow(dead_code)]
    pub(crate) file_signature: [u8; 8],
    #[allow(dead_code)]
    pub(crate) endianness_flag: u8,
    pub(crate) format_version: u16,
    pub(crate) compression_codec: u8,
    pub(crate) compression_level: u8,
    pub(crate) default_array_filter: u8,
    pub(crate) target_block_uncompressed_bytes: u64,
    pub(crate) off_spec_summary: u64,
    pub(crate) len_spec_summary: u64,
    pub(crate) off_chrom_summary: u64,
    pub(crate) len_chrom_summary: u64,
    pub(crate) off_spec_entries: u64,
    pub(crate) len_spec_entries: u64,
    pub(crate) off_spec_array_addresses: u64,
    pub(crate) len_spec_array_addresses: u64,
    pub(crate) off_chrom_entries: u64,
    pub(crate) len_chrom_entries: u64,
    pub(crate) off_chrom_array_addresses: u64,
    pub(crate) len_chrom_array_addresses: u64,
    pub(crate) off_spec_meta: u64,
    pub(crate) len_spec_meta: u64,
    pub(crate) off_chrom_meta: u64,
    pub(crate) len_chrom_meta: u64,
    pub(crate) off_global_meta: u64,
    pub(crate) len_global_meta: u64,
    pub(crate) off_spec_container: u64,
    pub(crate) len_spec_container: u64,
    pub(crate) off_chrom_container: u64,
    pub(crate) len_chrom_container: u64,
    pub(crate) spec_block_count: u64,
    pub(crate) chrom_block_count: u64,
    pub(crate) spectrum_count: u64,
    pub(crate) chrom_count: u64,
    pub(crate) spec_meta_count: u64,
    pub(crate) spec_meta_numeric_count: u64,
    pub(crate) spec_meta_string_count: u64,
    pub(crate) chrom_meta_count: u64,
    pub(crate) chrom_meta_numeric_count: u64,
    pub(crate) chrom_meta_string_count: u64,
    pub(crate) global_meta_count: u64,
    pub(crate) global_meta_numeric_count: u64,
    pub(crate) global_meta_string_count: u64,
    pub(crate) spec_array_type_count: u64,
    pub(crate) chrom_array_type_count: u64,
    pub(crate) spec_meta_uncompressed_bytes: u64,
    pub(crate) chrom_meta_uncompressed_bytes: u64,
    pub(crate) global_meta_uncompressed_bytes: u64,
    pub(crate) total_file_size: u64,
    pub(crate) meta_group_size: u32,
    pub(crate) spec_meta_group_count: u64,
    pub(crate) chrom_meta_group_count: u64,
    pub(crate) spec_summary_crc32: u32,
    pub(crate) spec_entries_crc32: u32,
    pub(crate) spec_array_addresses_crc32: u32,
    pub(crate) chrom_summary_crc32: u32,
    pub(crate) chrom_entries_crc32: u32,
    pub(crate) chrom_array_addresses_crc32: u32,
    pub(crate) target_mz_window: u32,
    pub(crate) spec_directory_crc32: u32,
    pub(crate) chrom_directory_crc32: u32,
    pub(crate) off_spec_window_directory: u64,
    pub(crate) len_spec_window_directory: u64,
    pub(crate) off_chrom_window_directory: u64,
    pub(crate) len_chrom_window_directory: u64,
    pub(crate) plain_len_spec_window_directory: u64,
    pub(crate) plain_len_chrom_window_directory: u64,
    pub(crate) spec_window_directory_crc32: u32,
    pub(crate) chrom_window_directory_crc32: u32,
    pub(crate) spec_meta_crc32: u32,
    pub(crate) chrom_meta_crc32: u32,
    pub(crate) global_meta_crc32: u32,
    pub(crate) header_crc32: u32,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            file_signature: [0u8; 8],
            endianness_flag: 0,
            format_version: 0,
            compression_codec: 0,
            compression_level: 0,
            default_array_filter: 0,
            target_block_uncompressed_bytes: 0,
            off_spec_summary: 0,
            len_spec_summary: 0,
            off_chrom_summary: 0,
            len_chrom_summary: 0,
            off_spec_entries: 0,
            len_spec_entries: 0,
            off_spec_array_addresses: 0,
            len_spec_array_addresses: 0,
            off_chrom_entries: 0,
            len_chrom_entries: 0,
            off_chrom_array_addresses: 0,
            len_chrom_array_addresses: 0,
            off_spec_meta: 0,
            len_spec_meta: 0,
            off_chrom_meta: 0,
            len_chrom_meta: 0,
            off_global_meta: 0,
            len_global_meta: 0,
            off_spec_container: 0,
            len_spec_container: 0,
            off_chrom_container: 0,
            len_chrom_container: 0,
            spec_block_count: 0,
            chrom_block_count: 0,
            spectrum_count: 0,
            chrom_count: 0,
            spec_meta_count: 0,
            spec_meta_numeric_count: 0,
            spec_meta_string_count: 0,
            chrom_meta_count: 0,
            chrom_meta_numeric_count: 0,
            chrom_meta_string_count: 0,
            global_meta_count: 0,
            global_meta_numeric_count: 0,
            global_meta_string_count: 0,
            spec_array_type_count: 0,
            chrom_array_type_count: 0,
            spec_meta_uncompressed_bytes: 0,
            chrom_meta_uncompressed_bytes: 0,
            global_meta_uncompressed_bytes: 0,
            total_file_size: 0,
            meta_group_size: 0,
            spec_meta_group_count: 0,
            chrom_meta_group_count: 0,
            spec_summary_crc32: 0,
            spec_entries_crc32: 0,
            spec_array_addresses_crc32: 0,
            chrom_summary_crc32: 0,
            chrom_entries_crc32: 0,
            chrom_array_addresses_crc32: 0,
            target_mz_window: 0,
            spec_directory_crc32: 0,
            chrom_directory_crc32: 0,
            off_spec_window_directory: 0,
            len_spec_window_directory: 0,
            off_chrom_window_directory: 0,
            len_chrom_window_directory: 0,
            plain_len_spec_window_directory: 0,
            plain_len_chrom_window_directory: 0,
            spec_window_directory_crc32: 0,
            chrom_window_directory_crc32: 0,
            spec_meta_crc32: 0,
            chrom_meta_crc32: 0,
            global_meta_crc32: 0,
            header_crc32: 0,
        }
    }
}

impl Header {
    pub(crate) fn parse(bytes: &[u8]) -> IonResult<Self> {
        if bytes.len() < HEADER_SIZE {
            return Err("header: file too small".into());
        }

        let h = &bytes[..HEADER_SIZE];

        let file_signature = <[u8; 8]>::try_from(&h[0..8]).unwrap();
        if file_signature != FILE_SIGNATURE {
            return Err("header: invalid file signature".into());
        }
        let endianness_flag = h[8];
        if endianness_flag != 0 {
            return Err("header: expected little-endian endianness_flag=0".into());
        }

        let format_version = version_of(bytes).unwrap();
        allow_version(format_version)?;
        let compression_codec = h[HEADER_CODEC_ID];
        let compression_level = h[HEADER_COMPRESSION_LEVEL];
        allow_compression(compression_codec, compression_level)?;
        let default_array_filter = h[HEADER_ARRAY_FILTER_ID];

        if h[14] != 0 || h[15] != 0 {
            return Err("header: reserved bytes at 14..16 must be zero".into());
        }

        let target_block_uncompressed_bytes = read_u64_at(h, HEADER_TARGET_BLOCK_SIZE);

        let off_spec_entries = read_u64_at(h, HEADER_OFFSET_SPEC_ENTRIES);
        let len_spec_entries = read_u64_at(h, HEADER_LEN_SPEC_ENTRIES);
        let off_spec_array_addresses = read_u64_at(h, HEADER_OFFSET_SPEC_ARRAY_ADDRESSES);
        let len_spec_array_addresses = read_u64_at(h, HEADER_LEN_SPEC_ARRAY_ADDRESSES);
        let off_chrom_entries = read_u64_at(h, HEADER_OFFSET_CHROM_ENTRIES);
        let len_chrom_entries = read_u64_at(h, HEADER_LEN_CHROM_ENTRIES);
        let off_chrom_array_addresses = read_u64_at(h, HEADER_OFFSET_CHROM_ARRAY_ADDRESSES);
        let len_chrom_array_addresses = read_u64_at(h, HEADER_LEN_CHROM_ARRAY_ADDRESSES);
        let off_spec_meta = read_u64_at(h, HEADER_OFFSET_SPEC_META);
        let len_spec_meta = read_u64_at(h, HEADER_LEN_SPEC_META);
        let off_chrom_meta = read_u64_at(h, HEADER_OFFSET_CHROM_META);
        let len_chrom_meta = read_u64_at(h, HEADER_LEN_CHROM_META);
        let off_global_meta = read_u64_at(h, HEADER_OFFSET_GLOBAL_META);
        let len_global_meta = read_u64_at(h, HEADER_LEN_GLOBAL_META);
        let off_spec_container = read_u64_at(h, HEADER_OFFSET_PACKED_SPECTRA);
        let len_spec_container = read_u64_at(h, HEADER_LEN_PACKED_SPECTRA);
        let off_chrom_container = read_u64_at(h, HEADER_OFFSET_PACKED_CHROMS);
        let len_chrom_container = read_u64_at(h, HEADER_LEN_PACKED_CHROMS);

        let spec_block_count = read_u64_at(h, HEADER_SPECTRUM_BLOCK_COUNT);
        let chrom_block_count = read_u64_at(h, HEADER_CHROM_BLOCK_COUNT);
        let spectrum_count = read_u64_at(h, HEADER_SPECTRUM_COUNT);
        let chrom_count = read_u64_at(h, HEADER_CHROM_COUNT);

        let spec_meta_count = read_u64_at(h, HEADER_SPEC_META_ROW_COUNT);
        let spec_meta_numeric_count = read_u64_at(h, HEADER_SPEC_META_NUMERIC_COUNT);
        let spec_meta_string_count = read_u64_at(h, HEADER_SPEC_META_STRING_COUNT);
        let chrom_meta_count = read_u64_at(h, HEADER_CHROM_META_ROW_COUNT);
        let chrom_meta_numeric_count = read_u64_at(h, HEADER_CHROM_META_NUMERIC_COUNT);
        let chrom_meta_string_count = read_u64_at(h, HEADER_CHROM_META_STRING_COUNT);
        let global_meta_count = read_u64_at(h, HEADER_GLOBAL_META_ROW_COUNT);
        let global_meta_numeric_count = read_u64_at(h, HEADER_GLOBAL_META_NUMERIC_COUNT);
        let global_meta_string_count = read_u64_at(h, HEADER_GLOBAL_META_STRING_COUNT);
        let spec_array_type_count = read_u64_at(h, HEADER_SPEC_ARRAY_TYPE_COUNT);
        let chrom_array_type_count = read_u64_at(h, HEADER_CHROM_ARRAY_TYPE_COUNT);

        let spec_meta_uncompressed_bytes = read_u64_at(h, HEADER_SPEC_META_UNCOMPRESSED_SIZE);
        let chrom_meta_uncompressed_bytes = read_u64_at(h, HEADER_CHROM_META_UNCOMPRESSED_SIZE);
        let global_meta_uncompressed_bytes = read_u64_at(h, HEADER_GLOBAL_META_UNCOMPRESSED_SIZE);

        let off_spec_summary = read_u64_at(h, HEADER_OFFSET_SPEC_SUMMARY);
        let len_spec_summary = read_u64_at(h, HEADER_LEN_SPEC_SUMMARY);
        let off_chrom_summary = read_u64_at(h, HEADER_OFFSET_CHROM_SUMMARY);
        let len_chrom_summary = read_u64_at(h, HEADER_LEN_CHROM_SUMMARY);
        let total_file_size = read_u64_at(h, HEADER_TOTAL_FILE_SIZE);

        let meta_group_size_stored = read_u64_at(h, HEADER_META_GROUP_SIZE);
        if meta_group_size_stored > u32::MAX as u64 {
            return Err("header: meta_group_size exceeds u32 range".into());
        }
        let meta_group_size = meta_group_size_stored as u32;
        let spec_meta_group_count = read_u64_at(h, HEADER_SPEC_META_GROUP_COUNT);
        let chrom_meta_group_count = read_u64_at(h, HEADER_CHROM_META_GROUP_COUNT);

        let off_spec_window_directory = read_u64_at(h, HEADER_OFFSET_SPEC_WINDOW_DIRECTORY);
        let len_spec_window_directory = read_u64_at(h, HEADER_LEN_SPEC_WINDOW_DIRECTORY);
        let off_chrom_window_directory = read_u64_at(h, HEADER_OFFSET_CHROM_WINDOW_DIRECTORY);
        let len_chrom_window_directory = read_u64_at(h, HEADER_LEN_CHROM_WINDOW_DIRECTORY);
        let spec_window_directory_crc32 = read_u32_at(h, HEADER_SPEC_WINDOW_DIRECTORY_CRC32);
        let chrom_window_directory_crc32 = read_u32_at(h, HEADER_CHROM_WINDOW_DIRECTORY_CRC32);

        let plain_len_spec_window_directory =
            read_u64_at(h, HEADER_PLAIN_LEN_SPEC_WINDOW_DIRECTORY);
        let plain_len_chrom_window_directory =
            read_u64_at(h, HEADER_PLAIN_LEN_CHROM_WINDOW_DIRECTORY);

        let spec_summary_crc32 = read_u32_at(h, HEADER_SPEC_SUMMARY_CRC32);
        let spec_entries_crc32 = read_u32_at(h, HEADER_SPEC_ENTRIES_CRC32);
        let spec_array_addresses_crc32 = read_u32_at(h, HEADER_SPEC_ARRAY_ADDRESSES_CRC32);
        let chrom_summary_crc32 = read_u32_at(h, HEADER_CHROM_SUMMARY_CRC32);
        let chrom_entries_crc32 = read_u32_at(h, HEADER_CHROM_ENTRIES_CRC32);
        let chrom_array_addresses_crc32 = read_u32_at(h, HEADER_CHROM_ARRAY_ADDRESSES_CRC32);

        let target_mz_window = read_u32_at(h, HEADER_TARGET_MZ_WINDOW);

        if h[RESERVED_BLOCK_START..RESERVED_BLOCK_START + RESERVED_BLOCK_SIZE]
            .iter()
            .any(|&b| b != 0)
        {
            return Err("header: reserved block 432..968 must be all zeros".into());
        }

        let spec_directory_crc32 = read_u32_at(h, HEADER_SPEC_DIRECTORY_CRC32);
        let chrom_directory_crc32 = read_u32_at(h, HEADER_CHROM_DIRECTORY_CRC32);
        let spec_meta_crc32 = read_u32_at(h, HEADER_SPEC_META_CRC32);
        let chrom_meta_crc32 = read_u32_at(h, HEADER_CHROM_META_CRC32);
        let global_meta_crc32 = read_u32_at(h, HEADER_GLOBAL_META_CRC32);
        let header_crc32 = read_u32_at(h, HEADER_CRC32);

        let header = Header {
            file_signature,
            endianness_flag,
            format_version,
            compression_codec,
            compression_level,
            default_array_filter,
            target_block_uncompressed_bytes,
            off_spec_entries,
            len_spec_entries,
            off_spec_array_addresses,
            len_spec_array_addresses,
            off_chrom_entries,
            len_chrom_entries,
            off_chrom_array_addresses,
            len_chrom_array_addresses,
            off_spec_meta,
            len_spec_meta,
            off_chrom_meta,
            len_chrom_meta,
            off_global_meta,
            len_global_meta,
            off_spec_container,
            len_spec_container,
            off_chrom_container,
            len_chrom_container,
            spec_block_count,
            chrom_block_count,
            spectrum_count,
            chrom_count,
            spec_meta_count,
            spec_meta_numeric_count,
            spec_meta_string_count,
            chrom_meta_count,
            chrom_meta_numeric_count,
            chrom_meta_string_count,
            global_meta_count,
            global_meta_numeric_count,
            global_meta_string_count,
            spec_array_type_count,
            chrom_array_type_count,
            spec_meta_uncompressed_bytes,
            chrom_meta_uncompressed_bytes,
            global_meta_uncompressed_bytes,
            off_spec_summary,
            len_spec_summary,
            off_chrom_summary,
            len_chrom_summary,
            total_file_size,
            meta_group_size,
            spec_meta_group_count,
            chrom_meta_group_count,
            spec_summary_crc32,
            spec_entries_crc32,
            spec_array_addresses_crc32,
            chrom_summary_crc32,
            chrom_entries_crc32,
            chrom_array_addresses_crc32,
            target_mz_window,
            spec_directory_crc32,
            chrom_directory_crc32,
            off_spec_window_directory,
            len_spec_window_directory,
            off_chrom_window_directory,
            len_chrom_window_directory,
            plain_len_spec_window_directory,
            plain_len_chrom_window_directory,
            spec_window_directory_crc32,
            chrom_window_directory_crc32,
            spec_meta_crc32,
            chrom_meta_crc32,
            global_meta_crc32,
            header_crc32,
        };

        let computed_header_crc = crc32fast::hash(&h[0..HEADER_CRC32]);
        if computed_header_crc != header.header_crc32 {
            return Err(format!(
                "header: header_crc32 mismatch (stored={:#010x}, computed={:#010x})",
                header.header_crc32, computed_header_crc
            )
            .into());
        }

        Ok(header)
    }

    pub(crate) fn write(&self, buf: &mut [u8]) {
        buf[0..FILE_SIGNATURE.len()].copy_from_slice(&FILE_SIGNATURE);
        buf[8] = 0u8;
        write_u16_at(
            buf,
            HEADER_FORMAT_VERSION_OFFSET,
            crate::ion::format::CURRENT_VERSION,
        );

        write_u8_at(buf, HEADER_CODEC_ID, self.compression_codec);
        write_u8_at(buf, HEADER_COMPRESSION_LEVEL, self.compression_level);
        write_u8_at(buf, HEADER_ARRAY_FILTER_ID, self.default_array_filter);

        write_u64_at(
            buf,
            HEADER_TARGET_BLOCK_SIZE,
            self.target_block_uncompressed_bytes,
        );

        write_u64_at(buf, HEADER_OFFSET_SPEC_ENTRIES, self.off_spec_entries);
        write_u64_at(buf, HEADER_LEN_SPEC_ENTRIES, self.len_spec_entries);
        write_u64_at(
            buf,
            HEADER_OFFSET_SPEC_ARRAY_ADDRESSES,
            self.off_spec_array_addresses,
        );
        write_u64_at(
            buf,
            HEADER_LEN_SPEC_ARRAY_ADDRESSES,
            self.len_spec_array_addresses,
        );
        write_u64_at(buf, HEADER_OFFSET_CHROM_ENTRIES, self.off_chrom_entries);
        write_u64_at(buf, HEADER_LEN_CHROM_ENTRIES, self.len_chrom_entries);
        write_u64_at(
            buf,
            HEADER_OFFSET_CHROM_ARRAY_ADDRESSES,
            self.off_chrom_array_addresses,
        );
        write_u64_at(
            buf,
            HEADER_LEN_CHROM_ARRAY_ADDRESSES,
            self.len_chrom_array_addresses,
        );
        write_u64_at(buf, HEADER_OFFSET_SPEC_META, self.off_spec_meta);
        write_u64_at(buf, HEADER_LEN_SPEC_META, self.len_spec_meta);
        write_u64_at(buf, HEADER_OFFSET_CHROM_META, self.off_chrom_meta);
        write_u64_at(buf, HEADER_LEN_CHROM_META, self.len_chrom_meta);
        write_u64_at(buf, HEADER_OFFSET_GLOBAL_META, self.off_global_meta);
        write_u64_at(buf, HEADER_LEN_GLOBAL_META, self.len_global_meta);
        write_u64_at(buf, HEADER_OFFSET_PACKED_SPECTRA, self.off_spec_container);
        write_u64_at(buf, HEADER_LEN_PACKED_SPECTRA, self.len_spec_container);
        write_u64_at(buf, HEADER_OFFSET_PACKED_CHROMS, self.off_chrom_container);
        write_u64_at(buf, HEADER_LEN_PACKED_CHROMS, self.len_chrom_container);

        write_u64_at(buf, HEADER_SPECTRUM_BLOCK_COUNT, self.spec_block_count);
        write_u64_at(buf, HEADER_CHROM_BLOCK_COUNT, self.chrom_block_count);
        write_u64_at(buf, HEADER_SPECTRUM_COUNT, self.spectrum_count);
        write_u64_at(buf, HEADER_CHROM_COUNT, self.chrom_count);
        write_u64_at(buf, HEADER_SPEC_META_ROW_COUNT, self.spec_meta_count);
        write_u64_at(
            buf,
            HEADER_SPEC_META_NUMERIC_COUNT,
            self.spec_meta_numeric_count,
        );
        write_u64_at(
            buf,
            HEADER_SPEC_META_STRING_COUNT,
            self.spec_meta_string_count,
        );
        write_u64_at(buf, HEADER_CHROM_META_ROW_COUNT, self.chrom_meta_count);
        write_u64_at(
            buf,
            HEADER_CHROM_META_NUMERIC_COUNT,
            self.chrom_meta_numeric_count,
        );
        write_u64_at(
            buf,
            HEADER_CHROM_META_STRING_COUNT,
            self.chrom_meta_string_count,
        );
        write_u64_at(buf, HEADER_GLOBAL_META_ROW_COUNT, self.global_meta_count);
        write_u64_at(
            buf,
            HEADER_GLOBAL_META_NUMERIC_COUNT,
            self.global_meta_numeric_count,
        );
        write_u64_at(
            buf,
            HEADER_GLOBAL_META_STRING_COUNT,
            self.global_meta_string_count,
        );
        write_u64_at(
            buf,
            HEADER_SPEC_ARRAY_TYPE_COUNT,
            self.spec_array_type_count,
        );
        write_u64_at(
            buf,
            HEADER_CHROM_ARRAY_TYPE_COUNT,
            self.chrom_array_type_count,
        );

        write_u64_at(
            buf,
            HEADER_SPEC_META_UNCOMPRESSED_SIZE,
            self.spec_meta_uncompressed_bytes,
        );
        write_u64_at(
            buf,
            HEADER_CHROM_META_UNCOMPRESSED_SIZE,
            self.chrom_meta_uncompressed_bytes,
        );
        write_u64_at(
            buf,
            HEADER_GLOBAL_META_UNCOMPRESSED_SIZE,
            self.global_meta_uncompressed_bytes,
        );

        write_u64_at(buf, HEADER_OFFSET_SPEC_SUMMARY, self.off_spec_summary);
        write_u64_at(buf, HEADER_LEN_SPEC_SUMMARY, self.len_spec_summary);
        write_u64_at(buf, HEADER_OFFSET_CHROM_SUMMARY, self.off_chrom_summary);
        write_u64_at(buf, HEADER_LEN_CHROM_SUMMARY, self.len_chrom_summary);

        write_u64_at(buf, HEADER_TOTAL_FILE_SIZE, self.total_file_size);

        write_u64_at(buf, HEADER_META_GROUP_SIZE, self.meta_group_size as u64);
        write_u64_at(
            buf,
            HEADER_SPEC_META_GROUP_COUNT,
            self.spec_meta_group_count,
        );
        write_u64_at(
            buf,
            HEADER_CHROM_META_GROUP_COUNT,
            self.chrom_meta_group_count,
        );

        write_u32_at(buf, HEADER_SPEC_SUMMARY_CRC32, self.spec_summary_crc32);
        write_u32_at(buf, HEADER_SPEC_ENTRIES_CRC32, self.spec_entries_crc32);
        write_u32_at(
            buf,
            HEADER_SPEC_ARRAY_ADDRESSES_CRC32,
            self.spec_array_addresses_crc32,
        );
        write_u32_at(buf, HEADER_CHROM_SUMMARY_CRC32, self.chrom_summary_crc32);
        write_u32_at(buf, HEADER_CHROM_ENTRIES_CRC32, self.chrom_entries_crc32);
        write_u32_at(
            buf,
            HEADER_CHROM_ARRAY_ADDRESSES_CRC32,
            self.chrom_array_addresses_crc32,
        );

        write_u32_at(buf, HEADER_TARGET_MZ_WINDOW, self.target_mz_window);

        write_u32_at(buf, HEADER_SPEC_DIRECTORY_CRC32, self.spec_directory_crc32);
        write_u32_at(
            buf,
            HEADER_CHROM_DIRECTORY_CRC32,
            self.chrom_directory_crc32,
        );

        write_u64_at(
            buf,
            HEADER_OFFSET_SPEC_WINDOW_DIRECTORY,
            self.off_spec_window_directory,
        );
        write_u64_at(
            buf,
            HEADER_LEN_SPEC_WINDOW_DIRECTORY,
            self.len_spec_window_directory,
        );
        write_u64_at(
            buf,
            HEADER_OFFSET_CHROM_WINDOW_DIRECTORY,
            self.off_chrom_window_directory,
        );
        write_u64_at(
            buf,
            HEADER_LEN_CHROM_WINDOW_DIRECTORY,
            self.len_chrom_window_directory,
        );
        write_u64_at(
            buf,
            HEADER_PLAIN_LEN_SPEC_WINDOW_DIRECTORY,
            self.plain_len_spec_window_directory,
        );
        write_u64_at(
            buf,
            HEADER_PLAIN_LEN_CHROM_WINDOW_DIRECTORY,
            self.plain_len_chrom_window_directory,
        );
        write_u32_at(
            buf,
            HEADER_SPEC_WINDOW_DIRECTORY_CRC32,
            self.spec_window_directory_crc32,
        );
        write_u32_at(
            buf,
            HEADER_CHROM_WINDOW_DIRECTORY_CRC32,
            self.chrom_window_directory_crc32,
        );

        write_u32_at(buf, HEADER_SPEC_META_CRC32, self.spec_meta_crc32);
        write_u32_at(buf, HEADER_CHROM_META_CRC32, self.chrom_meta_crc32);
        write_u32_at(buf, HEADER_GLOBAL_META_CRC32, self.global_meta_crc32);
        write_u32_at(buf, HEADER_CRC32, self.header_crc32);
    }
}

pub(crate) fn parse_header(bytes: &[u8]) -> IonResult<Header> {
    Header::parse(bytes)
}

#[inline]
pub fn version_of(bytes: &[u8]) -> Option<u16> {
    let end = HEADER_FORMAT_VERSION_OFFSET + 2;
    if bytes.len() < end {
        return None;
    }
    let mut buf = [0u8; 2];
    buf.copy_from_slice(&bytes[HEADER_FORMAT_VERSION_OFFSET..end]);
    Some(u16::from_le_bytes(buf))
}

pub fn set_version(header: &mut [u8]) -> IonResult<bool> {
    if header.len() < HEADER_SIZE {
        return Err(format!(
            "header: need {HEADER_SIZE} bytes to update the format version, got {}",
            header.len()
        )
        .into());
    }
    if header[0..8] != FILE_SIGNATURE {
        return Err("header: bad file signature".into());
    }
    let stored_crc = read_u32_at(header, HEADER_CRC32);
    let computed_crc = crc32fast::hash(&header[0..HEADER_CRC32]);
    if stored_crc != computed_crc {
        return Err(format!(
            "header: header_crc32 mismatch (stored={stored_crc:#010x}, computed={computed_crc:#010x})"
        )
        .into());
    }
    let version = version_of(header).unwrap();
    if version == CURRENT_VERSION {
        return Ok(false);
    }
    allow_version(version)?;
    write_u16_at(header, HEADER_FORMAT_VERSION_OFFSET, CURRENT_VERSION);
    let new_crc = crc32fast::hash(&header[0..HEADER_CRC32]);
    write_u32_at(header, HEADER_CRC32, new_crc);
    Ok(true)
}

#[inline]
#[allow(dead_code)]
pub(crate) fn get_total_file_size_from_header(bytes: &[u8]) -> Option<u64> {
    let end = HEADER_TOTAL_FILE_SIZE + 8;
    if bytes.len() < end {
        return None;
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[HEADER_TOTAL_FILE_SIZE..end]);
    Some(u64::from_le_bytes(buf))
}

pub(crate) fn check_section_layout(h: &Header) -> Vec<String> {
    let mut failures = Vec::new();

    match h.spec_block_count.checked_mul(BLOCK_DIRECTORY_ENTRY_SIZE_U64) {
        None => failures.push(format!(
            "condition 5: spec_block_count ({}) × {BLOCK_DIRECTORY_ENTRY_SIZE_U64} overflows u64",
            h.spec_block_count
        )),
        Some(directory_bytes) if directory_bytes > h.len_spec_container => failures.push(format!(
            "condition 5: spec_block_count × {BLOCK_DIRECTORY_ENTRY_SIZE_U64} ({directory_bytes}) > len_spec_container ({})",
            h.len_spec_container
        )),
        _ => {}
    }

    match h.chrom_block_count.checked_mul(BLOCK_DIRECTORY_ENTRY_SIZE_U64) {
        None => failures.push(format!(
            "condition 6: chrom_block_count ({}) × {BLOCK_DIRECTORY_ENTRY_SIZE_U64} overflows u64",
            h.chrom_block_count
        )),
        Some(directory_bytes) if directory_bytes > h.len_chrom_container => failures.push(format!(
            "condition 6: chrom_block_count × {BLOCK_DIRECTORY_ENTRY_SIZE_U64} ({directory_bytes}) > len_chrom_container ({})",
            h.len_chrom_container
        )),
        _ => {}
    }

    let trailer_start = h.total_file_size.saturating_sub(8);
    let sections: &[(&str, u64, u64)] = &[
        ("spec_entries", h.off_spec_entries, h.len_spec_entries),
        (
            "spec_array_addresses",
            h.off_spec_array_addresses,
            h.len_spec_array_addresses,
        ),
        ("chrom_entries", h.off_chrom_entries, h.len_chrom_entries),
        (
            "chrom_array_addresses",
            h.off_chrom_array_addresses,
            h.len_chrom_array_addresses,
        ),
        ("spec_meta", h.off_spec_meta, h.len_spec_meta),
        ("chrom_meta", h.off_chrom_meta, h.len_chrom_meta),
        ("global_meta", h.off_global_meta, h.len_global_meta),
        (
            "container_spect",
            h.off_spec_container,
            h.len_spec_container,
        ),
        (
            "container_chrom",
            h.off_chrom_container,
            h.len_chrom_container,
        ),
        ("spec_summary", h.off_spec_summary, h.len_spec_summary),
        ("chrom_summary", h.off_chrom_summary, h.len_chrom_summary),
        (
            "spec_window_directory",
            h.off_spec_window_directory,
            h.len_spec_window_directory,
        ),
        (
            "chrom_window_directory",
            h.off_chrom_window_directory,
            h.len_chrom_window_directory,
        ),
    ];

    for &(name, off, len) in sections {
        match off.checked_add(len) {
            None => failures.push(format!("condition 7: {name} offset+length overflows u64")),
            Some(end) if end > trailer_start => failures.push(format!(
                "condition 7: {name} end ({end}) exceeds trailer_start ({trailer_start})"
            )),
            _ => {}
        }
    }

    let mut sorted: Vec<(&str, u64, u64)> = sections
        .iter()
        .copied()
        .filter(|&(_, _, len)| len > 0)
        .collect();
    sorted.sort_by_key(|&(_, off, _)| off);
    for i in 1..sorted.len() {
        let (prev_name, prev_off, prev_len) = sorted[i - 1];
        let (curr_name, curr_off, _) = sorted[i];
        match prev_off.checked_add(prev_len) {
            None => failures.push(format!(
                "condition 8: section {prev_name} offset+length overflows u64"
            )),
            Some(prev_end) if prev_end > curr_off => failures.push(format!(
                "condition 8: sections {prev_name} and {curr_name} overlap \
                 ({prev_name} ends at {prev_end}, {curr_name} starts at {curr_off})"
            )),
            _ => {}
        }
    }

    for &(name, off, len) in sections {
        if len > 0 && off % 8 != 0 {
            failures.push(format!(
                "condition 9: {name} offset ({off}) is not 8-byte aligned"
            ));
        }
        if len > 0 && off < HEADER_SIZE as u64 {
            failures.push(format!(
                "condition 9: {name} offset ({off}) is inside the {HEADER_SIZE}-byte header"
            ));
        }
    }

    enforce_count_bounds(&mut failures, h);
    enforce_section_element_counts(&mut failures, h);

    failures
}

fn enforce_section_element_counts(failures: &mut Vec<String>, h: &Header) {
    if h.compression_codec == CODEC_ZSTD {
        return;
    }
    let checks: &[(&str, u64, u64, u64)] = &[
        (
            "spec_summary",
            h.len_spec_summary,
            h.spectrum_count,
            SPEC_SUMMARY_SIZE as u64,
        ),
        (
            "spec_entries",
            h.len_spec_entries,
            h.spectrum_count,
            INDEX_ENTRY_BYTES as u64,
        ),
        (
            "chrom_summary",
            h.len_chrom_summary,
            h.chrom_count,
            CHROM_SUMMARY_SIZE as u64,
        ),
        (
            "chrom_entries",
            h.len_chrom_entries,
            h.chrom_count,
            INDEX_ENTRY_BYTES as u64,
        ),
    ];
    for &(name, len, count, size) in checks {
        match count.checked_mul(size) {
            None => failures.push(format!("condition 14: {name} count × {size} overflows u64")),
            Some(expected) if expected != len => failures.push(format!(
                "condition 14: {name} length ({len}) != count ({count}) × {size}"
            )),
            _ => {}
        }
    }
}

fn enforce_count_bounds(failures: &mut Vec<String>, h: &Header) {
    let checks: &[(&str, u64, u64)] = &[
        ("spec_block_count", h.spec_block_count, h.total_file_size),
        ("chrom_block_count", h.chrom_block_count, h.total_file_size),
        ("spectrum_count", h.spectrum_count, h.total_file_size),
        ("chrom_count", h.chrom_count, h.total_file_size),
        (
            "spec_meta_count",
            h.spec_meta_count,
            h.spec_meta_uncompressed_bytes,
        ),
        (
            "spec_meta_numeric_count",
            h.spec_meta_numeric_count,
            h.spec_meta_uncompressed_bytes,
        ),
        (
            "spec_meta_string_count",
            h.spec_meta_string_count,
            h.spec_meta_uncompressed_bytes,
        ),
        (
            "chrom_meta_count",
            h.chrom_meta_count,
            h.chrom_meta_uncompressed_bytes,
        ),
        (
            "chrom_meta_numeric_count",
            h.chrom_meta_numeric_count,
            h.chrom_meta_uncompressed_bytes,
        ),
        (
            "chrom_meta_string_count",
            h.chrom_meta_string_count,
            h.chrom_meta_uncompressed_bytes,
        ),
        (
            "spec_array_type_count",
            h.spec_array_type_count,
            h.total_file_size,
        ),
        (
            "chrom_array_type_count",
            h.chrom_array_type_count,
            h.total_file_size,
        ),
    ];
    for &(name, count, limit) in checks {
        if count > limit {
            failures.push(format!(
                "condition 13: {name} ({count}) exceeds allowed bound ({limit})"
            ));
        }
    }
}

#[inline]
fn read_u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(<[u8; 4]>::try_from(&bytes[offset..offset + 4]).unwrap())
}

#[inline]
fn read_u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(<[u8; 8]>::try_from(&bytes[offset..offset + 8]).unwrap())
}

#[inline]
fn write_u8_at(buf: &mut [u8], offset: usize, value: u8) {
    buf[offset] = value;
}

#[inline]
fn write_u16_at(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

#[inline]
fn write_u32_at(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[inline]
fn write_u64_at(buf: &mut [u8], offset: usize, value: u64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ion::{
        IonError,
        format::{MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION},
    };

    fn valid_header_bytes() -> [u8; HEADER_SIZE] {
        let mut h = [0u8; HEADER_SIZE];
        h[0..8].copy_from_slice(&FILE_SIGNATURE);
        h[HEADER_FORMAT_VERSION_OFFSET..HEADER_FORMAT_VERSION_OFFSET + 2]
            .copy_from_slice(&CURRENT_VERSION.to_le_bytes());
        h
    }

    #[test]
    fn corrupt_reserved_area_is_rejected() {
        let mut h = valid_header_bytes();
        h[440] = 1;
        assert!(Header::parse(&h).is_err());

        let mut at_end = valid_header_bytes();
        at_end[967] = 1;
        assert!(Header::parse(&at_end).is_err());
    }

    fn header_bytes_with(build: impl FnOnce(&mut Header)) -> [u8; HEADER_SIZE] {
        let mut header = Header {
            file_signature: FILE_SIGNATURE,
            endianness_flag: 0,
            format_version: CURRENT_VERSION,
            ..Header::default()
        };
        build(&mut header);
        let mut buf = [0u8; HEADER_SIZE];
        header.write(&mut buf);
        let crc = crc32fast::hash(&buf[0..HEADER_CRC32]);
        buf[HEADER_CRC32..HEADER_SIZE].copy_from_slice(&crc.to_le_bytes());
        buf
    }

    #[test]
    fn reserved_block_must_be_zero() {
        let base = header_bytes_with(|_| {});

        for offset in [
            RESERVED_BLOCK_START,
            500,
            700,
            RESERVED_BLOCK_START + RESERVED_BLOCK_SIZE - 1,
        ] {
            let mut corrupted = base;
            corrupted[offset] = 1;
            let error = Header::parse(&corrupted).expect_err("nonzero reserved byte");
            assert!(
                error.to_string().contains("reserved block"),
                "offset {offset}: unexpected error {error}"
            );
        }
    }

    #[test]
    fn get_version_returns_none_on_short_buffer() {
        let too_short = [0u8; HEADER_FORMAT_VERSION_OFFSET + 1];
        assert_eq!(version_of(&too_short), None);
    }

    #[test]
    fn get_version_reads_little_endian_word() {
        let mut bytes = [0u8; HEADER_SIZE];
        bytes[HEADER_FORMAT_VERSION_OFFSET..HEADER_FORMAT_VERSION_OFFSET + 2]
            .copy_from_slice(&CURRENT_VERSION.to_le_bytes());
        assert_eq!(version_of(&bytes), Some(CURRENT_VERSION));
    }

    #[test]
    fn get_version_handles_max_u16() {
        let mut bytes = [0u8; HEADER_SIZE];
        bytes[HEADER_FORMAT_VERSION_OFFSET..HEADER_FORMAT_VERSION_OFFSET + 2]
            .copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(version_of(&bytes), Some(u16::MAX));
    }

    #[test]
    fn get_version_handles_exact_minimum_buffer_length() {
        let bytes = [0u8; HEADER_FORMAT_VERSION_OFFSET + 2];
        assert_eq!(version_of(&bytes), Some(0));
    }

    #[test]
    fn header_offsets_match_clean_v1_layout() {
        assert_eq!(HEADER_FORMAT_VERSION_OFFSET, 9);
        assert_eq!(HEADER_CODEC_ID, 11);
        assert_eq!(HEADER_COMPRESSION_LEVEL, 12);
        assert_eq!(HEADER_ARRAY_FILTER_ID, 13);
        assert_eq!(HEADER_TARGET_BLOCK_SIZE, 16);
        assert_eq!(HEADER_TARGET_MZ_WINDOW, 24);
        assert_eq!(HEADER_OFFSET_SPEC_WINDOW_DIRECTORY, 32);
        assert_eq!(HEADER_LEN_SPEC_WINDOW_DIRECTORY, 40);
        assert_eq!(HEADER_OFFSET_SPEC_SUMMARY, 48);
        assert_eq!(HEADER_LEN_SPEC_SUMMARY, 56);
        assert_eq!(HEADER_OFFSET_SPEC_ENTRIES, 64);
        assert_eq!(HEADER_LEN_SPEC_ENTRIES, 72);
        assert_eq!(HEADER_OFFSET_SPEC_ARRAY_ADDRESSES, 80);
        assert_eq!(HEADER_LEN_SPEC_ARRAY_ADDRESSES, 88);
        assert_eq!(HEADER_OFFSET_CHROM_WINDOW_DIRECTORY, 96);
        assert_eq!(HEADER_LEN_CHROM_WINDOW_DIRECTORY, 104);
        assert_eq!(HEADER_OFFSET_CHROM_SUMMARY, 112);
        assert_eq!(HEADER_LEN_CHROM_SUMMARY, 120);
        assert_eq!(HEADER_OFFSET_CHROM_ENTRIES, 128);
        assert_eq!(HEADER_LEN_CHROM_ENTRIES, 136);
        assert_eq!(HEADER_OFFSET_CHROM_ARRAY_ADDRESSES, 144);
        assert_eq!(HEADER_LEN_CHROM_ARRAY_ADDRESSES, 152);
        assert_eq!(HEADER_OFFSET_SPEC_META, 160);
        assert_eq!(HEADER_LEN_SPEC_META, 168);
        assert_eq!(HEADER_OFFSET_CHROM_META, 176);
        assert_eq!(HEADER_LEN_CHROM_META, 184);
        assert_eq!(HEADER_OFFSET_GLOBAL_META, 192);
        assert_eq!(HEADER_LEN_GLOBAL_META, 200);
        assert_eq!(HEADER_OFFSET_PACKED_SPECTRA, 208);
        assert_eq!(HEADER_LEN_PACKED_SPECTRA, 216);
        assert_eq!(HEADER_OFFSET_PACKED_CHROMS, 224);
        assert_eq!(HEADER_LEN_PACKED_CHROMS, 232);
        assert_eq!(HEADER_SPECTRUM_BLOCK_COUNT, 240);
        assert_eq!(HEADER_CHROM_BLOCK_COUNT, 248);
        assert_eq!(HEADER_SPECTRUM_COUNT, 256);
        assert_eq!(HEADER_CHROM_COUNT, 264);
        assert_eq!(HEADER_SPEC_ARRAY_TYPE_COUNT, 272);
        assert_eq!(HEADER_CHROM_ARRAY_TYPE_COUNT, 280);
        assert_eq!(HEADER_SPEC_META_ROW_COUNT, 288);
        assert_eq!(HEADER_SPEC_META_NUMERIC_COUNT, 296);
        assert_eq!(HEADER_SPEC_META_STRING_COUNT, 304);
        assert_eq!(HEADER_CHROM_META_ROW_COUNT, 312);
        assert_eq!(HEADER_CHROM_META_NUMERIC_COUNT, 320);
        assert_eq!(HEADER_CHROM_META_STRING_COUNT, 328);
        assert_eq!(HEADER_GLOBAL_META_ROW_COUNT, 336);
        assert_eq!(HEADER_GLOBAL_META_NUMERIC_COUNT, 344);
        assert_eq!(HEADER_GLOBAL_META_STRING_COUNT, 352);
        assert_eq!(HEADER_SPEC_META_UNCOMPRESSED_SIZE, 360);
        assert_eq!(HEADER_CHROM_META_UNCOMPRESSED_SIZE, 368);
        assert_eq!(HEADER_GLOBAL_META_UNCOMPRESSED_SIZE, 376);
        assert_eq!(HEADER_PLAIN_LEN_SPEC_WINDOW_DIRECTORY, 384);
        assert_eq!(HEADER_PLAIN_LEN_CHROM_WINDOW_DIRECTORY, 392);
        assert_eq!(HEADER_TOTAL_FILE_SIZE, 400);
        assert_eq!(HEADER_META_GROUP_SIZE, 408);
        assert_eq!(HEADER_SPEC_META_GROUP_COUNT, 416);
        assert_eq!(HEADER_CHROM_META_GROUP_COUNT, 424);
        assert_eq!(RESERVED_BLOCK_START, 432);
        assert_eq!(RESERVED_BLOCK_SIZE, 536);
        assert_eq!(RESERVED_BLOCK_START + RESERVED_BLOCK_SIZE, 968);
        assert_eq!(HEADER_SPEC_WINDOW_DIRECTORY_CRC32, 968);
        assert_eq!(HEADER_SPEC_SUMMARY_CRC32, 972);
        assert_eq!(HEADER_SPEC_ENTRIES_CRC32, 976);
        assert_eq!(HEADER_SPEC_ARRAY_ADDRESSES_CRC32, 980);
        assert_eq!(HEADER_CHROM_WINDOW_DIRECTORY_CRC32, 984);
        assert_eq!(HEADER_CHROM_SUMMARY_CRC32, 988);
        assert_eq!(HEADER_CHROM_ENTRIES_CRC32, 992);
        assert_eq!(HEADER_CHROM_ARRAY_ADDRESSES_CRC32, 996);
        assert_eq!(HEADER_SPEC_DIRECTORY_CRC32, 1000);
        assert_eq!(HEADER_CHROM_DIRECTORY_CRC32, 1004);
        assert_eq!(HEADER_SPEC_META_CRC32, 1008);
        assert_eq!(HEADER_CHROM_META_CRC32, 1012);
        assert_eq!(HEADER_GLOBAL_META_CRC32, 1016);
        assert_eq!(HEADER_CRC32, 1020);
    }

    #[test]
    fn write_then_parse_round_trips_every_field() {
        let mut original = Header {
            file_signature: FILE_SIGNATURE,
            endianness_flag: 0,
            format_version: CURRENT_VERSION,
            compression_codec: 1,
            compression_level: 3,
            default_array_filter: 2,
            target_block_uncompressed_bytes: 0x0102_0304_0506_0708,
            off_spec_summary: 0x0010_0000_0000_0000,
            len_spec_summary: 0x0011_0000_0000_0001,
            off_chrom_summary: 0x0012_0000_0000_0002,
            len_chrom_summary: 0x0013_0000_0000_0003,
            off_spec_entries: 0x0014_0000_0000_0004,
            len_spec_entries: 0x0015_0000_0000_0005,
            off_spec_array_addresses: 0x0016_0000_0000_0006,
            len_spec_array_addresses: 0x0017_0000_0000_0007,
            off_chrom_entries: 0x0018_0000_0000_0008,
            len_chrom_entries: 0x0019_0000_0000_0009,
            off_chrom_array_addresses: 0x001a_0000_0000_000a,
            len_chrom_array_addresses: 0x001b_0000_0000_000b,
            off_spec_meta: 0x001c_0000_0000_000c,
            len_spec_meta: 0x001d_0000_0000_000d,
            off_chrom_meta: 0x001e_0000_0000_000e,
            len_chrom_meta: 0x001f_0000_0000_000f,
            off_global_meta: 0x0020_0000_0000_0010,
            len_global_meta: 0x0021_0000_0000_0011,
            off_spec_container: 0x0022_0000_0000_0012,
            len_spec_container: 0x0023_0000_0000_0013,
            off_chrom_container: 0x0024_0000_0000_0014,
            len_chrom_container: 0x0025_0000_0000_0015,
            spec_block_count: 0x0026_0000_0000_0016,
            chrom_block_count: 0x0027_0000_0000_0017,
            spectrum_count: 0x0028_0000_0000_0018,
            chrom_count: 0x0029_0000_0000_0019,
            spec_meta_count: 0x002a_0000_0000_001a,
            spec_meta_numeric_count: 0x002b_0000_0000_001b,
            spec_meta_string_count: 0x002c_0000_0000_001c,
            chrom_meta_count: 0x002d_0000_0000_001d,
            chrom_meta_numeric_count: 0x002e_0000_0000_001e,
            chrom_meta_string_count: 0x002f_0000_0000_001f,
            global_meta_count: 0x0030_0000_0000_0020,
            global_meta_numeric_count: 0x0031_0000_0000_0021,
            global_meta_string_count: 0x0032_0000_0000_0022,
            spec_array_type_count: 0x0033_0000_0000_0023,
            chrom_array_type_count: 0x0034_0000_0000_0024,
            spec_meta_uncompressed_bytes: 0x0035_0000_0000_0025,
            chrom_meta_uncompressed_bytes: 0x0036_0000_0000_0026,
            global_meta_uncompressed_bytes: 0x0037_0000_0000_0027,
            total_file_size: 0x0038_0000_0000_0028,
            meta_group_size: 0x0000_1234,
            spec_meta_group_count: 0x0039_0000_0000_0029,
            chrom_meta_group_count: 0x003a_0000_0000_002a,
            spec_summary_crc32: 0xAABB_CC0A,
            spec_entries_crc32: 0xAABB_CC0B,
            spec_array_addresses_crc32: 0xAABB_CC0C,
            chrom_summary_crc32: 0xAABB_CC0D,
            chrom_entries_crc32: 0xAABB_CC0E,
            chrom_array_addresses_crc32: 0xAABB_CC0F,
            target_mz_window: 0x0041_0031,
            spec_directory_crc32: 0xAABB_CC01,
            chrom_directory_crc32: 0xAABB_CC02,
            off_spec_window_directory: 0x003b_0000_0000_002b,
            len_spec_window_directory: 0x003c_0000_0000_002c,
            off_chrom_window_directory: 0x003d_0000_0000_002d,
            len_chrom_window_directory: 0x003e_0000_0000_002e,
            plain_len_spec_window_directory: 0x003f_0000_0000_002f,
            plain_len_chrom_window_directory: 0x0040_0000_0000_0030,
            spec_window_directory_crc32: 0xAABB_CC03,
            chrom_window_directory_crc32: 0xAABB_CC04,
            spec_meta_crc32: 0xAABB_CC05,
            chrom_meta_crc32: 0xAABB_CC06,
            global_meta_crc32: 0xAABB_CC07,
            header_crc32: 0xAABB_CC08,
        };

        let mut buf = [0u8; HEADER_SIZE];
        original.write(&mut buf);
        let crc = crc32fast::hash(&buf[0..HEADER_CRC32]);
        buf[HEADER_CRC32..HEADER_SIZE].copy_from_slice(&crc.to_le_bytes());
        original.header_crc32 = crc;

        let parsed = Header::parse(&buf).expect("round-trip parse failed");

        assert_eq!(parsed.file_signature, original.file_signature);
        assert_eq!(parsed.endianness_flag, original.endianness_flag);
        assert_eq!(parsed.format_version, original.format_version);
        assert_eq!(parsed.compression_codec, original.compression_codec);
        assert_eq!(parsed.compression_level, original.compression_level);
        assert_eq!(parsed.default_array_filter, original.default_array_filter);
        assert_eq!(
            parsed.target_block_uncompressed_bytes,
            original.target_block_uncompressed_bytes
        );
        assert_eq!(parsed.off_spec_summary, original.off_spec_summary);
        assert_eq!(parsed.len_spec_summary, original.len_spec_summary);
        assert_eq!(parsed.off_chrom_summary, original.off_chrom_summary);
        assert_eq!(parsed.len_chrom_summary, original.len_chrom_summary);
        assert_eq!(parsed.off_spec_entries, original.off_spec_entries);
        assert_eq!(parsed.len_spec_entries, original.len_spec_entries);
        assert_eq!(
            parsed.off_spec_array_addresses,
            original.off_spec_array_addresses
        );
        assert_eq!(
            parsed.len_spec_array_addresses,
            original.len_spec_array_addresses
        );
        assert_eq!(parsed.off_chrom_entries, original.off_chrom_entries);
        assert_eq!(parsed.len_chrom_entries, original.len_chrom_entries);
        assert_eq!(
            parsed.off_chrom_array_addresses,
            original.off_chrom_array_addresses
        );
        assert_eq!(
            parsed.len_chrom_array_addresses,
            original.len_chrom_array_addresses
        );
        assert_eq!(parsed.off_spec_meta, original.off_spec_meta);
        assert_eq!(parsed.len_spec_meta, original.len_spec_meta);
        assert_eq!(parsed.off_chrom_meta, original.off_chrom_meta);
        assert_eq!(parsed.len_chrom_meta, original.len_chrom_meta);
        assert_eq!(parsed.off_global_meta, original.off_global_meta);
        assert_eq!(parsed.len_global_meta, original.len_global_meta);
        assert_eq!(parsed.off_spec_container, original.off_spec_container);
        assert_eq!(parsed.len_spec_container, original.len_spec_container);
        assert_eq!(parsed.off_chrom_container, original.off_chrom_container);
        assert_eq!(parsed.len_chrom_container, original.len_chrom_container);
        assert_eq!(parsed.spec_block_count, original.spec_block_count);
        assert_eq!(parsed.chrom_block_count, original.chrom_block_count);
        assert_eq!(parsed.spectrum_count, original.spectrum_count);
        assert_eq!(parsed.chrom_count, original.chrom_count);
        assert_eq!(parsed.spec_meta_count, original.spec_meta_count);
        assert_eq!(
            parsed.spec_meta_numeric_count,
            original.spec_meta_numeric_count
        );
        assert_eq!(
            parsed.spec_meta_string_count,
            original.spec_meta_string_count
        );
        assert_eq!(parsed.chrom_meta_count, original.chrom_meta_count);
        assert_eq!(
            parsed.chrom_meta_numeric_count,
            original.chrom_meta_numeric_count
        );
        assert_eq!(
            parsed.chrom_meta_string_count,
            original.chrom_meta_string_count
        );
        assert_eq!(parsed.global_meta_count, original.global_meta_count);
        assert_eq!(
            parsed.global_meta_numeric_count,
            original.global_meta_numeric_count
        );
        assert_eq!(
            parsed.global_meta_string_count,
            original.global_meta_string_count
        );
        assert_eq!(parsed.spec_array_type_count, original.spec_array_type_count);
        assert_eq!(
            parsed.chrom_array_type_count,
            original.chrom_array_type_count
        );
        assert_eq!(
            parsed.spec_meta_uncompressed_bytes,
            original.spec_meta_uncompressed_bytes
        );
        assert_eq!(
            parsed.chrom_meta_uncompressed_bytes,
            original.chrom_meta_uncompressed_bytes
        );
        assert_eq!(
            parsed.global_meta_uncompressed_bytes,
            original.global_meta_uncompressed_bytes
        );
        assert_eq!(parsed.total_file_size, original.total_file_size);
        assert_eq!(parsed.meta_group_size, original.meta_group_size);
        assert_eq!(parsed.spec_meta_group_count, original.spec_meta_group_count);
        assert_eq!(
            parsed.chrom_meta_group_count,
            original.chrom_meta_group_count
        );
        assert_eq!(parsed.spec_summary_crc32, original.spec_summary_crc32);
        assert_eq!(parsed.spec_entries_crc32, original.spec_entries_crc32);
        assert_eq!(
            parsed.spec_array_addresses_crc32,
            original.spec_array_addresses_crc32
        );
        assert_eq!(parsed.chrom_summary_crc32, original.chrom_summary_crc32);
        assert_eq!(parsed.chrom_entries_crc32, original.chrom_entries_crc32);
        assert_eq!(
            parsed.chrom_array_addresses_crc32,
            original.chrom_array_addresses_crc32
        );
        assert_eq!(parsed.target_mz_window, original.target_mz_window);
        assert_eq!(parsed.spec_directory_crc32, original.spec_directory_crc32);
        assert_eq!(parsed.chrom_directory_crc32, original.chrom_directory_crc32);
        assert_eq!(
            parsed.off_spec_window_directory,
            original.off_spec_window_directory
        );
        assert_eq!(
            parsed.len_spec_window_directory,
            original.len_spec_window_directory
        );
        assert_eq!(
            parsed.off_chrom_window_directory,
            original.off_chrom_window_directory
        );
        assert_eq!(
            parsed.len_chrom_window_directory,
            original.len_chrom_window_directory
        );
        assert_eq!(
            parsed.plain_len_spec_window_directory,
            original.plain_len_spec_window_directory
        );
        assert_eq!(
            parsed.plain_len_chrom_window_directory,
            original.plain_len_chrom_window_directory
        );
        assert_eq!(
            parsed.spec_window_directory_crc32,
            original.spec_window_directory_crc32
        );
        assert_eq!(
            parsed.chrom_window_directory_crc32,
            original.chrom_window_directory_crc32
        );
        assert_eq!(parsed.spec_meta_crc32, original.spec_meta_crc32);
        assert_eq!(parsed.chrom_meta_crc32, original.chrom_meta_crc32);
        assert_eq!(parsed.global_meta_crc32, original.global_meta_crc32);
        assert_eq!(parsed.header_crc32, original.header_crc32);
    }

    #[test]
    fn check_section_layout_accepts_empty_window_directories() {
        let header = Header {
            total_file_size: 4096,
            ..Default::default()
        };

        let failures = check_section_layout(&header);

        assert!(
            failures
                .iter()
                .all(|f| !f.contains("spec_window_directory")
                    && !f.contains("chrom_window_directory")),
            "empty window directories must not fail layout checks: {failures:?}"
        );
    }

    #[test]
    fn check_section_layout_rejects_spec_window_directory_inside_header() {
        let header = Header {
            total_file_size: 4096,
            off_spec_window_directory: 16,
            len_spec_window_directory: 32,
            ..Default::default()
        };

        let failures = check_section_layout(&header);

        assert!(
            failures.iter().any(|f| f.contains("spec_window_directory")),
            "expected a spec_window_directory layout failure, got: {failures:?}"
        );
    }

    #[test]
    fn check_section_layout_rejects_chrom_window_directory_overlap() {
        let header = Header {
            total_file_size: 4096,
            off_global_meta: 1024,
            len_global_meta: 64,
            off_chrom_window_directory: 1040,
            len_chrom_window_directory: 32,
            ..Default::default()
        };

        let failures = check_section_layout(&header);

        assert!(
            failures
                .iter()
                .any(|f| f.contains("chrom_window_directory")),
            "expected a chrom_window_directory overlap failure, got: {failures:?}"
        );
    }

    #[test]
    fn condition_13_bounds_meta_by_uncompressed_size_1() {
        let mut header = Header {
            total_file_size: 100,
            spec_meta_uncompressed_bytes: 100_000,
            spec_meta_count: 50_000,
            ..Header::default()
        };

        let mut failures = Vec::new();
        enforce_count_bounds(&mut failures, &header);
        assert!(
            failures.iter().all(|f| !f.contains("spec_meta_count")),
            "spec_meta_count under the uncompressed-size limit must not fail: {failures:?}"
        );

        header.spec_meta_count = 200_000;
        let mut failures = Vec::new();
        enforce_count_bounds(&mut failures, &header);
        assert!(
            failures.iter().any(|f| f.contains("spec_meta_count")),
            "spec_meta_count over the uncompressed-size limit must fail: {failures:?}"
        );
    }

    fn header_bytes_at_version(version: u16) -> [u8; HEADER_SIZE] {
        let mut buf = header_bytes_with(|_| {});
        write_u16_at(&mut buf, HEADER_FORMAT_VERSION_OFFSET, version);
        let crc = crc32fast::hash(&buf[0..HEADER_CRC32]);
        write_u32_at(&mut buf, HEADER_CRC32, crc);
        buf
    }

    #[test]
    fn update_rewrites_older_supported_version_to_current() {
        let mut buf = header_bytes_at_version(MIN_SUPPORTED_VERSION);
        assert_eq!(set_version(&mut buf), Ok(true));
        assert_eq!(version_of(&buf), Some(CURRENT_VERSION));
        assert_eq!(
            read_u32_at(&buf, HEADER_CRC32),
            crc32fast::hash(&buf[0..HEADER_CRC32])
        );
        assert!(Header::parse(&buf).is_ok());
    }

    #[test]
    fn update_leaves_current_version_untouched() {
        let mut buf = header_bytes_at_version(CURRENT_VERSION);
        let before = buf;
        assert_eq!(set_version(&mut buf), Ok(false));
        assert_eq!(buf, before);
    }

    #[test]
    fn update_rejects_unsupported_version() {
        let mut buf = header_bytes_at_version(MAX_SUPPORTED_VERSION + 1);
        let before = buf;
        assert_eq!(
            set_version(&mut buf),
            Err(IonError::UnsupportedFormatVersion(
                MAX_SUPPORTED_VERSION + 1
            ))
        );
        assert_eq!(buf, before);
    }

    #[test]
    fn update_rejects_corrupt_header_crc() {
        let mut buf = header_bytes_at_version(MIN_SUPPORTED_VERSION);
        buf[HEADER_CRC32] ^= 0xff;
        let before = buf;
        let error = set_version(&mut buf).unwrap_err();
        assert!(error.contains("header_crc32 mismatch"), "{error}");
        assert_eq!(buf, before);
    }

    #[test]
    fn update_rejects_bad_signature_and_short_buffer() {
        let mut buf = header_bytes_at_version(MIN_SUPPORTED_VERSION);
        buf[0] = b'X';
        assert!(set_version(&mut buf).is_err());

        let mut short = [0u8; HEADER_SIZE - 1];
        assert!(set_version(&mut short).is_err());
    }
}
