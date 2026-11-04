mod common;

use common::{
    helpers::{minimal_file_description, synthetic_binary_data_array},
    write_mzml_to_ion,
};
use ionic::{ByteRangesOptions, IonReader, Range, ReadOptions, WriteOptions, mzml::structs::*};

const MZ_ARRAY: u32 = 1000514;
const INTENSITY_ARRAY: u32 = 1000515;
const CUSTOM_ARRAY: u32 = 1000786;
const MISSING_ARRAY: u32 = 1000999;
const WHOLE_SCAN: Range = Range {
    from: 0.0,
    to: 100_000.0,
};

fn build_array(accession: &str, values: Vec<f64>) -> BinaryDataArray {
    let length = values.len();
    let mut array = synthetic_binary_data_array(
        accession,
        NumericType::Float64,
        NumericArray::F64(values),
        Some(length),
    );
    array.cv_params[0].name = "custom data array".to_string();
    array
}

fn build_spectrum(index: u32, mz: &[f64], custom: &[f64]) -> Spectrum {
    let intensity: Vec<f64> = mz.iter().map(|value| value * 2.0).collect();
    Spectrum {
        id: format!("scan={}", index + 1),
        index: Some(index),
        default_array_length: Some(mz.len()),
        binary_data_array_list: Some(BinaryDataArrayList {
            count: Some(3),
            binary_data_arrays: vec![
                build_array("MS:1000514", mz.to_vec()),
                build_array("MS:1000515", intensity),
                build_array("MS:1000786", custom.to_vec()),
            ],
        }),
        ..Default::default()
    }
}

fn build_file(spectra: Vec<Spectrum>) -> Vec<u8> {
    let mzml = MzML {
        file_description: Some(minimal_file_description()),
        run: Run {
            id: "byte-ranges-options".to_string(),
            spectrum_list: Some(SpectrumList {
                count: Some(spectra.len()),
                spectra,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bytes = Vec::new();
    write_mzml_to_ion(&mzml, WriteOptions::default(), &mut bytes).expect("write ion");
    bytes
}

fn open_file(bytes: &[u8]) -> IonReader {
    IonReader::from_bytes(bytes, &ReadOptions::default()).expect("open ion")
}

fn only(array: u32) -> ByteRangesOptions {
    ByteRangesOptions {
        arrays: vec![array],
    }
}

fn fingerprint() -> Vec<f64> {
    (0..256).map(|value| value as f64 / 256.0 - 0.5).collect()
}

fn fingerprint_file(spectrum_count: u32) -> Vec<u8> {
    let mz = [100.0, 200.0, 300.0];
    build_file(
        (0..spectrum_count)
            .map(|index| build_spectrum(index, &mz, &fingerprint()))
            .collect(),
    )
}

#[test]
fn default_options_are_mz_and_intensity() {
    assert_eq!(
        ByteRangesOptions::default().arrays,
        vec![MZ_ARRAY, INTENSITY_ARRAY]
    );
}

#[test]
fn default_covers_mz_and_intensity_only() {
    let bytes = fingerprint_file(2);
    let mut reader = open_file(&bytes);
    let mut both = reader.byte_ranges(0, WHOLE_SCAN, &only(MZ_ARRAY)).unwrap();
    both.extend(
        reader
            .byte_ranges(0, WHOLE_SCAN, &only(INTENSITY_ARRAY))
            .unwrap(),
    );
    both.sort_unstable_by_key(|range| (range.offset, range.length));
    both.dedup();
    assert_eq!(
        reader
            .byte_ranges(0, WHOLE_SCAN, &ByteRangesOptions::default())
            .unwrap(),
        both
    );
    assert!(!both.is_empty());
}

#[test]
fn custom_array_has_its_own_ranges_inside_the_file() {
    let bytes = fingerprint_file(3);
    let mut reader = open_file(&bytes);
    let peaks = reader
        .byte_ranges(0, WHOLE_SCAN, &ByteRangesOptions::default())
        .unwrap();
    for scan_index in 0..3 {
        let ranges = reader
            .byte_ranges(scan_index, WHOLE_SCAN, &only(CUSTOM_ARRAY))
            .unwrap();
        assert!(!ranges.is_empty());
        for range in &ranges {
            assert!(range.length > 0);
            assert!(range.offset + range.length <= bytes.len() as u64);
            assert!(!peaks.contains(range));
        }
    }
}

#[test]
fn array_without_windows_ignores_the_range() {
    let bytes = fingerprint_file(1);
    let mut reader = open_file(&bytes);
    let narrow = Range {
        from: 150.0,
        to: 151.0,
    };
    let whole = reader
        .byte_ranges(0, WHOLE_SCAN, &only(CUSTOM_ARRAY))
        .unwrap();
    assert_eq!(
        reader.byte_ranges(0, narrow, &only(CUSTOM_ARRAY)).unwrap(),
        whole
    );
}

#[test]
fn aligned_array_follows_the_range() {
    let mz: Vec<f64> = (0..400).map(|value| 100.0 + value as f64 * 2.0).collect();
    let aligned: Vec<f64> = mz.iter().map(|value| value / 10.0).collect();
    let bytes = build_file(vec![build_spectrum(0, &mz, &aligned)]);
    let mut reader = open_file(&bytes);
    let narrow = Range {
        from: 120.0,
        to: 125.0,
    };
    let some = reader.byte_ranges(0, narrow, &only(CUSTOM_ARRAY)).unwrap();
    let every = reader
        .byte_ranges(0, WHOLE_SCAN, &only(CUSTOM_ARRAY))
        .unwrap();
    assert!(!some.is_empty());
    assert!(some.iter().all(|range| every.contains(range)));
    assert_eq!(reader.spectrum_array(0, CUSTOM_ARRAY).unwrap(), aligned);
}

#[test]
fn custom_array_reads_back_exactly() {
    let bytes = fingerprint_file(2);
    let mut reader = open_file(&bytes);
    assert_eq!(
        reader.spectrum_array(1, CUSTOM_ARRAY).unwrap(),
        fingerprint()
    );
}

#[test]
fn missing_array_has_no_ranges() {
    let bytes = fingerprint_file(2);
    let mut reader = open_file(&bytes);
    assert!(
        reader
            .byte_ranges(0, WHOLE_SCAN, &only(MISSING_ARRAY))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn scan_out_of_range_is_an_error() {
    let bytes = fingerprint_file(2);
    let mut reader = open_file(&bytes);
    assert!(
        reader
            .byte_ranges(2, WHOLE_SCAN, &only(CUSTOM_ARRAY))
            .is_err()
    );
}

#[test]
fn no_arrays_gives_no_ranges() {
    let bytes = fingerprint_file(1);
    let mut reader = open_file(&bytes);
    assert!(
        reader
            .byte_ranges(0, WHOLE_SCAN, &ByteRangesOptions { arrays: Vec::new() })
            .unwrap()
            .is_empty()
    );
}
