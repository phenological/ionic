use std::{fs, path::PathBuf};

use crate::ion::utilities::Header;

const PATH: &str = "data/ion/test.ion";

fn read_bytes(path: &str) -> Vec<u8> {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read(&full).unwrap_or_else(|e| panic!("cannot read {:?}: {}", full, e))
}

#[test]
fn check_header() {
    let bytes = read_bytes(PATH);
    let header = Header::parse(&bytes).expect("Header::parse failed");

    assert_eq!(header.spectrum_count, 2);
    assert_eq!(header.chrom_count, 2);
    assert_eq!(header.spec_meta_count, 71);
    assert_eq!(header.spec_meta_numeric_count, 2);
    assert_eq!(header.spec_meta_string_count, 35);
    assert_eq!(header.chrom_meta_count, 45);
    assert_eq!(header.chrom_meta_numeric_count, 0);
    assert_eq!(header.chrom_meta_string_count, 20);
    assert_eq!(header.spec_block_count, 2);
    assert_eq!(header.chrom_block_count, 3);
    assert_eq!(header.compression_codec, 1);
    assert_eq!(header.compression_level, 22);
    assert_eq!(header.default_array_filter, 1);
    assert!(header.spec_array_type_count > 0);
    assert!(header.chrom_array_type_count > 0);
    assert_eq!(header.target_block_uncompressed_bytes, 8 * 1024 * 1024);
    assert!(header.spec_meta_uncompressed_bytes > 0);
    assert!(header.chrom_meta_uncompressed_bytes > 0);
    assert!(header.global_meta_uncompressed_bytes > 0);
    assert_eq!(header.meta_group_size, 8192);
    assert_eq!(header.spec_meta_group_count, 1);
    assert_eq!(header.chrom_meta_group_count, 1);

    for &off in &[
        header.off_spec_summary,
        header.off_spec_entries,
        header.off_spec_array_addresses,
        header.off_chrom_summary,
        header.off_chrom_entries,
        header.off_chrom_array_addresses,
        header.off_spec_meta,
        header.off_chrom_meta,
        header.off_global_meta,
        header.off_spec_container,
        header.off_chrom_container,
    ] {
        assert!(off >= 1024, "offset {off} must be >= 1024");
        assert_eq!(off % 8, 0, "offset {off} must be 8-aligned");
    }

    assert!(
        header
            .off_spec_summary
            .saturating_add(header.len_spec_summary)
            <= header.off_spec_entries
    );
    assert!(
        header
            .off_spec_entries
            .saturating_add(header.len_spec_entries)
            <= header.off_spec_array_addresses
    );
    assert!(
        header
            .off_spec_array_addresses
            .saturating_add(header.len_spec_array_addresses)
            <= header.off_chrom_summary
    );
    assert!(
        header
            .off_chrom_summary
            .saturating_add(header.len_chrom_summary)
            <= header.off_chrom_entries
    );
    assert!(
        header
            .off_chrom_entries
            .saturating_add(header.len_chrom_entries)
            <= header.off_chrom_array_addresses
    );
    assert!(
        header
            .off_chrom_array_addresses
            .saturating_add(header.len_chrom_array_addresses)
            <= header.off_spec_meta
    );
    assert!(header.off_spec_meta.saturating_add(header.len_spec_meta) <= header.off_chrom_meta);
    assert!(header.off_chrom_meta.saturating_add(header.len_chrom_meta) <= header.off_global_meta);
    assert!(
        header
            .off_chrom_container
            .saturating_add(header.len_chrom_container)
            <= header.off_spec_summary
    );
    assert!(
        header
            .off_spec_container
            .saturating_add(header.len_spec_container)
            <= header.off_chrom_container
    );
    assert!(header.len_spec_container >= header.spec_block_count * 32);
    assert!(header.len_chrom_container >= header.chrom_block_count * 32);

    assert_eq!(&bytes[5..8], &[0u8; 3]);
    assert_eq!(&bytes[432..968], &[0u8; 536]);

    let len = bytes.len() as u64;
    for &off in &[
        header.off_spec_summary,
        header.off_spec_entries,
        header.off_spec_array_addresses,
        header.off_chrom_summary,
        header.off_chrom_entries,
        header.off_chrom_array_addresses,
        header.off_spec_meta,
        header.off_chrom_meta,
        header.off_global_meta,
        header.off_spec_container,
        header.off_chrom_container,
    ] {
        assert!(off < len, "offset {off} out of bounds (len={len})");
    }

    let end = header
        .off_chrom_container
        .saturating_add(header.len_chrom_container);
    assert!(
        end <= len,
        "container_chrom end out of bounds (end={end}, len={len})"
    );
}
