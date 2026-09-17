mod common;

use std::sync::Arc;

use ionic::{
    BytesSource,
    ion::{
        IonReader, IonWriter, MemoryReader, Range, ReadOptions, SectionStorage,
        TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions,
    },
    mzml::structs::{
        BinaryDataArray, BinaryDataArrayList, CvParam, MzML, NumericArray, Run, Spectrum,
        SpectrumList,
    },
};

fn write_config() -> WriteOptions {
    WriteOptions {
        compression_level: 0,
        force_f32: false,
        block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
        parallel: false,
        section_storage: SectionStorage::Memory,
        mz_window: 0.0,
    }
}

fn mz_cv_param() -> CvParam {
    CvParam {
        cv_ref: Some("MS".to_string()),
        accession: Some("MS:1000514".to_string()),
        name: "m/z array".to_string(),
        value: None,
        unit_cv_ref: None,
        unit_accession: None,
        unit_name: None,
    }
}

fn intensity_cv_param() -> CvParam {
    CvParam {
        cv_ref: Some("MS".to_string()),
        accession: Some("MS:1000515".to_string()),
        name: "intensity array".to_string(),
        value: None,
        unit_cv_ref: None,
        unit_accession: None,
        unit_name: None,
    }
}

fn mz_array(values: Vec<f64>) -> BinaryDataArray {
    BinaryDataArray {
        binary: Some(NumericArray::F64(values)),
        cv_params: vec![mz_cv_param()],
        ..BinaryDataArray::default()
    }
}

fn intensity_array(values: Vec<f64>) -> BinaryDataArray {
    BinaryDataArray {
        binary: Some(NumericArray::F64(values)),
        cv_params: vec![intensity_cv_param()],
        ..BinaryDataArray::default()
    }
}

fn mzml_with_spectra(spectra: Vec<Spectrum>) -> MzML {
    let count = spectra.len();
    MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                count: Some(count),
                default_data_processing_ref: Some("dp".to_string()),
                spectra,
            }),
            ..Run::default()
        },
        ..MzML::default()
    }
}

fn spectrum_with_mz(id: &str, mz: Vec<f64>, intensities: Vec<f64>) -> Spectrum {
    Spectrum {
        id: id.to_string(),
        index: Some(0),
        default_array_length: Some(mz.len()),
        binary_data_array_list: Some(BinaryDataArrayList {
            count: Some(2),
            binary_data_arrays: vec![mz_array(mz), intensity_array(intensities)],
        }),
        ..Spectrum::default()
    }
}

#[test]
fn write_mzml_rejects_unsorted_mz_spectrum() {
    let mzml = mzml_with_spectra(vec![spectrum_with_mz(
        "scan=1",
        vec![100.0, 99.0, 101.0],
        vec![1.0, 2.0, 3.0],
    )]);
    let mut bytes = Vec::new();
    let result = IonWriter::create(&mut bytes, write_config())
        .expect("begin must succeed")
        .write_mzml(&mzml);
    match result {
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("spectrum 0"),
                "error must name spectrum index 0, got: {msg}"
            );
            assert!(
                msg.contains("m/z array must be sorted ascending"),
                "error must describe the sorting requirement, got: {msg}"
            );
        }
        Ok(()) => panic!("write_mzml must reject a spectrum with unsorted m/z"),
    }
}

#[test]
fn write_stream_rejects_unsorted_mz_spectrum() {
    let mzml = mzml_with_spectra(vec![spectrum_with_mz(
        "scan=1",
        vec![100.0, 99.0, 101.0],
        vec![1.0, 2.0, 3.0],
    )]);
    let mut reader = MemoryReader::new(mzml);
    let mut bytes = Vec::new();
    let result = IonWriter::create(&mut bytes, write_config())
        .expect("begin must succeed")
        .write_stream(&mut reader);
    match result {
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("spectrum 0"),
                "error must name spectrum index 0, got: {msg}"
            );
            assert!(
                msg.contains("m/z array must be sorted ascending"),
                "error must describe the sorting requirement, got: {msg}"
            );
        }
        Ok(()) => panic!("write_stream must reject a spectrum with unsorted m/z"),
    }
}

#[test]
fn write_mzml_rejects_second_spectrum_unsorted_names_correct_index() {
    let mzml = mzml_with_spectra(vec![
        spectrum_with_mz("scan=1", vec![100.0, 200.0, 300.0], vec![1.0, 2.0, 3.0]),
        spectrum_with_mz("scan=2", vec![100.0, 99.0, 101.0], vec![1.0, 2.0, 3.0]),
    ]);
    let mut bytes = Vec::new();
    let result = IonWriter::create(&mut bytes, write_config())
        .expect("begin must succeed")
        .write_mzml(&mzml);
    match result {
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("spectrum 1"),
                "error must name spectrum index 1, got: {msg}"
            );
        }
        Ok(()) => panic!("write_mzml must reject the second spectrum with unsorted m/z"),
    }
}

#[test]
fn write_mzml_accepts_sorted_mz_spectrum_and_roundtrips() {
    let mzml = mzml_with_spectra(vec![spectrum_with_mz(
        "scan=1",
        vec![100.0, 200.0, 300.0],
        vec![1.0, 2.0, 3.0],
    )]);
    let mut bytes = Vec::new();
    IonWriter::create(&mut bytes, write_config())
        .expect("begin must succeed")
        .write_mzml(&mzml)
        .expect("sorted m/z must be accepted");
    assert!(!bytes.is_empty(), "output must be non-empty");
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    IonReader::open_source(Arc::new(BytesSource::new(arc)), ReadOptions::default())
        .expect("file must open after encoding sorted m/z");
}

#[test]
fn read_mz_range_works_on_sorted_spectrum() {
    let mzml = mzml_with_spectra(vec![spectrum_with_mz(
        "scan=1",
        vec![100.0, 200.0, 300.0],
        vec![1.0, 2.0, 3.0],
    )]);
    let mut bytes = Vec::new();
    IonWriter::create(&mut bytes, write_config())
        .expect("begin must succeed")
        .write_mzml(&mzml)
        .expect("sorted m/z must be accepted");
    let arc: Arc<[u8]> = Arc::from(bytes.as_slice());
    let mut decoder =
        IonReader::open_source(Arc::new(BytesSource::new(arc)), ReadOptions::default())
            .expect("open must succeed");
    decoder.require_bounds().expect("window bounds must exist");
    let window = decoder
        .read_window(
            0,
            Range {
                from: 150.0,
                to: 250.0,
            },
        )
        .expect("read_mz_range must succeed");
    let mz = window.x.to_f64();
    assert_eq!(
        mz.len(),
        1,
        "window [150, 250] must contain exactly one point (200.0)"
    );
    assert!((mz[0] - 200.0).abs() < 1e-9, "window point must be 200.0");
}

#[test]
fn write_mzml_accepts_equal_adjacent_mz_values() {
    let mzml = mzml_with_spectra(vec![spectrum_with_mz(
        "scan=1",
        vec![100.0, 100.0, 200.0],
        vec![1.0, 2.0, 3.0],
    )]);
    let mut bytes = Vec::new();
    IonWriter::create(&mut bytes, write_config())
        .expect("begin must succeed")
        .write_mzml(&mzml)
        .expect("non-decreasing (equal adjacent values) must be accepted");
    assert!(!bytes.is_empty());
}
