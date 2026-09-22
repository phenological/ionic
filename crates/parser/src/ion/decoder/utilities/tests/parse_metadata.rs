use std::{fs, path::PathBuf};

use crate::ion::{
    DecompressionLimit,
    attr_meta::*,
    decoder::decode::{Metadatum, MetadatumValue},
    utilities::{Header, parse_metadata},
};
use crate::mzml::schema::TagId;

const PATH: &str = "data/ion/test.ion";

fn read_bytes(path: &str) -> Vec<u8> {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read(&full).unwrap_or_else(|e| panic!("cannot read {:?}: {}", full, e))
}

fn parse_metadata_section_from_test_file(
    start_off: u64,
    end_off: u64,
    item_count: u32,
    expected_item_count: u32,
    meta_count: u32,
    num_count: u32,
    str_count: u32,
    codec_id: u8,
    expected_uncompressed: u64,
    section_name: &str,
) -> Vec<Metadatum> {
    let bytes = read_bytes(PATH);

    let c0 = start_off as usize;
    let c1 = end_off as usize;

    assert!(
        c0 < c1,
        "invalid metadata offsets for {section_name}: start >= end"
    );
    assert!(
        c1 <= bytes.len(),
        "invalid metadata offsets for {section_name}: end out of bounds"
    );

    assert_eq!(
        item_count, expected_item_count,
        "test.ion should contain {expected_item_count} {section_name} items"
    );

    let slice = &bytes[c0..c1];

    let expected = if codec_id == parse_metadata::CODEC_ZSTD {
        usize::try_from(expected_uncompressed)
            .unwrap_or_else(|_| panic!("{section_name}: expected_uncompressed overflow"))
    } else {
        0
    };

    let meta = parse_metadata(
        slice,
        item_count,
        meta_count,
        num_count,
        str_count,
        codec_id,
        expected,
        DecompressionLimit::default(),
    )
    .expect("parse_metadata failed");

    meta
}

#[test]
fn check_first_spectrum() {
    let header_bytes = read_bytes(PATH);
    let header = Header::parse(&header_bytes).expect("Header::parse failed");
    let spec_meta = parse_metadata_section_from_test_file(
        header.off_spec_meta,
        header.off_chrom_meta,
        header.spectrum_count,
        2,
        header.spec_meta_count,
        header.spec_meta_numeric_count,
        header.spec_meta_string_count,
        header.compression_codec,
        header.spec_meta_uncompressed_bytes,
        "spectra",
    );

    assert_eq!(item_meta_count(&spec_meta, 0), 30);

    expect_text_one(
        &spec_meta,
        0,
        TagId::Spectrum,
        CV_CODE_ATTR,
        ACC_ATTR_ID,
        CV_CODE_UNKNOWN,
        0,
        "scan=1",
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::Spectrum,
        CV_CODE_ATTR,
        ACC_ATTR_INDEX,
        CV_CODE_UNKNOWN,
        0,
        0.0,
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::Spectrum,
        CV_CODE_ATTR,
        ACC_ATTR_DEFAULT_ARRAY_LENGTH,
        CV_CODE_UNKNOWN,
        0,
        340032.0,
    );

    expect_number_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000511,
        CV_CODE_UNKNOWN,
        0,
        1.0,
    );
    expect_empty_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000579,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_empty_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000130,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000505,
        CV_CODE_UNKNOWN,
        0,
        24998.0,
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000285,
        CV_CODE_UNKNOWN,
        0,
        440132.0,
    );
    expect_empty_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000128,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::ScanList,
        CV_CODE_ATTR,
        ACC_ATTR_COUNT,
        CV_CODE_UNKNOWN,
        0,
        1.0,
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000016,
        CV_CODE_UO,
        10,
        0.191,
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000501,
        CV_CODE_MS,
        1000040,
        30.0,
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000500,
        CV_CODE_MS,
        1000040,
        1000.0,
    );
    expect_number_one(
        &spec_meta,
        0,
        TagId::BinaryDataArrayList,
        CV_CODE_ATTR,
        ACC_ATTR_COUNT,
        CV_CODE_UNKNOWN,
        0,
        2.0,
    );
    expect_empty_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000523,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_empty_count(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000576,
        CV_CODE_UNKNOWN,
        0,
        2,
    );
    expect_empty_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000514,
        CV_CODE_MS,
        1000040,
    );
    expect_empty_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000521,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_empty_one(
        &spec_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000515,
        CV_CODE_MS,
        1000131,
    );
}

