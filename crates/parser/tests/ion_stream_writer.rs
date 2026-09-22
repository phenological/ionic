mod common;

use std::fs;

use common::{canonical_diff_paths, decode_ion};
use ionic::{
    IonReader, IonWriter, ReadOptions, SectionStorage, WriteOptions,
    mzml::{MzmlReader, parse_mzml::parse_mzml, structs::MzML},
};

fn config() -> WriteOptions {
    WriteOptions {
        compression_level: 0,
        force_f32: false,
        block_size: WriteOptions::default().block_size,
        parallel: false,
        section_storage: SectionStorage::Memory,
        mz_window: 0.0,
    }
}

fn coordinate_xml() -> &'static [u8] {
    br#"
<mzML>
  <run id="coords">
    <spectrumList count="1">
      <spectrum index="0" id="scan=1" defaultArrayLength="0">
        <scanList count="1">
          <scan>
            <cvParam cvRef="IMS" accession="IMS:1000050" name="position x" value="11"/>
            <cvParam cvRef="IMS" accession="IMS:1000051" name="position y" value="22"/>
            <cvParam cvRef="IMS" accession="IMS:1000052" name="position z" value="3"/>
          </scan>
        </scanList>
      </spectrum>
    </spectrumList>
  </run>
</mzML>
"#
}

fn mixed_xml() -> &'static [u8] {
    br#"
<mzML>
  <run id="mixed">
    <spectrumList count="1" defaultDataProcessingRef="dp">
      <spectrum index="0" id="scan=1" defaultArrayLength="0"/>
    </spectrumList>
    <chromatogramList count="1" defaultDataProcessingRef="dp">
      <chromatogram index="0" id="tic" defaultArrayLength="0"/>
    </chromatogramList>
  </run>
</mzML>
"#
}

#[test]
fn stream_writer_roundtrips_like_mzml_writer() {
    let mzml = parse_mzml(coordinate_xml()).unwrap();
    let mut reader = MzmlReader::from_mzml(mzml.clone());
    let mut bytes = Vec::new();
    let mut writer = IonWriter::to(&mut bytes, &MzML::default(), &config()).unwrap();
    writer.write_stream(&mut reader).unwrap();
    let decoded = decode_ion(&bytes).unwrap();
    let diffs = canonical_diff_paths(&mzml, &decoded);
    assert!(diffs.is_empty(), "{diffs:#?}");
}

#[test]
fn spectrum_summary_keeps_coordinates() {
    let mzml = parse_mzml(coordinate_xml()).unwrap();
    let mut reader = MzmlReader::from_mzml(mzml);
    let mut bytes = Vec::new();
    let mut writer = IonWriter::to(&mut bytes, &MzML::default(), &config()).unwrap();
    writer.write_stream(&mut reader).unwrap();
    let ion = IonReader::from_bytes(&bytes, &ReadOptions::default()).unwrap();
    let summary = ion.spectrum_summary(0).unwrap();
    assert_eq!(summary.position_x, 11);
    assert_eq!(summary.position_y, 22);
    assert_eq!(summary.position_z, 3);
}

#[test]
fn file_stream_reader_roundtrips_spectra_then_chromatograms() {
    let path =
        std::env::temp_dir().join(format!("ionic-stream-reader-{}.mzML", std::process::id()));
    fs::write(&path, mixed_xml()).unwrap();

    let mzml = parse_mzml(mixed_xml()).unwrap();
    let mut reader = MzmlReader::open(&path).unwrap();
    let mut bytes = Vec::new();
    let mut writer = IonWriter::to(&mut bytes, &MzML::default(), &config()).unwrap();
    writer.write_stream(&mut reader).unwrap();
    let decoded = decode_ion(&bytes).unwrap();
    let diffs = canonical_diff_paths(&mzml, &decoded);
    let _ = fs::remove_file(&path);
    assert!(diffs.is_empty(), "{diffs:#?}");
}
