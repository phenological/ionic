mod common;

use std::path::PathBuf;

use common::helpers::{build_mzml, make_chromatogram_f64, make_spectrum_f64};
use ionic::{
    ArrayKind, ChromatogramSummary, ConvertKind, ConvertOptions, DataXY, DecompressionLimit,
    IonError, IonReader, IonResult, IonWriter, Metadatum, MetadatumValue, Range, ReadOptions,
    ScanQuery, ScanStream, ScanSummary, SectionStorage, Select, SpectrumSummary, TimeUnit, Window,
    WriteOptions,
    mzml::structs::MzML,
};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ionic-api-surface-{}-{name}", std::process::id()))
}

fn sample_mzml() -> MzML {
    let mz: Vec<f64> = (0..50).map(|i| 100.0 + i as f64 * 0.5).collect();
    let intensity: Vec<f64> = (0..50).map(|i| i as f64).collect();
    let time: Vec<f64> = (0..10).map(|i| i as f64 * 0.1).collect();
    let tic: Vec<f64> = (0..10).map(|i| i as f64 * 10.0).collect();
    build_mzml(
        vec![make_spectrum_f64("scan=1", mz, intensity)],
        vec![make_chromatogram_f64("tic", time, tic)],
    )
}

#[test]
fn root_read_and_write_round_trip_a_file() {
    let path = temp_path("read_write.ion");
    let mzml = sample_mzml();

    ionic::write(&path, &mzml, &WriteOptions::default()).expect("write must succeed");
    let round_tripped = ionic::read(&path).expect("read must succeed");
    let _ = std::fs::remove_file(&path);

    assert_eq!(round_tripped.run.spectrum_list.unwrap().spectra.len(), 1);
}

#[test]
fn create_write_spectrum_finish_round_trips_and_open_reads_it_back() {
    let path = temp_path("create_push.ion");
    let mz: Vec<f64> = (0..20).map(|i| 200.0 + i as f64).collect();
    let intensity: Vec<f64> = (0..20).map(|i| i as f64 * 2.0).collect();
    let spectrum = make_spectrum_f64("scan=1", mz.clone(), intensity);
    let metadata = build_mzml(vec![], vec![]);

    let mut writer =
        IonWriter::create(&path, &metadata, &WriteOptions::default()).expect("create must succeed");
    writer
        .write_spectrum(&spectrum)
        .expect("write_spectrum must succeed");
    writer.finish().expect("finish must succeed");

    let mut reader = IonReader::open(&path, &ReadOptions::default()).expect("open must succeed");
    let _ = std::fs::remove_file(&path);

    assert_eq!(reader.spectrum_count(), 1);
    let got_mz = reader.array(0, ArrayKind::Mz).expect("array must succeed");
    assert_eq!(got_mz, mz);
}

#[test]
fn array_into_reuses_the_callers_buffer() {
    let mz: Vec<f64> = (0..30).map(|i| 300.0 + i as f64).collect();
    let intensity: Vec<f64> = (0..30).map(|i| i as f64).collect();
    let spectrum = make_spectrum_f64("scan=1", mz.clone(), intensity);
    let mzml = build_mzml(vec![], vec![]);

    let mut bytes = Vec::new();
    let mut writer = IonWriter::to(&mut bytes, &mzml, &WriteOptions::default()).unwrap();
    writer.write_spectrum(&spectrum).unwrap();
    writer.finish().unwrap();

    let mut reader = IonReader::from_bytes(&bytes, &ReadOptions::default()).unwrap();
    let mut buf = Vec::with_capacity(4096);
    let addr_before = buf.as_ptr();
    reader.array_into(0, ArrayKind::Mz, &mut buf).unwrap();

    assert_eq!(buf, mz);
    assert_eq!(
        buf.as_ptr(),
        addr_before,
        "array_into must reuse the caller's allocation"
    );
}