#[test]
fn check_second_spectrum() {
    let header_bytes = read_bytes(PATH);
    let header = Header::parse(&header_bytes).expect("Header::parse failed");
    let spec_meta = parse_metadata_section_from_test_file(
        header.off_spec_meta,
        header.off_chrom_meta,
        header.spectrum_count,
        2,
        header.spec_meta_count,
        header.spec_meta_numeric_count,
        header.spec_meta_string_count,
        header.compression_codec,
        header.spec_meta_uncompressed_bytes,
        "spectra",
    );

    assert_eq!(item_meta_count(&spec_meta, 1), 39);

    expect_text_one(
        &spec_meta,
        1,
        TagId::Spectrum,
        CV_CODE_ATTR,
        ACC_ATTR_ID,
        CV_CODE_UNKNOWN,
        0,
        "scan=3476",
    );

    expect_number_one(
        &spec_meta,
        1,
        TagId::Spectrum,
        CV_CODE_ATTR,
        ACC_ATTR_INDEX,
        CV_CODE_UNKNOWN,
        0,
        1.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::Spectrum,
        CV_CODE_ATTR,
        ACC_ATTR_DEFAULT_ARRAY_LENGTH,
        CV_CODE_UNKNOWN,
        0,
        4340.0,
    );

    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000511,
        CV_CODE_UNKNOWN,
        0,
        2.0,
    );
    expect_empty_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000580,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_empty_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000130,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000505,
        CV_CODE_UNKNOWN,
        0,
        20032.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000285,
        CV_CODE_UNKNOWN,
        0,
        359026.0,
    );
    expect_empty_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000127,
        CV_CODE_UNKNOWN,
        0,
    );

    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000016,
        CV_CODE_UO,
        10,
        452.262,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000501,
        CV_CODE_MS,
        1000040,
        30.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000500,
        CV_CODE_MS,
        1000040,
        1000.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::PrecursorList,
        CV_CODE_ATTR,
        ACC_ATTR_COUNT,
        CV_CODE_UNKNOWN,
        0,
        1.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000827,
        CV_CODE_MS,
        1000040,
        515.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000828,
        CV_CODE_MS,
        1000040,
        485.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000829,
        CV_CODE_MS,
        1000040,
        485.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::SelectedIonList,
        CV_CODE_ATTR,
        ACC_ATTR_COUNT,
        CV_CODE_UNKNOWN,
        0,
        1.0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000744,
        CV_CODE_MS,
        1000040,
        515.0,
    );
    expect_empty_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1001880,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_number_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000045,
        CV_CODE_UNKNOWN,
        0,
        20.0,
    );
    expect_empty_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000523,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_empty_count(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000576,
        CV_CODE_UNKNOWN,
        0,
        2,
    );
    expect_empty_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000514,
        CV_CODE_MS,
        1000040,
    );
    expect_empty_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000521,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_empty_one(
        &spec_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000515,
        CV_CODE_MS,
        1000131,
    );
}

