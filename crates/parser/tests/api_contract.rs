mod common;

use std::sync::Arc;

use common::{
    assertions::{assert_mzml_semantic_eq, assert_mzml_structural_eq},
    decode_ion, encode_to_ion, test_files,
};
use ionic::{
    ByteRange, BytesSource, CallbackSource, Range, ScanSource,
    ion::{
        IonError, IonReader, IonWriter, MemoryReader, ReadOptions, SectionStorage,
        TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions, open_ranges,
    },
};

fn default_config() -> ReadOptions {
    ReadOptions::default()
}

fn segmented_write_config() -> WriteOptions {
    WriteOptions {
        compression_level: 0,
        force_f32: false,
        block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
        parallel: false,
        section_storage: SectionStorage::Memory,
        mz_window: 5.0,
    }
}

fn stream_write_config() -> WriteOptions {
    WriteOptions {
        compression_level: 0,
        force_f32: false,
        block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
        parallel: false,
        section_storage: SectionStorage::Memory,
        mz_window: 0.0,
    }
}

fn encode_with_segments(src: &ionic::mzml::structs::MzML) -> Vec<u8> {
    let mut bytes = Vec::new();
    let config = segmented_write_config();
    let mut writer = IonWriter::create(&mut bytes, config).expect("writer begin must succeed");
    let mut reader = MemoryReader::new(src.clone());
    writer
        .write_stream(&mut reader)
        .expect("write_stream must succeed");
    bytes
}

fn windowed_write_config() -> WriteOptions {
    WriteOptions {
        compression_level: 9,
        force_f32: false,
        block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
        parallel: false,
        section_storage: SectionStorage::Memory,
        mz_window: 50.0,
    }
}

fn encode_windowed(src: &ionic::mzml::structs::MzML) -> Vec<u8> {
    let mut bytes = Vec::new();
    let config = windowed_write_config();
    let mut writer = IonWriter::create(&mut bytes, config).expect("writer begin must succeed");
    let mut reader = MemoryReader::new(src.clone());
    writer
        .write_stream(&mut reader)
        .expect("write_stream must succeed");
    bytes
}

#[test]
fn windowed_roundtrip_gives_semantic_equal_mzml() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_windowed(src);
    let decoded = decode_ion(&bytes).expect("decode must succeed");
    assert_mzml_semantic_eq(src, &decoded);
}

#[test]
fn reencode_through_decode_preserves_data() {
    let src = test_files::tiny_pwiz_11();
    let first = encode_to_ion(src, 9, false);
    let decoded_first = decode_ion(&first).expect("decode first must succeed");
    let second = encode_windowed(&decoded_first);
    let decoded_second = decode_ion(&second).expect("decode second must succeed");
    assert_mzml_semantic_eq(src, &decoded_second);
}

#[test]
fn root_level_imports_cover_a_full_read_workflow() {
    let src = test_files::tiny_pwiz_11();

    let mut bytes = Vec::new();
    ionic::write_mzml_to_ion(src, ionic::WriteOptions::default(), &mut bytes)
        .expect("encode must succeed");

    let mut reader =
        ionic::IonReader::open(&bytes, ionic::ReadOptions::default()).expect("open must succeed");
    assert!(reader.spectrum_count() > 0);

    let _spectrum: Option<ionic::Spectrum> = reader.spectrum(0).expect("get_spectrum");

    reader.require_bounds().expect("bounds must load");
    let peaks: ionic::DataXY = reader
        .read_window(0, Range { from: 0.0, to: 1e9 })
        .expect("read_mz_range");
    assert_eq!(peaks.x.len(), peaks.y.len());

    let _mzml: ionic::MzML = reader.to_mzml().expect("to_mzml");
}

#[test]
fn encode_is_deterministic() {
    let src = test_files::tiny_pwiz_11();
    let first = encode_to_ion(src, 12, false);
    let second = encode_to_ion(src, 12, false);
    assert_eq!(
        first, second,
        "two identical encodes must produce byte-identical output"
    );
}

