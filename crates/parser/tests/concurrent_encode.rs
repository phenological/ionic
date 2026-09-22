mod common;

use std::{sync::Arc, thread};

use common::{parse_test_file, test_files::PWIZ_TEST_FILES, write_mzml_to_ion};
use ionic::{IonReader, ReadOptions, SectionStorage, WriteOptions, mzml::structs::MzML};

const THREADS: usize = 4;

fn spilling_config() -> WriteOptions {
    WriteOptions {
        compression_level: 3,
        section_storage: SectionStorage::Disk,
        ..Default::default()
    }
}

fn encode(source: &MzML) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_mzml_to_ion(source, spilling_config(), &mut bytes).expect("encode must succeed");
    bytes
}

fn encode_on_threads(source: Arc<MzML>, threads: usize) -> Vec<Vec<u8>> {
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let source = Arc::clone(&source);
            thread::spawn(move || encode(&source))
        })
        .collect();
    handles
        .into_iter()
        .map(|handle| handle.join().expect("encoder thread must not panic"))
        .collect()
}

#[test]
fn encoding_the_same_file_on_many_threads_gives_identical_bytes() {
    let source = Arc::new(parse_test_file(PWIZ_TEST_FILES[4]).clone());
    let reference = encode(&source);

    for (thread_index, bytes) in encode_on_threads(source, THREADS).into_iter().enumerate() {
        assert_eq!(
            bytes.len(),
            reference.len(),
            "thread {thread_index} produced a different length than a single-threaded encode"
        );
        assert!(
            bytes == reference,
            "thread {thread_index} produced different bytes than a single-threaded encode"
        );
    }
}

#[test]
fn every_concurrently_encoded_file_keeps_its_spectrum_bounds() {
    let source = Arc::new(parse_test_file(PWIZ_TEST_FILES[4]).clone());
    let expected_count = encode(&source);
    let expected_count = IonReader::from_bytes(&expected_count, &ReadOptions::default())
        .expect("reference open must succeed")
        .spectrum_count();

    for (thread_index, bytes) in encode_on_threads(source, THREADS).into_iter().enumerate() {
        let mut reader = IonReader::from_bytes(&bytes, &ReadOptions::default()).unwrap_or_else(|error| {
            panic!("thread {thread_index} wrote an unreadable file: {error}")
        });
        assert_eq!(
            reader.spectrum_count(),
            expected_count,
            "thread {thread_index} lost spectra"
        );
        reader.require_bounds().unwrap_or_else(|error| {
            panic!("thread {thread_index} wrote a file with no usable spectrum bounds: {error}")
        });
    }
}

#[test]
fn encoding_different_files_at_the_same_time_keeps_every_result_valid() {
    let sources: Vec<Arc<MzML>> = PWIZ_TEST_FILES
        .iter()
        .map(|path| Arc::new(parse_test_file(path).clone()))
        .collect();

    let expected: Vec<u64> = sources
        .iter()
        .map(|source| {
            IonReader::from_bytes(&encode(source), &ReadOptions::default())
                .expect("reference open must succeed")
                .spectrum_count()
        })
        .collect();

    let handles: Vec<_> = sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| thread::spawn(move || (index, encode(&source))))
        .collect();

    for handle in handles {
        let (index, bytes) = handle.join().expect("encoder thread must not panic");
        let name = PWIZ_TEST_FILES[index];
        let mut reader = IonReader::from_bytes(&bytes, &ReadOptions::default())
            .unwrap_or_else(|error| panic!("{name} came back unreadable: {error}"));
        assert_eq!(
            reader.spectrum_count(),
            expected[index],
            "{name} lost spectra"
        );
        reader
            .require_bounds()
            .unwrap_or_else(|error| panic!("{name} has no usable spectrum bounds: {error}"));
    }
}
