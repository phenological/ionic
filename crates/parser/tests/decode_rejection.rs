mod common;

use common::{assertions::assert_mzml_semantic_eq, decode_ion, encode_to_ion, test_files};
use ionic::{
    IonReader, IonResult, ReadOptions,
    format::{HEADER_FORMAT_VERSION_OFFSET, MAX_SUPPORTED_VERSION},
    mzml::structs::MzML,
};

const HEADER_CRC32_OFFSET: usize = 1020;
const HEADER_OFFSET_GLOBAL_META: usize = 192;
const HEADER_LEN_GLOBAL_META: usize = 200;
const HEADER_OFFSET_SPEC_ENTRIES: usize = 64;
const HEADER_LEN_SPEC_ENTRIES: usize = 72;
const HEADER_LEN_SPEC_ARRAY_ADDRESSES: usize = 88;
const HEADER_OFFSET_CHROM_ENTRIES: usize = 128;
const HEADER_LEN_CHROM_ENTRIES: usize = 136;
const HEADER_OFFSET_SPEC_CONTAINER: usize = 208;
const HEADER_LEN_SPEC_CONTAINER: usize = 216;
const HEADER_SPEC_BLOCK_COUNT: usize = 240;
const BLOCK_DIRECTORY_ENTRY_SIZE: u64 = 32;

fn read_header_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

fn write_u64_at(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn spec_block_directory_entry_offset(bytes: &[u8]) -> usize {
    let container_offset = read_header_u64(bytes, HEADER_OFFSET_SPEC_CONTAINER);
    let container_len = read_header_u64(bytes, HEADER_LEN_SPEC_CONTAINER);
    let block_count = read_header_u64(bytes, HEADER_SPEC_BLOCK_COUNT);
    assert!(
        block_count > 0,
        "test file must produce at least one spec block"
    );
    (container_offset + container_len - block_count * BLOCK_DIRECTORY_ENTRY_SIZE) as usize
}

fn decode_ion_without_checksum_verification(bytes: &[u8]) -> IonResult<MzML> {
    let options = ReadOptions {
        verify_checksums: false,
        ..ReadOptions::default()
    };
    let mut decoder = IonReader::from_bytes(bytes, &options)?;
    decoder.to_mzml()
}

#[test]
fn rejects_invalid_signature() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    bytes[0] = b'X';
    let err = decode_ion(&bytes).expect_err("decode must reject invalid signature");
    assert!(err.contains("file_signature") || err.contains("signature"));
}

