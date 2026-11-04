mod common;

use std::sync::{Arc, OnceLock};

use common::{
    helpers::{minimal_file_description, synthetic_ms_cv},
    write_mzml_to_ion,
};
use ionic::{
    IonError, IonReader, ReadOptions, WriteOptions,
    mzml::structs::*,
    source::{ByteRange, CallbackSource, header_ranges},
};

const GROUP_SIZE: usize = 8192;
const SPECTRUM_COUNT: usize = 2 * GROUP_SIZE + 4;

fn library_id(index: usize) -> String {
    format!("LIB-{index:05}")
}

fn build_spectrum(index: usize) -> Spectrum {
    let ms_level = if index.is_multiple_of(3) { "1" } else { "2" };
    Spectrum {
        id: format!("scan={}", index + 1),
        index: Some(index as u32),
        default_array_length: Some(0),
        cv_params: vec![synthetic_ms_cv("MS:1000511", Some(ms_level))],
        user_params: vec![UserParam {
            name: "library_id".to_string(),
            r#type: Some("xsd:string".to_string()),
            value: Some(library_id(index)),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn get_file() -> &'static Arc<[u8]> {
    static FILE: OnceLock<Arc<[u8]>> = OnceLock::new();
    FILE.get_or_init(|| {
        let spectra: Vec<Spectrum> = (0..SPECTRUM_COUNT).map(build_spectrum).collect();
        let mzml = MzML {
            file_description: Some(minimal_file_description()),
            run: Run {
                id: "spectrum-metadata-byte-ranges".to_string(),
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
        Arc::from(bytes)
    })
}

fn open_file(bytes: &[u8]) -> IonReader {
    IonReader::from_bytes(bytes, &ReadOptions::default()).expect("open ion")
}

fn open_with_only(bytes: &Arc<[u8]>, allowed: Vec<ByteRange>) -> IonReader {
    let input = bytes.clone();
    let source = CallbackSource::new(move |range: ByteRange| {
        let inside = allowed.iter().any(|allowed| {
            range.offset >= allowed.offset
                && range.offset + range.length <= allowed.offset + allowed.length
        });
        if !inside {
            return Err(IonError::from(format!(
                "read outside allowed ranges: {range:?}"
            )));
        }
        Ok(input[range.offset as usize..(range.offset + range.length) as usize].to_vec())
    });
    IonReader::new(Arc::new(source), &ReadOptions::default()).expect("open ion")
}

fn get_library_id(spectrum: &Spectrum) -> Option<&str> {
    spectrum
        .user_params
        .iter()
        .find(|param| param.name == "library_id")
        .and_then(|param| param.value.as_deref())
}

fn search_indices() -> [usize; 7] {
    [
        0,
        GROUP_SIZE / 2,
        GROUP_SIZE - 1,
        GROUP_SIZE,
        2 * GROUP_SIZE - 1,
        2 * GROUP_SIZE,
        SPECTRUM_COUNT - 1,
    ]
}

fn get_ranges(reader: &IonReader, index: usize) -> Vec<ByteRange> {
    reader.spectrum_metadata_byte_ranges(index).unwrap()
}

#[test]
fn spectra_fall_into_more_than_one_group() {
    let bytes = get_file();
    let reader = open_file(bytes);
    let first = get_ranges(&reader, 0);
    let second = get_ranges(&reader, GROUP_SIZE);
    let third = get_ranges(&reader, 2 * GROUP_SIZE);
    assert_eq!(first, get_ranges(&reader, GROUP_SIZE - 1));
    assert_eq!(second, get_ranges(&reader, 2 * GROUP_SIZE - 1));
    assert_eq!(third, get_ranges(&reader, SPECTRUM_COUNT - 1));
    assert_ne!(first, second);
    assert_ne!(first, third);
    assert_ne!(second, third);
}

#[test]
fn returned_ranges_are_enough_to_read_metadata() {
    let bytes = get_file();
    let open_ranges = header_ranges(&bytes[..1024]).unwrap();
    let ranges_reader = open_file(bytes);
    for index in search_indices() {
        let mut allowed = open_ranges.clone();
        allowed.extend(get_ranges(&ranges_reader, index));
        let mut reader = open_with_only(bytes, allowed);
        let spectrum = reader.spectrum_metadata_at(index).unwrap();
        assert_eq!(get_library_id(&spectrum), Some(library_id(index).as_str()));
    }
}

#[test]
fn open_ranges_alone_are_not_enough() {
    let bytes = get_file();
    let open_ranges = header_ranges(&bytes[..1024]).unwrap();
    let mut reader = open_with_only(bytes, open_ranges);
    assert!(reader.spectrum_metadata_at(0).is_err());
}

#[test]
fn out_of_range_index_matches_spectrum_metadata_at() {
    let bytes = get_file();
    let mut reader = open_file(bytes);
    let ranges = reader.spectrum_metadata_byte_ranges(SPECTRUM_COUNT);
    let metadata = reader.spectrum_metadata_at(SPECTRUM_COUNT);
    assert!(matches!(
        ranges,
        Err(IonError::OutOfRange { index, count })
            if index == SPECTRUM_COUNT && count == SPECTRUM_COUNT as u64
    ));
    assert!(matches!(
        metadata,
        Err(IonError::OutOfRange { index, count })
            if index == SPECTRUM_COUNT && count == SPECTRUM_COUNT as u64
    ));
}

#[test]
fn each_spectrum_has_one_range_inside_the_file() {
    let bytes = get_file();
    let reader = open_file(bytes);
    for index in search_indices() {
        let ranges = get_ranges(&reader, index);
        assert_eq!(ranges.len(), 1);
        assert!(ranges[0].length > 0);
        assert!(ranges[0].offset + ranges[0].length <= bytes.len() as u64);
    }
}