fn assert_chromatogram_binary_data_array_list(meta: &[Metadatum], item_index: u32) {
    expect_empty_one(
        meta,
        item_index,
        TagId::CvParam,
        CV_CODE_MS,
        1000523,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_empty_count(
        meta,
        item_index,
        TagId::CvParam,
        CV_CODE_MS,
        1000576,
        CV_CODE_UNKNOWN,
        0,
        3,
    );
    expect_empty_one(
        meta,
        item_index,
        TagId::CvParam,
        CV_CODE_MS,
        1000595,
        CV_CODE_UO,
        10,
    );

    expect_empty_one(
        meta,
        item_index,
        TagId::CvParam,
        CV_CODE_MS,
        1000521,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_empty_one(
        meta,
        item_index,
        TagId::CvParam,
        CV_CODE_MS,
        1000515,
        CV_CODE_MS,
        1000131,
    );

    expect_empty_one(
        meta,
        item_index,
        TagId::CvParam,
        CV_CODE_MS,
        1000522,
        CV_CODE_UNKNOWN,
        0,
    );
    expect_text_one(
        meta,
        item_index,
        TagId::CvParam,
        CV_CODE_MS,
        1000786,
        CV_CODE_UO,
        186,
        "ms level",
    );
}

#[test]
fn check_first_chromatogram() {
    let header_bytes = read_bytes(PATH);
    let header = Header::parse(&header_bytes).expect("Header::parse failed");
    let chrom_meta = parse_metadata_section_from_test_file(
        header.off_chrom_meta,
        header.off_global_meta,
        header.chrom_count,
        2,
        header.chrom_meta_count,
        header.chrom_meta_numeric_count,
        header.chrom_meta_string_count,
        header.compression_codec,
        header.chrom_meta_uncompressed_bytes,
        "chromatograms",
    );
    assert_eq!(chrom_meta.len(), 45);
    assert_eq!(item_meta_count(&chrom_meta, 0), 24);
    expect_text_one(
        &chrom_meta,
        0,
        TagId::Chromatogram,
        CV_CODE_ATTR,
        ACC_ATTR_ID,
        CV_CODE_UNKNOWN,
        0,
        "TIC",
    );
    expect_number_one(
        &chrom_meta,
        0,
        TagId::Chromatogram,
        CV_CODE_ATTR,
        ACC_ATTR_INDEX,
        CV_CODE_UNKNOWN,
        0,
        0.0,
    );
    expect_number_one(
        &chrom_meta,
        0,
        TagId::Chromatogram,
        CV_CODE_ATTR,
        ACC_ATTR_DEFAULT_ARRAY_LENGTH,
        CV_CODE_UNKNOWN,
        0,
        3476.0,
    );

    expect_empty_one(
        &chrom_meta,
        0,
        TagId::CvParam,
        CV_CODE_MS,
        1000235,
        CV_CODE_UNKNOWN,
        0,
    );

    assert_chromatogram_binary_data_array_list(&chrom_meta, 0);
}

#[test]
fn check_second_chromatogram() {
    let header_bytes = read_bytes(PATH);
    let header = Header::parse(&header_bytes).expect("Header::parse failed");
    let chrom_meta = parse_metadata_section_from_test_file(
        header.off_chrom_meta,
        header.off_global_meta,
        header.chrom_count,
        2,
        header.chrom_meta_count,
        header.chrom_meta_numeric_count,
        header.chrom_meta_string_count,
        header.compression_codec,
        header.chrom_meta_uncompressed_bytes,
        "chromatograms",
    );

    assert_eq!(chrom_meta.len(), 45);
    assert_eq!(item_meta_count(&chrom_meta, 1), 21);

    expect_text_one(
        &chrom_meta,
        1,
        TagId::Chromatogram,
        CV_CODE_ATTR,
        ACC_ATTR_ID,
        CV_CODE_UNKNOWN,
        0,
        "BPC",
    );
    expect_number_one(
        &chrom_meta,
        1,
        TagId::Chromatogram,
        CV_CODE_ATTR,
        ACC_ATTR_INDEX,
        CV_CODE_UNKNOWN,
        0,
        1.0,
    );
    expect_number_one(
        &chrom_meta,
        1,
        TagId::Chromatogram,
        CV_CODE_ATTR,
        ACC_ATTR_DEFAULT_ARRAY_LENGTH,
        CV_CODE_UNKNOWN,
        0,
        3476.0,
    );

    expect_empty_one(
        &chrom_meta,
        1,
        TagId::CvParam,
        CV_CODE_MS,
        1000628,
        CV_CODE_UNKNOWN,
        0,
    );

    assert_chromatogram_binary_data_array_list(&chrom_meta, 1);
}

fn item_meta_count(meta: &[Metadatum], item_index: u32) -> usize {
    let mut n = 0;
    for m in meta {
        if m.item_index == item_index {
            n += 1;
        }
    }
    n
}

fn find_meta_all<'a>(
    meta: &'a [Metadatum],
    item_index: u32,
    tag_id: TagId,
    ref_id: u8,
    accession_tail: AccessionTail,
) -> Vec<&'a Metadatum> {
    let expected_accession = format_accession(ref_id, accession_tail.raw());

    let mut out = Vec::new();
    for m in meta {
        if m.item_index == item_index
            && m.tag_id == tag_id
            && m.accession.as_deref() == expected_accession.as_deref()
        {
            out.push(m);
        }
    }
    out
}

