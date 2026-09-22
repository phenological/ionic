mod common;

use std::{fs, path::PathBuf};

use common::{
    encode_to_ion, parse_test_file, read_test_file, repo_root, test_files::ALL_TEST_FILES,
};
use ionic::{ConvertKind, ConvertOptions, IonReader, ReadOptions, format::FILE_SIGNATURE, mzml::bin_to_mzml};

const MZML: &str = "crates/parser/data/mzml/tiny.pwiz.1.1.mzML";
const ION: &str = "crates/parser/data/ion/tiny.msdata.mzML0.99.9.ion";

fn fixture_path(relative_path: &str) -> PathBuf {
    repo_root().join(relative_path)
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ionic-convert-{}-{name}", std::process::id()))
}

fn options_with_kind(kind: ConvertKind) -> ConvertOptions {
    ConvertOptions {
        kind,
        ..Default::default()
    }
}

fn expected_ion_bytes() -> Vec<u8> {
    encode_to_ion(&parse_test_file(MZML), 12, false)
}

fn expected_mzml_xml() -> Vec<u8> {
    let mut reader = IonReader::open(fixture_path(ION), &ReadOptions::default()).unwrap();
    bin_to_mzml(&reader.to_mzml().unwrap()).unwrap()
}

fn copy_fixture(relative_path: &str, name: &str) -> PathBuf {
    let path = temp_path(name);
    fs::write(&path, read_test_file(relative_path)).unwrap();
    path
}

#[test]
fn mzml_path_to_ion_path_writes_the_file() {
    let out = temp_path("a.ion");
    ionic::convert_file(fixture_path(MZML), &out, &ConvertOptions::default()).unwrap();
    let written = fs::read(&out).unwrap();
    fs::remove_file(&out).unwrap();
    assert_eq!(written, expected_ion_bytes());
}

#[test]
fn ion_path_to_mzml_path_writes_the_file() {
    let out = temp_path("b.mzML");
    ionic::convert_file(fixture_path(ION), &out, &ConvertOptions::default()).unwrap();
    let written = fs::read(&out).unwrap();
    fs::remove_file(&out).unwrap();
    assert_eq!(written, expected_mzml_xml());
}

#[test]
fn mzml_bytes_to_memory_returns_the_ion_bytes() {
    let result = ionic::convert(&read_test_file(MZML), &ConvertOptions::default()).unwrap();
    assert_eq!(result, expected_ion_bytes());
}

#[test]
fn ion_bytes_to_memory_returns_the_xml() {
    let result = ionic::convert(&read_test_file(ION), &ConvertOptions::default()).unwrap();
    assert_eq!(result, expected_mzml_xml());
}

#[test]
fn auto_reads_the_direction_from_the_file_signature_for_a_buffer() {
    let ion = ionic::convert(&read_test_file(MZML), &ConvertOptions::default()).unwrap();
    assert!(ion.starts_with(&FILE_SIGNATURE));
    let xml = ionic::convert(&read_test_file(ION), &ConvertOptions::default()).unwrap();
    assert!(xml.starts_with(b"<?xml"));
}

#[test]
fn auto_reads_the_direction_from_the_file_signature_for_a_path_without_extension() {
    let mzml_path = copy_fixture(MZML, "no-extension-mzml");
    let ion_path = copy_fixture(ION, "no-extension-ion");
    let ion_out = temp_path("no-extension-ion-out.bin");
    let xml_out = temp_path("no-extension-xml-out.bin");

    ionic::convert_file(&mzml_path, &ion_out, &ConvertOptions::default()).unwrap();
    ionic::convert_file(&ion_path, &xml_out, &ConvertOptions::default()).unwrap();

    let ion_bytes = fs::read(&ion_out).unwrap();
    let xml_bytes = fs::read(&xml_out).unwrap();
    for path in [&mzml_path, &ion_path, &ion_out, &xml_out] {
        let _ = fs::remove_file(path);
    }
    assert!(ion_bytes.starts_with(&FILE_SIGNATURE));
    assert!(xml_bytes.starts_with(b"<?xml"));
}

#[test]
fn auto_reads_the_direction_from_the_file_signature_for_an_unknown_extension() {
    let path = copy_fixture(ION, "unknown.dat");
    let out = temp_path("unknown-out.bin");
    ionic::convert_file(&path, &out, &ConvertOptions::default()).unwrap();
    let bytes = fs::read(&out).unwrap();
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&out);
    assert!(bytes.starts_with(b"<?xml"));
}

#[test]
fn auto_trusts_the_extension_over_the_file_signature() {
    let ion_named_mzml = copy_fixture(ION, "wrong.mzML");
    let mzml_named_ion = copy_fixture(MZML, "wrong.ion");
    let out1 = temp_path("wrong-out1.bin");
    let out2 = temp_path("wrong-out2.bin");

    let first = ionic::convert_file(&ion_named_mzml, &out1, &ConvertOptions::default());
    let second = ionic::convert_file(&mzml_named_ion, &out2, &ConvertOptions::default());

    for path in [&ion_named_mzml, &mzml_named_ion, &out1, &out2] {
        let _ = fs::remove_file(path);
    }
    assert!(first.unwrap_err().to_string().contains("ion file"));
    assert!(second.unwrap_err().to_string().contains("header"));
}

#[test]
fn path_input_and_buffer_input_give_the_same_ion_bytes() {
    for relative_path in ALL_TEST_FILES {
        let out = temp_path("path-vs-bytes.ion");
        ionic::convert_file(fixture_path(relative_path), &out, &ConvertOptions::default())
            .unwrap();
        let from_path = fs::read(&out).unwrap();
        let _ = fs::remove_file(&out);
        let from_bytes =
            ionic::convert(&read_test_file(relative_path), &ConvertOptions::default()).unwrap();
        assert_eq!(from_path, from_bytes, "{relative_path}");
    }
}

#[test]
fn mzml_to_ion_refuses_ion_input() {
    let out = temp_path("refused.ion");
    let options = options_with_kind(ConvertKind::MzmlToIon);
    let from_bytes = ionic::convert(&read_test_file(ION), &options);
    let from_path = ionic::convert_file(fixture_path(ION), &out, &options);
    assert!(from_bytes.unwrap_err().to_string().contains("ion file"));
    assert!(from_path.unwrap_err().to_string().contains("ion file"));
    assert!(!out.exists());
}

#[test]
fn ion_to_mzml_refuses_mzml_input() {
    let from_bytes = ionic::convert(&read_test_file(MZML), &options_with_kind(ConvertKind::IonToMzml));
    let out = temp_path("refused.mzml");
    let from_path =
        ionic::convert_file(fixture_path(MZML), &out, &options_with_kind(ConvertKind::IonToMzml));
    assert!(from_bytes.unwrap_err().to_string().contains("header"));
    assert!(from_path.unwrap_err().to_string().contains("header"));
}

#[test]
fn empty_buffer_with_ion_kind_returns_an_error() {
    let empty: Vec<u8> = Vec::new();
    assert!(ionic::convert(&empty, &options_with_kind(ConvertKind::IonToMzml)).is_err());
}