#[test]
fn roundtrip_gives_semantic_equal_mzml() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 9, false);
    let decoded = decode_ion(&bytes).expect("decode must succeed");
    assert_mzml_semantic_eq(src, &decoded);
}

#[test]
fn roundtrip_f32_mode_preserves_structure() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 9, true);
    let decoded = decode_ion(&bytes).expect("decode must succeed");
    assert_mzml_structural_eq(src, &decoded);
}

#[test]
fn open_bytes_to_mzml_matches_reference() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let reference = decode_ion(&bytes).expect("reference decode must succeed");
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open_bytes must succeed");
    let result = decoder.to_mzml().expect("to_mzml must succeed");
    assert_mzml_semantic_eq(&reference, &result);
}

#[test]
fn open_via_callback_to_mzml_matches_reference() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let reference = decode_ion(&bytes).expect("reference decode must succeed");
    let shared: Arc<[u8]> = Arc::from(bytes.as_slice());
    let callback_bytes = shared.clone();
    let mut decoder = IonReader::open_source(
        Arc::new(CallbackSource::new(move |range: ByteRange| {
            let start = range.offset as usize;
            let end = start + range.length as usize;
            callback_bytes
                .get(start..end)
                .map(|slice| slice.to_vec())
                .ok_or_else(|| IonError::from("callback: read out of bounds"))
        })),
        default_config(),
    )
    .expect("open_remote must succeed");
    let result = decoder.to_mzml().expect("to_mzml must succeed");
    assert_mzml_semantic_eq(&reference, &result);
}

#[test]
fn arc_and_callback_open_paths_give_equal_results() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);

    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut arc_decoder =
        IonReader::open_source(Arc::new(BytesSource::new(arc.clone())), default_config())
            .expect("open_bytes must succeed");
    let arc_mzml = arc_decoder.to_mzml().expect("arc to_mzml must succeed");

    let callback_data = arc.clone();
    let mut cb_decoder = IonReader::open_source(
        Arc::new(CallbackSource::new(move |range: ByteRange| {
            let start = range.offset as usize;
            let end = start + range.length as usize;
            callback_data
                .get(start..end)
                .map(|slice| slice.to_vec())
                .ok_or_else(|| IonError::from("callback: out of bounds"))
        })),
        default_config(),
    )
    .expect("open_remote must succeed");
    let cb_mzml = cb_decoder.to_mzml().expect("callback to_mzml must succeed");

    assert_mzml_semantic_eq(&arc_mzml, &cb_mzml);
}

#[test]
fn open_source_with_bytes_source_shares_the_arc_instead_of_copying() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let shared: Arc<[u8]> = Arc::from(bytes.as_slice());
    let pointer_before = Arc::as_ptr(&shared);
    let strong_count_before = Arc::strong_count(&shared);

    let reader =
        IonReader::open_source(Arc::new(BytesSource::new(shared.clone())), default_config())
            .expect("open_source must succeed");

    assert_eq!(
        Arc::as_ptr(&shared),
        pointer_before,
        "the shared buffer must stay at the same address"
    );
    assert!(
        Arc::strong_count(&shared) > strong_count_before,
        "BytesSource must hold a clone of the Arc rather than a fresh copy of the bytes"
    );

    drop(reader);
    assert_eq!(
        Arc::strong_count(&shared),
        strong_count_before,
        "dropping the reader must release the shared clone"
    );
}

#[test]
fn spectrum_count_and_chromatogram_count_are_stable() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");
    let spec_count = decoder.spectrum_count();
    let chrom_count = decoder.chromatogram_count();
    assert!(spec_count > 0, "spectrum count must be positive");
    let _ = chrom_count;
}

#[test]
fn scan_source_summary_count_equals_spectrum_count() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");
    let total = decoder.spectrum_count();
    let mut counted = 0usize;
    decoder.for_each_summary(&mut |_, _| counted += 1);
    assert_eq!(
        counted as u64, total,
        "for_each_summary count must equal spectrum_count"
    );
}