#[test]
fn rejects_unsupported_format_version() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let unsupported = MAX_SUPPORTED_VERSION + 1;
    bytes[HEADER_FORMAT_VERSION_OFFSET..HEADER_FORMAT_VERSION_OFFSET + 2]
        .copy_from_slice(&unsupported.to_le_bytes());
    let err = decode_ion(&bytes).expect_err("decode must reject unsupported format version");
    assert!(
        err.contains("unsupported format version"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn accepts_all_supported_format_versions() {
    use ionic::format::{MIN_SUPPORTED_VERSION, allow_version};
    for version in MIN_SUPPORTED_VERSION..=MAX_SUPPORTED_VERSION {
        assert!(
            allow_version(version).is_ok(),
            "version {version} must be allowed"
        );
    }
}

#[test]
fn rejects_truncated_payload() {
    let bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let truncated = &bytes[..bytes.len() / 2];
    let _ = decode_ion(truncated).expect_err("decode must reject truncated payload");
}

#[test]
fn rejects_corrupted_offset_range() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    bytes[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    let _ = decode_ion(&bytes).expect_err("decode must reject invalid section offset");
}

#[test]
fn rejects_flipped_header_crc32() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    bytes[HEADER_CRC32_OFFSET] ^= 0xFF;
    let err = decode_ion(&bytes).expect_err("decode must reject flipped header_crc32");
    assert!(
        err.contains("header_crc32") || err.contains("crc"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn rejects_corrupted_global_meta() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 0, false);
    let off = u64::from_le_bytes(
        bytes[HEADER_OFFSET_GLOBAL_META..HEADER_OFFSET_GLOBAL_META + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    let len = u64::from_le_bytes(
        bytes[HEADER_LEN_GLOBAL_META..HEADER_LEN_GLOBAL_META + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    assert!(len > 0, "fixture must have a global_meta section");
    bytes[off + len / 2] ^= 0xFF;
    let err = decode_ion(&bytes).expect_err("decode must reject corrupted global_meta");
    assert!(
        err.contains("global_meta"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn rejects_corrupted_trailer() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    let err = decode_ion(&bytes).expect_err("decode must reject corrupted trailer");
    assert!(err.contains("trailer"), "unexpected decode error: {err}");
}

#[test]
fn rejects_huge_spec_entry_count_4() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let off = read_header_u64(&bytes, HEADER_OFFSET_SPEC_ENTRIES) as usize;
    let len = read_header_u64(&bytes, HEADER_LEN_SPEC_ENTRIES) as usize;
    assert!(len >= 16, "fixture must have at least one spec entry");
    bytes[off + 8..off + 16].copy_from_slice(&u64::MAX.to_le_bytes());
    let err = decode_ion(&bytes).expect_err("decode must reject a huge spec entry count");
    assert!(
        err.contains("spec_entries") || err.contains("overflow"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn rejects_huge_spec_entry_count_with_checksums_off_4() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 0, false);
    let off = read_header_u64(&bytes, HEADER_OFFSET_SPEC_ENTRIES) as usize;
    let len = read_header_u64(&bytes, HEADER_LEN_SPEC_ENTRIES) as usize;
    assert!(len >= 16, "fixture must have at least one spec entry");
    let config = ReadOptions {
        verify_checksums: false,
        ..ReadOptions::default()
    };

    bytes[off + 8..off + 16].copy_from_slice(&u64::MAX.to_le_bytes());
    let err = IonReader::from_bytes(&bytes, &config.clone())
        .map(|_| ())
        .expect_err("decode must reject a huge spec entry count without checksums");
    assert!(err.contains("overflow"), "unexpected decode error: {err}");

    let table_len = read_header_u64(&bytes, HEADER_LEN_SPEC_ARRAY_ADDRESSES);
    let wrapping_count = table_len / 32 + (1u64 << 59);
    bytes[off..off + 8].copy_from_slice(&0u64.to_le_bytes());
    bytes[off + 8..off + 16].copy_from_slice(&wrapping_count.to_le_bytes());
    let err = IonReader::from_bytes(&bytes, &config)
        .map(|_| ())
        .expect_err("decode must reject a count whose product wraps to the table size");
    assert!(err.contains("overflow"), "unexpected decode error: {err}");
}

#[test]
fn rejects_huge_chrom_entry_count_4() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let off = read_header_u64(&bytes, HEADER_OFFSET_CHROM_ENTRIES) as usize;
    let len = read_header_u64(&bytes, HEADER_LEN_CHROM_ENTRIES) as usize;
    assert!(len >= 16, "fixture must have at least one chrom entry");
    bytes[off + 8..off + 16].copy_from_slice(&u64::MAX.to_le_bytes());
    let err = decode_ion(&bytes).expect_err("decode must reject a huge chrom entry count");
    assert!(
        err.contains("chrom_entries") || err.contains("overflow"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn rejects_block_directory_range_that_wraps_past_u64_5() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let entry = spec_block_directory_entry_offset(&bytes);
    write_u64_at(&mut bytes, entry, u64::MAX - 4);
    write_u64_at(&mut bytes, entry + 8, 8);
    let err = decode_ion_without_checksum_verification(&bytes)
        .expect_err("decode must reject a block payload range that wraps past u64::MAX");
    assert!(
        err.contains("payload exceeds container bounds"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn rejects_block_directory_offset_at_u64_max_5() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let entry = spec_block_directory_entry_offset(&bytes);
    write_u64_at(&mut bytes, entry, u64::MAX);
    write_u64_at(&mut bytes, entry + 8, 1);
    let err = decode_ion_without_checksum_verification(&bytes)
        .expect_err("decode must reject a block payload offset at u64::MAX");
    assert!(
        err.contains("payload exceeds container bounds"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn rejects_block_directory_size_beyond_container_5() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let entry = spec_block_directory_entry_offset(&bytes);
    write_u64_at(&mut bytes, entry + 8, u64::MAX / 2);
    let err = decode_ion_without_checksum_verification(&bytes)
        .expect_err("decode must reject a block payload size beyond the container");
    assert!(
        err.contains("payload exceeds container bounds"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn rejects_block_declaring_huge_uncompressed_size_6() {
    let mut bytes = encode_to_ion(test_files::tiny_pwiz_11(), 9, false);
    let entry = spec_block_directory_entry_offset(&bytes);
    write_u64_at(&mut bytes, entry + 16, 1 << 30);
    let err = decode_ion_without_checksum_verification(&bytes)
        .expect_err("decode must reject an implausible declared uncompressed size");
    assert!(
        err.contains("implausible"),
        "unexpected decode error: {err}"
    );
}

#[test]
fn valid_file_still_round_trips() {
    let src = test_files::tiny_pwiz_11();
    for level in [0, 9] {
        let bytes = encode_to_ion(src, level, false);
        let decoded = decode_ion(&bytes).expect("valid file must open");
        assert_mzml_semantic_eq(src, &decoded);
    }
}