fn find_meta_one<'a>(
    meta: &'a [Metadatum],
    item_index: u32,
    tag_id: TagId,
    ref_id: u8,
    accession_tail: AccessionTail,
) -> &'a Metadatum {
    let hits = find_meta_all(meta, item_index, tag_id, ref_id, accession_tail);
    if hits.len() != 1 {
        let expected_accession = format_accession(ref_id, accession_tail.raw());
        panic!(
            "expected exactly 1 metadatum, found {}: item_index={}, tag_id={:?}, accession={:?}",
            hits.len(),
            item_index,
            tag_id,
            expected_accession
        );
    }
    hits[0]
}
fn expect_text_one(
    meta: &[Metadatum],
    item_index: u32,
    tag_id: TagId,
    ref_id: u8,
    accession_tail: AccessionTail,
    unit_ref_id: u8,
    unit_accession_tail: u32,
    expected: &str,
) {
    let m = find_meta_one(meta, item_index, tag_id, ref_id, accession_tail);

    let expected_unit_accession = format_accession(unit_ref_id, unit_accession_tail);
    assert_eq!(
        m.unit_accession.as_deref(),
        expected_unit_accession.as_deref()
    );

    match &m.value {
        MetadatumValue::Text(s) => assert_eq!(s.as_str(), expected),
        other => panic!("expected Text({expected:?}), got {other:?} for {m:?}"),
    }
}

fn expect_number_one(
    meta: &[Metadatum],
    item_index: u32,
    tag_id: TagId,
    ref_id: u8,
    accession_tail: AccessionTail,
    unit_ref_id: u8,
    unit_accession_tail: u32,
    expected: f64,
) {
    let m = find_meta_one(meta, item_index, tag_id, ref_id, accession_tail);

    let expected_unit_accession = format_accession(unit_ref_id, unit_accession_tail);
    assert_eq!(
        m.unit_accession.as_deref(),
        expected_unit_accession.as_deref()
    );

    match &m.value {
        MetadatumValue::Number(v) => {
            let tol = 1e-9_f64.max(expected.abs() * 1e-9);
            assert!(
                (v - expected).abs() <= tol,
                "expected Number({expected}), got Number({v}) for {m:?}"
            );
        }
        other => panic!("expected Number({expected}), got {other:?} for {m:?}"),
    }
}

fn expect_empty_one(
    meta: &[Metadatum],
    item_index: u32,
    tag_id: TagId,
    ref_id: u8,
    accession_tail: AccessionTail,
    unit_ref_id: u8,
    unit_accession_tail: u32,
) {
    let m = find_meta_one(meta, item_index, tag_id, ref_id, accession_tail);

    let expected_unit_accession = format_accession(unit_ref_id, unit_accession_tail);
    assert_eq!(
        m.unit_accession.as_deref(),
        expected_unit_accession.as_deref()
    );

    match &m.value {
        MetadatumValue::Empty => {}
        other => panic!("expected Empty, got {other:?} for {m:?}"),
    }
}

fn expect_empty_count(
    meta: &[Metadatum],
    item_index: u32,
    tag_id: TagId,
    ref_id: u8,
    accession_tail: AccessionTail,
    unit_ref_id: u8,
    unit_accession_tail: u32,
    expected_count: usize,
) {
    let hits = find_meta_all(meta, item_index, tag_id, ref_id, accession_tail);

    assert_eq!(
        hits.len(),
        expected_count,
        "unexpected metadatum count for item_index={}, tag_id={:?}, accession={:?}",
        item_index,
        tag_id,
        format_accession(ref_id, accession_tail.raw())
    );

    let expected_unit_accession = format_accession(unit_ref_id, unit_accession_tail);
    for m in hits {
        assert_eq!(
            m.unit_accession.as_deref(),
            expected_unit_accession.as_deref()
        );
        match &m.value {
            MetadatumValue::Empty => {}
            other => panic!("expected Empty, got {other:?} for {m:?}"),
        }
    }
}