#[test]
fn load_scan_gives_arrays_for_first_spectrum() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");
    let mut mz = Vec::new();
    let mut intensity = Vec::new();
    let loaded = decoder.load_scan(0, &mut mz, &mut intensity);
    assert!(loaded, "load_scan must return true for index 0");
    assert!(!mz.is_empty(), "mz must not be empty");
    assert_eq!(
        mz.len(),
        intensity.len(),
        "mz and intensity must have equal length"
    );
}

#[test]
fn plan_open_ranges_includes_header_range() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let ranges = open_ranges(&bytes).expect("open_ranges must succeed");
    assert!(!ranges.is_empty(), "ranges must not be empty");
    assert_eq!(ranges[0].offset, 0, "first range must start at byte 0");
    assert_eq!(ranges[0].length, 1024, "first range must span 1024 bytes");
}

#[test]
fn plan_open_ranges_all_ranges_fit_in_file() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let file_len = bytes.len() as u64;
    let ranges = open_ranges(&bytes).expect("open_ranges must succeed");
    for range in &ranges {
        let end = range.offset + range.length;
        assert!(
            end <= file_len,
            "range (offset={}, length={}) extends past file end ({file_len})",
            range.offset,
            range.length,
        );
    }
}

#[test]
fn write_via_memory_reader_matches_direct_encode() {
    let src = test_files::tiny_pwiz_11();
    let direct_bytes = encode_to_ion(src, 0, false);
    let reference = decode_ion(&direct_bytes).expect("reference decode must succeed");

    let mut stream_bytes = Vec::new();
    let mut reader = MemoryReader::new(src.clone());
    let mut writer =
        IonWriter::create(&mut stream_bytes, stream_write_config()).expect("begin must succeed");
    writer
        .write_stream(&mut reader)
        .expect("write_stream must succeed");
    let stream_decoded = decode_ion(&stream_bytes).expect("stream decode must succeed");
    assert_mzml_semantic_eq(&reference, &stream_decoded);
}

#[test]
fn require_bounds_succeeds_for_segmented_file() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_with_segments(src);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");
    decoder
        .require_bounds()
        .expect("require_bounds must succeed when file has window bounds");
}

#[test]
fn mz_range_read_returns_data_for_segmented_file() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_with_segments(src);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");
    decoder
        .require_bounds()
        .expect("require_bounds must succeed");
    let window = decoder
        .read_window(0, Range { from: 0.0, to: 1e9 })
        .expect("read_mz_range must succeed");
    assert_eq!(
        window.x.len(),
        window.y.len(),
        "window mz and intensity must have equal length"
    );
    assert!(
        !window.x.is_empty(),
        "wide window must return data for a non-empty spectrum"
    );
}

#[test]
fn mz_range_matches_get_spectrum_for_full_range() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_with_segments(src);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");
    decoder
        .require_bounds()
        .expect("require_bounds must succeed");

    let spectrum = decoder
        .spectrum(0)
        .expect("get_spectrum must succeed")
        .expect("first spectrum must exist");
    let full_len = spectrum
        .binary_data_array_list
        .as_ref()
        .and_then(|list| list.binary_data_arrays.first())
        .and_then(|bda| bda.binary.as_ref())
        .map(|bin| bin.len())
        .unwrap_or(0);

    let window = decoder
        .read_window(0, Range { from: 0.0, to: 1e9 })
        .expect("wide window must succeed");
    assert_eq!(
        window.x.len(),
        full_len,
        "wide mz window must return as many points as the full spectrum"
    );
}

#[test]
fn mz_range_block_ranges_are_non_empty_for_segmented_file() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_with_segments(src);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");
    decoder
        .require_bounds()
        .expect("require_bounds must succeed");
    let ranges = decoder
        .byte_ranges(0, Range { from: 0.0, to: 1e9 })
        .expect("plan_mz_range must succeed");
    assert!(
        !ranges.is_empty(),
        "wide window must return at least one block range"
    );
}

