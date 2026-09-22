use crate::ion::decoder::decode::{IonReader, Metadatum, ReadOptions};

fn read_file(path: &str) -> Vec<u8> {
    let full = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read(&full).unwrap_or_else(|error| panic!("cannot read {full:?}: {error}"))
}

pub(super) fn spectra_metadata(path: &str) -> Vec<Metadatum> {
    IonReader::from_bytes(&read_file(path), &ReadOptions::default())
        .expect("open")
        .spectrum_metadata()
        .expect("read spectra metadata")
}

pub(super) fn chromatograms_metadata(path: &str) -> Vec<Metadatum> {
    IonReader::from_bytes(&read_file(path), &ReadOptions::default())
        .expect("open")
        .chromatogram_metadata()
        .expect("read chromatograms metadata")
}
