mod common;

use std::path::PathBuf;

use common::canonical_diff_paths;
use ionic::{
    ion::{
        IonReader, ReadOptions, SectionStorage, TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions,
        write_mzml_to_ion,
    },
    mzml::parse_mzml::parse_mzml,
};

fn decode_ion_bytes(bytes: &[u8]) -> ionic::mzml::structs::MzML {
    let mut ion = IonReader::open(bytes, ReadOptions::default()).unwrap();
    ion.to_mzml().unwrap()
}

fn mzml_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("mzml")
}

fn mzml_files() -> Vec<PathBuf> {
    std::fs::read_dir(mzml_dir())
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            let is_mzml = path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("mzml"));
            if is_mzml { Some(path) } else { None }
        })
        .collect()
}

fn config_for_level(compression_level: u8) -> WriteOptions {
    WriteOptions {
        compression_level,
        force_f32: false,
        block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
        parallel: false,
        section_storage: SectionStorage::Memory,
        mz_window: 0.0,
    }
}

#[test]
fn writer_roundtrips_plain_mzml_files() {
    check_writer_roundtrip_for_level(0);
}

#[test]
fn writer_roundtrips_compressed_mzml_files() {
    check_writer_roundtrip_for_level(3);
}

fn check_writer_roundtrip_for_level(compression_level: u8) {
    let files = mzml_files();
    assert!(!files.is_empty(), "no mzml fixtures found");

    for path in &files {
        let mzml_bytes = std::fs::read(path).unwrap();
        let mzml = parse_mzml(&mzml_bytes).unwrap();
        let config = config_for_level(compression_level);
        let mut ion_bytes = Vec::new();
        write_mzml_to_ion(&mzml, config, &mut ion_bytes).unwrap();
        let decoded = decode_ion_bytes(&ion_bytes);
        let diffs = canonical_diff_paths(&mzml, &decoded);
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(
            diffs.is_empty(),
            "{name}: writer roundtrip changed semantic data at compression level {compression_level}: {diffs:#?}"
        );
    }
}