#[test]
fn spec_summary_matches_source_facts() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");

    let source_spectra = common::spectra(src);
    assert_eq!(
        decoder.spectrum_count() as usize,
        source_spectra.len(),
        "summary count must match source spectrum count"
    );

    for (index, source) in source_spectra.iter().enumerate() {
        let summary = decoder
            .spectrum_summary(index)
            .expect("spec_summary must return Some for a valid index");

        let expected_level = source.ms_level.unwrap_or(0) as u8;
        assert_eq!(
            summary.ms_level, expected_level,
            "spectrum {index}: stored ms_level must match source ms_level"
        );

        if let Some(expected_rt) = common::scan_start_time_raw(source) {
            assert!(
                (summary.rt - expected_rt).abs() <= 1e-6,
                "spectrum {index}: stored rt {} must match source raw value {expected_rt}",
                summary.rt,
            );
        }

        assert_eq!(
            summary.rt_unit,
            common::scan_start_time_unit_code(source),
            "spectrum {index}: stored rt_unit must match the source scan-start-time unit",
        );
    }
}

#[test]
fn format_version_is_supported() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 6, false);
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let decoder = IonReader::open_source(Arc::new(BytesSource::new(arc)), default_config())
        .expect("open must succeed");
    let version = decoder.format_version();
    assert!(
        ionic::ion::is_supported(version),
        "format_version must be within supported range"
    );
}

struct GoldenEncode {
    name: &'static str,
    path: &'static str,
    encoded_len: usize,
    encoded_fnv: u64,
}

const GOLDEN_ENCODES: &[GoldenEncode] = &[
    GoldenEncode {
        name: "tiny_pwiz_10",
        path: "crates/parser/data/mzml/tiny.pwiz.1.0.mzML",
        encoded_len: 11464,
        encoded_fnv: 0x8809_d202_308a_8999,
    },
    GoldenEncode {
        name: "tiny_pwiz_11",
        path: "crates/parser/data/mzml/tiny.pwiz.1.1.mzML",
        encoded_len: 12560,
        encoded_fnv: 0x537f_7db5_ad76_3e4c,
    },
    GoldenEncode {
        name: "anpc_test",
        path: "crates/parser/data/mzml/test.mzML",
        encoded_len: 8832,
        encoded_fnv: 0x7f6a_6713_6c50_2cb5,
    },
];

#[test]
fn encoded_bytes_stay_byte_for_byte_stable() {
    for golden in GOLDEN_ENCODES {
        let src = common::parse_test_file(golden.path);
        let bytes = encode_to_ion(&src, 0, false);
        assert_eq!(
            bytes.len(),
            golden.encoded_len,
            "{}: encoded length changed (format drift)",
            golden.name,
        );
        assert_eq!(
            common::fnv64_bytes(&bytes),
            golden.encoded_fnv,
            "{}: encoded bytes changed (format drift)",
            golden.name,
        );
    }
}

#[test]
fn spec_array_addresses_crc_tamper_fails_closed_with_verify_on_and_opens_with_verify_off() {
    let src = test_files::tiny_pwiz_11();
    let bytes = encode_to_ion(src, 0, false);

    let off_spec_array_addresses = u64::from_le_bytes(bytes[80..88].try_into().unwrap()) as usize;
    let len_spec_array_addresses = u64::from_le_bytes(bytes[88..96].try_into().unwrap()) as usize;
    assert!(
        len_spec_array_addresses > 0,
        "spec_array_addresses must be non-empty for this test to be meaningful"
    );

    let mut tampered = bytes.clone();
    tampered[off_spec_array_addresses] ^= 0xFF;

    let verify_on = ReadOptions {
        verify_checksums: true,
        ..ReadOptions::default()
    };
    match IonReader::open(&tampered, verify_on) {
        Ok(_) => {
            panic!("tampered spec_array_addresses must be rejected when verify_checksums is true")
        }
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("spec_array_addresses") && msg.contains("crc"),
                "error message must name spec_array_addresses and crc, got: {msg}"
            );
        }
    }

    let verify_off = ReadOptions {
        verify_checksums: false,
        ..ReadOptions::default()
    };
    IonReader::open(&tampered, verify_off)
        .expect("tampered spec_array_addresses must open when verify_checksums is false");
}