#[test]
fn convert_and_convert_file_round_trip_bytes_and_paths() {
    let mzml = sample_mzml();
    let mut ion_bytes = Vec::new();
    let mut writer = IonWriter::to(&mut ion_bytes, &mzml, &WriteOptions::default()).unwrap();
    for spectrum in &mzml.run.spectrum_list.as_ref().unwrap().spectra {
        writer.write_spectrum(spectrum).unwrap();
    }
    for chromatogram in &mzml.run.chromatogram_list.as_ref().unwrap().chromatograms {
        writer.write_chromatogram(chromatogram).unwrap();
    }
    writer.finish().unwrap();

    let xml = ionic::convert(&ion_bytes, &ConvertOptions::default()).expect("convert must succeed");
    assert!(xml.starts_with(b"<?xml"));

    let ion_path = temp_path("convert.ion");
    let xml_path = temp_path("convert.mzml");
    std::fs::write(&ion_path, &ion_bytes).unwrap();
    ionic::convert_file(&ion_path, &xml_path, &ConvertOptions::default())
        .expect("convert_file must succeed");
    let xml_from_file = std::fs::read(&xml_path).unwrap();
    let _ = std::fs::remove_file(&ion_path);
    let _ = std::fs::remove_file(&xml_path);

    assert!(xml_from_file.starts_with(b"<?xml"));
}

#[test]
fn scans_in_and_metadata_are_reachable() {
    let mz: Vec<f64> = (0..15).map(|i| 400.0 + i as f64).collect();
    let intensity: Vec<f64> = (0..15).map(|i| i as f64).collect();
    let spectrum = make_spectrum_f64("scan=1", mz, intensity);
    let mzml = build_mzml(vec![], vec![]);

    let mut bytes = Vec::new();
    let mut writer = IonWriter::to(&mut bytes, &mzml, &WriteOptions::default()).unwrap();
    writer.write_spectrum(&spectrum).unwrap();
    writer.finish().unwrap();

    let mut reader = IonReader::from_bytes(&bytes, &ReadOptions::default()).unwrap();
    let mut seen = 0usize;
    reader
        .scans_in(
            &ScanQuery {
                mz: Range {
                    from: 0.0,
                    to: f64::MAX,
                },
                select: Select::All,
                ms_level: None,
            },
            |_window| seen += 1,
        )
        .unwrap();
    assert_eq!(seen, 1);

    let metadata: MzML = reader.metadata().unwrap();
    assert_eq!(metadata.run.id, "test-run");

    let summaries: Vec<SpectrumSummary> = reader.spectrum_summaries().unwrap();
    assert_eq!(summaries.len(), 1);
    let chrom_summaries: Vec<ChromatogramSummary> = reader.chromatogram_summaries().unwrap();
    assert!(chrom_summaries.is_empty());
}

#[test]
fn root_items_are_reachable_and_usable() {
    let _kind: ConvertKind = ConvertKind::Auto;
    let _options = ConvertOptions::default();
    let _read_options = ReadOptions::default();
    let _write_options = WriteOptions::default();
    let _array_kind = ArrayKind::Mz;
    let _range = Range { from: 0.0, to: 1.0 };
    let _select = Select::All;
    let _section_storage = SectionStorage::Memory;
    let _decompression_limit = DecompressionLimit::default();
    let _query = ScanQuery::default();

    let error: IonError = IonError::from("example error");
    let _result: IonResult<()> = Err(error);

    fn _accepts_scan_stream(_stream: &mut dyn ScanStream) {}

    fn _accepts_window(window: &Window) {
        let _index: usize = window.index;
        let _summary: &ScanSummary = window.summary;
        let _mz: &[f64] = window.mz;
        let _intensity: &[f64] = window.intensity;
    }

    fn _accepts_data_xy(data: &DataXY) {
        let _x = data.x.to_f64();
        let _y = data.y.to_f64();
    }

    fn _accepts_summaries(spectrum: &SpectrumSummary, chromatogram: &ChromatogramSummary) {
        let _rt: f64 = spectrum.rt;
        let _ms_level: u8 = spectrum.ms_level;
        let _lowest_mz: f64 = chromatogram.lowest_mz;
    }

    fn _accepts_metadatum(metadatum: &Metadatum) {
        let _id: u32 = metadatum.id;
        match &metadatum.value {
            MetadatumValue::Number(_) | MetadatumValue::Text(_) | MetadatumValue::Empty => {}
            _ => {}
        }
    }

    fn _accepts_time_unit(_unit: TimeUnit) {}
}