#[test]
fn public_api_surface_is_frozen() {
    let mz = ionic::Range { from: 0.0, to: 1.0 };
    let _byte = ionic::ByteRange {
        offset: 0,
        length: 0,
    };
    let pixel = ionic::Pixel {
        x: mz,
        y: mz,
        z: mz,
    };
    let _all = ionic::Select::All;
    let _rt = ionic::Select::Rt(mz);
    let _area = ionic::Select::Area(pixel);

    fn time_unit_is_frozen(unit: ionic::TimeUnit) -> u8 {
        match unit {
            ionic::TimeUnit::Second => 1,
            ionic::TimeUnit::Minute => 2,
            ionic::TimeUnit::Millisecond => 3,
            ionic::TimeUnit::Other => 0,
        }
    }
    let _ = time_unit_is_frozen(ionic::TimeUnit::Second);

    fn numeric_array_is_frozen(array: &ionic::NumericArray) -> usize {
        match array {
            ionic::NumericArray::F64(values) => values.len(),
            ionic::NumericArray::F32(values) => values.len(),
            ionic::NumericArray::F16(values) => values.len(),
            ionic::NumericArray::I16(values) => values.len(),
            ionic::NumericArray::I32(values) => values.len(),
            ionic::NumericArray::I64(values) => values.len(),
        }
    }

    let data = ionic::DataXY {
        x: ionic::NumericArray::F64(vec![1.0]),
        y: ionic::NumericArray::F64(vec![2.0]),
    };
    let _: Vec<f64> = data.x.to_f64();
    let _ = numeric_array_is_frozen(&data.y);

    let src = test_files::tiny_pwiz_11();
    let mut bytes = Vec::new();
    ionic::write_mzml_to_ion(src, ionic::WriteOptions::default(), &mut bytes)
        .expect("write_mzml_to_ion must succeed");
    let mut reader =
        ionic::IonReader::open(&bytes, ionic::ReadOptions::default()).expect("open must succeed");

    let _spectrum_count: u64 = reader.spectrum_count();
    let _chromatogram_count: u64 = reader.chromatogram_count();

    reader.for_each_summary(&mut |index: usize, summary: ionic::ScanSummary| {
        let _: usize = index;
        let _: f64 = summary.rt;
        let _: ionic::TimeUnit = summary.rt_unit;
        let _: u8 = summary.ms_level;
        let _: u32 = summary.position_x;
    });

    let _read: ionic::DataXY = reader.read_window(0, mz).expect("read_window must succeed");

    reader
        .scans_in(
            mz,
            ionic::Select::All,
            None,
            &mut |window: &ionic::Window| {
                let _: usize = window.index;
                let _: &ionic::ScanSummary = window.summary;
                let _: &[f64] = window.mz;
                let _: &[f64] = window.intensity;
            },
        )
        .expect("scans_in must succeed");

    let _spectrum: Option<ionic::Spectrum> = reader.spectrum(0).expect("spectrum must succeed");
    let _ranges: Vec<ionic::ByteRange> =
        reader.byte_ranges(0, mz).expect("byte_ranges must succeed");
    let _mzml: ionic::MzML = reader.to_mzml().expect("to_mzml must succeed");

    let mut output = Vec::new();
    let mut writer = ionic::IonWriter::create(&mut output, ionic::WriteOptions::default())
        .expect("create must succeed");
    let mut stream = ionic::MemoryReader::new(reader.to_mzml().expect("to_mzml must succeed"));
    writer
        .write_stream(&mut stream)
        .expect("write_stream must succeed");

    let mut output2 = Vec::new();
    ionic::IonWriter::create(&mut output2, ionic::WriteOptions::default())
        .expect("create must succeed")
        .write_mzml(src)
        .expect("write_mzml must succeed");

    let _open: Vec<ionic::ByteRange> =
        ionic::open_ranges(&bytes[..1024]).expect("open_ranges must succeed");
}
