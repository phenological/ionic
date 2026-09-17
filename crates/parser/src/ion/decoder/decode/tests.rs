use super::{arrays::decode_into, *};
use crate::{
    ion::{
        encoder::utilities::SectionStorage,
        format::{CODEC_NONE, CODEC_ZSTD},
    },
    mzml::structs::{BinaryDataArray, CvParam, NumericArray},
};

const BYTES: &[u8] = include_bytes!("../../../../data/ion/test.ion");

fn ref_with(array_type: u32, continues_previous_segment: u8, encoded_len: u32) -> ArrayAddress {
    ArrayAddress {
        block_id: 0,
        element_offset: 0,
        element_count: 4,
        array_type,
        dtype: FILE_DTYPE_F64,
        array_filter: 0,
        encoded_len,
        continues_previous_segment,
        array_cv_code: 0,
    }
}

#[test]
fn group_arrays_keeps_same_accession_logical_arrays_separate() {
    let refs = [ref_with(1000514, 0, 0), ref_with(1000514, 0, 0)];
    let groups = group_arrays(&refs).unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].refs.len(), 1);
    assert_eq!(groups[1].refs.len(), 1);
}

#[test]
fn group_arrays_joins_continuation_segments_into_one_group() {
    let refs = [ref_with(1000514, 0, 0), ref_with(1000514, 1, 0)];
    let groups = group_arrays(&refs).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].refs.len(), 2);
}

#[test]
fn group_arrays_errors_on_leading_continuation() {
    let refs = [ref_with(1000514, 1, 0)];
    assert!(group_arrays(&refs).is_err());
}

#[test]
fn group_arrays_errors_on_invalid_continues_value() {
    let refs = [ref_with(1000514, 0, 0), ref_with(1000514, 2, 0)];
    assert!(group_arrays(&refs).is_err());
}

#[test]
fn group_arrays_errors_on_multi_ref_variable_length() {
    let refs = [ref_with(1000514, 0, 8), ref_with(1000514, 1, 8)];
    assert!(group_arrays(&refs).is_err());
}

#[test]
fn group_arrays_errors_on_type_mismatch_in_continuation() {
    let mut second = ref_with(1000514, 1, 0);
    second.array_type = 1000515;
    let refs = [ref_with(1000514, 0, 0), second];
    assert!(group_arrays(&refs).is_err());
}

#[test]
fn new_reader_opens_old_fixture() {
    let reader = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    assert!(matches!(
        reader.spec_window_directory,
        WindowDirectoryCache::Unloaded
    ));
}

#[test]
fn to_mzml_preserves_declared_spectrum_count() {
    let mut reader = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let mzml = reader.to_mzml().unwrap();
    let list = mzml.run.spectrum_list.expect("spectrum list");
    assert_eq!(list.count, Some(3476));
    assert_eq!(list.spectra.len(), 2);
}

#[test]
fn to_mzml_keeps_data_type_cv_param_in_place() {
    let mut reader = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let mzml = reader.to_mzml().unwrap();
    let spectra = mzml.run.spectrum_list.expect("spectrum list").spectra;
    let array = &spectra[0]
        .binary_data_array_list
        .as_ref()
        .expect("binary data array list")
        .binary_data_arrays[0];
    assert_eq!(array.cv_params[0].accession.as_deref(), Some("MS:1000523"));
}

#[test]
fn get_spectrum_lazy_matches_full_conversion() {
    let mut reader = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let full = reader.to_mzml().unwrap();
    let full_spectra = full.run.spectrum_list.expect("spectrum list").spectra;
    for (index, full_spectrum) in full_spectra.iter().enumerate() {
        let lazy = reader.spectrum(index).unwrap().expect("spectrum present");
        assert_eq!(
            format!("{lazy:?}"),
            format!("{full_spectrum:?}"),
            "spectrum {index} differs between lazy and full paths"
        );
    }
}

#[test]
fn metadata_at_matches_filtered_full_read() {
    let mut reader = IonReader::open(BYTES, ReadOptions::default()).unwrap();

    let all_spectra = reader.spectrum_metadata().unwrap();
    for index in 0..reader.spectrum_count() as usize {
        let one = reader.spectrum_metadata_at(index).unwrap();
        let expected: Vec<_> = all_spectra
            .iter()
            .filter(|row| row.item_index as usize == index)
            .cloned()
            .collect();
        assert_eq!(format!("{one:?}"), format!("{expected:?}"));
    }

    let all_chroms = reader.chromatogram_metadata().unwrap();
    for index in 0..reader.chromatogram_count() as usize {
        let one = reader.chromatogram_metadata_at(index).unwrap();
        let expected: Vec<_> = all_chroms
            .iter()
            .filter(|row| row.item_index as usize == index)
            .cloned()
            .collect();
        assert_eq!(format!("{one:?}"), format!("{expected:?}"));
    }
}

#[test]
fn open_parses_header() {
    let d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    assert!(d.spectrum_count() > 0);
}

#[test]
fn summary_returns_none_out_of_bounds() {
    let d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    assert!(d.spectrum_summary(d.spectrum_count() as usize).is_none());
}

#[test]
fn summary_has_valid_rt() {
    let d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let r = d.spectrum_summary(0).unwrap();
    assert!(r.rt.is_finite() && r.rt >= 0.0);
    assert!(r.ms_level >= 1);
}

#[test]
fn array_addresses_contain_mz_and_intensity() {
    let d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let refs = d.spectrum_array_addresses(0).unwrap();
    assert!(refs.iter().any(|a| a.array_type == ACC_MZ));
    assert!(refs.iter().any(|a| a.array_type == ACC_INT));
}

#[test]
fn read_array_produces_mz_values() {
    let mut d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let refs = d.spectrum_array_addresses(0).unwrap();
    let mz_address = refs.iter().find(|a| a.array_type == ACC_MZ).unwrap();

    let mut mz = Vec::new();
    d.read_spectrum_values(mz_address, &mut mz).unwrap();

    assert!(!mz.is_empty());
    assert!(mz.iter().all(|v| v.is_finite()));
}

#[test]
fn for_each_scan_yields_matching_scans() {
    let mut d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let mut count = 0usize;
    d.for_each_in_range(0.0, f64::MAX, 0, |summary, mz, int| {
        assert!(summary.rt.is_finite());
        assert!(!mz.is_empty());
        assert_eq!(mz.len(), int.len());
        count += 1;
    });
    assert_eq!(count, d.spectrum_count() as usize);
}

#[test]
fn for_each_scan_filters_by_ms_level() {
    let mut d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let mut count = 0usize;
    d.for_each_in_range(0.0, f64::MAX, 1, |_, _, _| {
        count += 1;
    });
    let expected = (0..d.spectrum_count() as usize)
        .filter(|&i| d.spectrum_summary(i).is_some_and(|r| r.ms_level == 1))
        .count();
    assert_eq!(count, expected);
}

fn reencoded_fixture() -> Vec<u8> {
    use crate::ion::encoder::{encode::WriteOptions, ion_writer::write_mzml_to_ion};
    let mzml = {
        let mut reader = IonReader::open(BYTES, ReadOptions::default()).unwrap();
        reader.to_mzml().unwrap()
    };
    let mut out = Vec::new();
    write_mzml_to_ion(&mzml, WriteOptions::default(), &mut out).unwrap();
    out
}

#[test]
fn scans_in_visits_every_scan_and_matches_read_window() {
    let bytes = reencoded_fixture();
    let mut reader = IonReader::open(&bytes, ReadOptions::default()).unwrap();
    let mz = Range {
        from: 0.0,
        to: f64::MAX,
    };
    let mut seen: Vec<(usize, Vec<f64>, Vec<f64>)> = Vec::new();
    reader
        .scans_in(mz, Select::All, None, &mut |window| {
            seen.push((window.index, window.mz.to_vec(), window.intensity.to_vec()));
        })
        .unwrap();
    assert_eq!(seen.len(), reader.spectrum_count() as usize);
    for (index, mz_values, intensity_values) in &seen {
        let direct = reader.read_window(*index, mz).unwrap();
        assert_eq!(*mz_values, direct.x.to_f64());
        assert_eq!(*intensity_values, direct.y.to_f64());
    }
}

#[test]
fn scans_in_ms_level_filter_selects_only_matching_scans() {
    let bytes = reencoded_fixture();
    let mut reader = IonReader::open(&bytes, ReadOptions::default()).unwrap();
    let mz = Range {
        from: 0.0,
        to: f64::MAX,
    };
    let mut count = 0usize;
    reader
        .scans_in(mz, Select::All, Some(1), &mut |_window| {
            count += 1;
        })
        .unwrap();
    let expected = (0..reader.spectrum_count() as usize)
        .filter(|&index| {
            reader
                .spectrum_summary(index)
                .is_some_and(|s| s.ms_level == 1)
        })
        .count();
    assert_eq!(count, expected);
}

#[test]
fn spec_array_addresses_store_cv_code_in_32_byte_records() {
    let bytes = reencoded_fixture();
    let reader = IonReader::open(&bytes, ReadOptions::default()).unwrap();
    let table = &reader.spec_array_addresses;
    assert!(
        !table.is_empty(),
        "fixture must have spectrum array addresses"
    );
    assert_eq!(ARRAY_ADDRESS_BYTES, 32, "A3 record is 32 bytes");
    assert_eq!(
        table.len() % ARRAY_ADDRESS_BYTES,
        0,
        "A3 length is whole records"
    );
    for record in table.chunks_exact(ARRAY_ADDRESS_BYTES) {
        assert_eq!(
            record[31],
            crate::ion::attr_meta::CV_CODE_MS,
            "m/z and intensity arrays use the MS vocabulary"
        );
    }
}

#[test]
fn to_mzml_produces_valid_structure() {
    let mut d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let mzml = d.to_mzml().unwrap();
    let sl = mzml.run.spectrum_list.as_ref().unwrap();
    assert!(!sl.spectra.is_empty());
    assert!(
        sl.spectra[0]
            .binary_data_array_list
            .as_ref()
            .unwrap()
            .binary_data_arrays
            .iter()
            .any(|b| b.binary.is_some())
    );
}

#[test]
fn global_metadata_returns_entries() {
    let d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    assert!(!d.global_metadata().unwrap().is_empty());
}

#[test]
fn spectrum_metadata_returns_entries() {
    let d = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    assert!(!d.spectrum_metadata().unwrap().is_empty());
}

#[test]
fn custom_config_opens_successfully() {
    let config = ReadOptions {
        max_cached_bytes: 1024 * 1024,
        verify_checksums: true,
        parallel: true,
        decompression_limit: DecompressionLimit::default(),
    };
    let d = IonReader::open(BYTES, config).unwrap();
    assert!(d.spectrum_count() > 0);
}

#[test]
fn decode_into_f64_roundtrips() {
    let vals = [1.5f64, 2.5, 3.5];
    let raw: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut buf = Vec::new();
    decode_into(&mut buf, &raw, 1, 0).unwrap();
    assert_eq!(buf, vals);
}

#[test]
fn decode_into_f32_converts() {
    let vals = [1.0f32, 2.0];
    let raw: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut buf = Vec::new();
    decode_into(&mut buf, &raw, 2, 0).unwrap();
    assert!((buf[0] - 1.0).abs() < f64::EPSILON);
    assert!((buf[1] - 2.0).abs() < f64::EPSILON);
}

#[test]
fn dtype_stride_maps_all_types() {
    assert_eq!(dtype_stride(1), 8);
    assert_eq!(dtype_stride(2), 4);
    assert_eq!(dtype_stride(3), 2);
    assert_eq!(dtype_stride(4), 2);
    assert_eq!(dtype_stride(5), 4);
    assert_eq!(dtype_stride(6), 8);
}

#[test]
fn open_bytes_gives_same_result_as_open() {
    let bytes_arc: Arc<[u8]> = Arc::from(BYTES);
    let mut d1 = IonReader::open(BYTES, ReadOptions::default()).unwrap();
    let mut d2 = IonReader::open_source(
        Arc::new(BytesSource::new(bytes_arc)),
        ReadOptions::default(),
    )
    .unwrap();
    assert_eq!(d1.spectrum_count(), d2.spectrum_count());
    let mzml1 = d1.to_mzml().unwrap();
    let mzml2 = d2.to_mzml().unwrap();
    assert_eq!(format!("{mzml1:?}"), format!("{mzml2:?}"));
}

#[test]
fn open_source_uses_provided_source() {
    use crate::ion::decoder::utilities::byte_source::{BytesSource, ReadBytes};
    let bytes_arc: Arc<[u8]> = Arc::from(BYTES);
    let source = Arc::new(BytesSource::new(bytes_arc.clone())) as Arc<dyn ReadBytes>;
    let mut d = IonReader::open_source(source, ReadOptions::default()).unwrap();
    assert!(d.spectrum_count() > 0);
    let mzml = d.to_mzml().unwrap();
    assert!(!mzml.run.spectrum_list.unwrap().spectra.is_empty());
}

#[test]
fn mixed_normal_and_oversized_spectra_preserve_order_and_data() {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{
            BinaryDataArray, BinaryDataArrayList, CvParam, MzML, NumericArray, Run, Spectrum,
            SpectrumList,
        },
    };

    fn make_bda(accession: &str, name: &str, data: Vec<f64>) -> BinaryDataArray {
        BinaryDataArray {
            cv_params: vec![CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(accession.to_string()),
                name: name.to_string(),
                value: None,
                unit_cv_ref: None,
                unit_name: None,
                unit_accession: None,
            }],
            binary: Some(NumericArray::F64(data)),
            ..Default::default()
        }
    }

    fn make_spectrum(id: &str, mz: Vec<f64>, int: Vec<f64>) -> Spectrum {
        Spectrum {
            id: id.to_string(),
            binary_data_array_list: Some(BinaryDataArrayList {
                count: Some(2),
                binary_data_arrays: vec![
                    make_bda("MS:1000514", "m/z array", mz),
                    make_bda("MS:1000515", "intensity array", int),
                ],
            }),
            ..Default::default()
        }
    }

    let small_count_before = 5;
    let small_count_after = 5;
    let huge_n = (TARGET_BLOCK_UNCOMPRESSED_BYTES / 8) * 2;

    let mut expected_spectra: Vec<(String, Vec<f64>, Vec<f64>)> = Vec::new();
    let mut spectra = Vec::new();

    for i in 0..small_count_before {
        let mz: Vec<f64> = (0..10).map(|j| 100.0 + i as f64 + j as f64 * 0.1).collect();
        let int: Vec<f64> = (0..10).map(|j| (i * 10 + j) as f64).collect();
        let id = format!("small_pre_{i}");
        spectra.push(make_spectrum(&id, mz.clone(), int.clone()));
        expected_spectra.push((id, mz, int));
    }

    let huge_mz: Vec<f64> = (0..huge_n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let huge_int: Vec<f64> = (0..huge_n).map(|i| i as f64 * 10.0).collect();
    let huge_id = "huge_ms1".to_string();
    spectra.push(make_spectrum(&huge_id, huge_mz.clone(), huge_int.clone()));
    expected_spectra.push((huge_id, huge_mz, huge_int));

    for i in 0..small_count_after {
        let mz: Vec<f64> = (0..10).map(|j| 500.0 + i as f64 + j as f64 * 0.1).collect();
        let int: Vec<f64> = (0..10).map(|j| (i * 20 + j) as f64).collect();
        let id = format!("small_post_{i}");
        spectra.push(make_spectrum(&id, mz.clone(), int.clone()));
        expected_spectra.push((id, mz, int));
    }

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 3,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: true,
            section_storage: SectionStorage::Memory,
            mz_window: 0.0,
        },
        &mut encoded,
    )
    .unwrap();

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let mzml_out = decoder.to_mzml().unwrap();
    let out_spectra = mzml_out.run.spectrum_list.unwrap().spectra;

    assert_eq!(out_spectra.len(), expected_spectra.len());

    for (out, (expected_id, expected_mz, expected_int)) in
        out_spectra.iter().zip(expected_spectra.iter())
    {
        assert_eq!(&out.id, expected_id);
        let arrays = out
            .binary_data_array_list
            .as_ref()
            .unwrap()
            .binary_data_arrays
            .as_slice();
        let mz_out = arrays
            .iter()
            .find(|a| {
                a.cv_params
                    .iter()
                    .any(|cv| cv.accession.as_deref() == Some("MS:1000514"))
            })
            .and_then(|a| a.binary.as_ref())
            .unwrap();
        let int_out = arrays
            .iter()
            .find(|a| {
                a.cv_params
                    .iter()
                    .any(|cv| cv.accession.as_deref() == Some("MS:1000515"))
            })
            .and_then(|a| a.binary.as_ref())
            .unwrap();
        let NumericArray::F64(mz_vec) = mz_out else {
            panic!("expected F64 mz array for {expected_id}");
        };
        let NumericArray::F64(int_vec) = int_out else {
            panic!("expected F64 intensity array for {expected_id}");
        };
        assert_eq!(mz_vec, expected_mz, "mz mismatch for {expected_id}");
        assert_eq!(
            int_vec, expected_int,
            "intensity mismatch for {expected_id}"
        );
    }
}

#[test]
fn oversized_array_roundtrips_with_compression_and_parallel() {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{
            BinaryDataArray, BinaryDataArrayList, CvParam, MzML, NumericArray, Run, Spectrum,
            SpectrumList,
        },
    };

    let n = (TARGET_BLOCK_UNCOMPRESSED_BYTES / 8) * 2;
    let mz_data: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int_data: Vec<f64> = (0..n).map(|i| i as f64 * 10.0).collect();

    fn make_bda(accession: &str, name: &str, data: Vec<f64>) -> BinaryDataArray {
        BinaryDataArray {
            cv_params: vec![CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(accession.to_string()),
                name: name.to_string(),
                value: None,
                unit_cv_ref: None,
                unit_name: None,
                unit_accession: None,
            }],
            binary: Some(NumericArray::F64(data)),
            ..Default::default()
        }
    }

    let spectrum = Spectrum {
        id: "scan=1".to_string(),
        binary_data_array_list: Some(BinaryDataArrayList {
            count: Some(2),
            binary_data_arrays: vec![
                make_bda("MS:1000514", "m/z array", mz_data.clone()),
                make_bda("MS:1000515", "intensity array", int_data.clone()),
            ],
        }),
        ..Default::default()
    };

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra: vec![spectrum],
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 3,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: true,
            section_storage: SectionStorage::Memory,
            mz_window: 0.0,
        },
        &mut encoded,
    )
    .unwrap();

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let mzml_out = decoder.to_mzml().unwrap();

    let spectra = mzml_out.run.spectrum_list.unwrap().spectra;
    let arrays = spectra[0]
        .binary_data_array_list
        .as_ref()
        .unwrap()
        .binary_data_arrays
        .as_slice();

    let mz_out = arrays
        .iter()
        .find(|a| {
            a.cv_params
                .iter()
                .any(|cv| cv.accession.as_deref() == Some("MS:1000514"))
        })
        .and_then(|a| a.binary.as_ref())
        .unwrap();
    let int_out = arrays
        .iter()
        .find(|a| {
            a.cv_params
                .iter()
                .any(|cv| cv.accession.as_deref() == Some("MS:1000515"))
        })
        .and_then(|a| a.binary.as_ref())
        .unwrap();

    let NumericArray::F64(mz_vec) = mz_out else {
        panic!("expected F64 mz array");
    };
    let NumericArray::F64(int_vec) = int_out else {
        panic!("expected F64 intensity array");
    };

    assert_eq!(mz_vec.len(), n);
    assert_eq!(int_vec.len(), n);
    assert_eq!(mz_vec, &mz_data);
    assert_eq!(int_vec, &int_data);
}

#[test]
fn oversized_array_roundtrips_through_encode_decode() {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{
            BinaryDataArray, BinaryDataArrayList, CvParam, MzML, NumericArray, Run, Spectrum,
            SpectrumList,
        },
    };

    let n = (TARGET_BLOCK_UNCOMPRESSED_BYTES / 8) * 2;
    let mz_data: Vec<f64> = (0..n).map(|i| i as f64 * 0.001).collect();
    let int_data: Vec<f64> = (0..n).map(|i| i as f64 * 10.0).collect();

    fn make_bda(accession: &str, name: &str, data: Vec<f64>) -> BinaryDataArray {
        BinaryDataArray {
            cv_params: vec![CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(accession.to_string()),
                name: name.to_string(),
                value: None,
                unit_cv_ref: None,
                unit_name: None,
                unit_accession: None,
            }],
            binary: Some(NumericArray::F64(data)),
            ..Default::default()
        }
    }

    let spectrum = Spectrum {
        id: "scan=1".to_string(),
        binary_data_array_list: Some(BinaryDataArrayList {
            count: Some(2),
            binary_data_arrays: vec![
                make_bda("MS:1000514", "m/z array", mz_data.clone()),
                make_bda("MS:1000515", "intensity array", int_data.clone()),
            ],
        }),
        ..Default::default()
    };

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra: vec![spectrum],
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 0,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: false,
            section_storage: SectionStorage::Memory,
            mz_window: 0.0,
        },
        &mut encoded,
    )
    .unwrap();

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let mzml_out = decoder.to_mzml().unwrap();

    let spectra = mzml_out.run.spectrum_list.unwrap().spectra;
    let arrays = spectra[0]
        .binary_data_array_list
        .as_ref()
        .unwrap()
        .binary_data_arrays
        .as_slice();

    let mz_out = arrays
        .iter()
        .find(|a| {
            a.cv_params
                .iter()
                .any(|cv| cv.accession.as_deref() == Some("MS:1000514"))
        })
        .and_then(|a| a.binary.as_ref())
        .unwrap();
    let int_out = arrays
        .iter()
        .find(|a| {
            a.cv_params
                .iter()
                .any(|cv| cv.accession.as_deref() == Some("MS:1000515"))
        })
        .and_then(|a| a.binary.as_ref())
        .unwrap();

    let NumericArray::F64(mz_vec) = mz_out else {
        panic!("expected F64 mz array");
    };
    let NumericArray::F64(int_vec) = int_out else {
        panic!("expected F64 intensity array");
    };

    assert_eq!(mz_vec.len(), n);
    assert_eq!(int_vec.len(), n);
    assert_eq!(mz_vec, &mz_data);
    assert_eq!(int_vec, &int_data);
}

fn make_split_bda(accession: &str, name: &str, data: Vec<f64>) -> BinaryDataArray {
    BinaryDataArray {
        cv_params: vec![CvParam {
            cv_ref: Some("MS".to_string()),
            accession: Some(accession.to_string()),
            name: name.to_string(),
            value: None,
            unit_cv_ref: None,
            unit_name: None,
            unit_accession: None,
        }],
        binary: Some(NumericArray::F64(data)),
        ..Default::default()
    }
}

fn encode_one_spectrum_windowed(mz: Vec<f64>, int: Vec<f64>, mz_window: f64) -> Vec<u8> {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{BinaryDataArrayList, MzML, Run, Spectrum, SpectrumList},
    };

    let spectrum = Spectrum {
        id: "split_ms1".to_string(),
        binary_data_array_list: Some(BinaryDataArrayList {
            count: Some(2),
            binary_data_arrays: vec![
                make_split_bda("MS:1000514", "m/z array", mz),
                make_split_bda("MS:1000515", "intensity array", int),
            ],
        }),
        ..Default::default()
    };

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra: vec![spectrum],
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 3,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: true,
            section_storage: SectionStorage::Memory,
            mz_window,
        },
        &mut encoded,
    )
    .unwrap();
    encoded
}

fn encode_one_chromatogram_windowed(time: Vec<f64>, int: Vec<f64>, time_window: f64) -> Vec<u8> {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{BinaryDataArrayList, Chromatogram, ChromatogramList, MzML, Run},
    };

    let chromatogram = Chromatogram {
        id: "split_chrom".to_string(),
        binary_data_array_list: Some(BinaryDataArrayList {
            count: Some(2),
            binary_data_arrays: vec![
                make_split_bda("MS:1000595", "time array", time),
                make_split_bda("MS:1000515", "intensity array", int),
            ],
        }),
        ..Default::default()
    };

    let mzml_in = MzML {
        run: Run {
            chromatogram_list: Some(ChromatogramList {
                count: Some(1),
                default_data_processing_ref: None,
                chromatograms: vec![chromatogram],
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 3,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: true,
            section_storage: SectionStorage::Memory,
            mz_window: time_window,
        },
        &mut encoded,
    )
    .unwrap();
    encoded
}

#[test]
fn to_mzml_keeps_all_spectra_across_metadata_group_boundaries() {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{BinaryDataArrayList, MzML, Run, Spectrum, SpectrumList},
    };

    let spectrum_count = 8192 + 5;
    let spectra: Vec<Spectrum> = (0..spectrum_count)
        .map(|i| Spectrum {
            id: format!("scan={i}"),
            binary_data_array_list: Some(BinaryDataArrayList {
                count: Some(2),
                binary_data_arrays: vec![
                    make_split_bda("MS:1000514", "m/z array", vec![100.0, 100.5]),
                    make_split_bda("MS:1000515", "intensity array", vec![10.0, 20.0]),
                ],
            }),
            ..Default::default()
        })
        .collect();

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 3,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            ..Default::default()
        },
        &mut encoded,
    )
    .unwrap();

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    assert_eq!(decoder.header.spectrum_count, spectrum_count as u64);

    let out = decoder.to_mzml().unwrap();
    let spectra_out = out.run.spectrum_list.unwrap().spectra;
    assert_eq!(
        spectra_out.len(),
        spectrum_count,
        "to_mzml must not drop spectra past the first metadata group"
    );
    assert_eq!(spectra_out[0].id, "scan=0");
    assert_eq!(
        spectra_out[spectrum_count - 1].id,
        format!("scan={}", spectrum_count - 1)
    );
}

#[test]
fn to_mzml_keeps_all_chromatograms_across_metadata_group_boundaries() {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{BinaryDataArrayList, Chromatogram, ChromatogramList, MzML, Run},
    };

    let chromatogram_count = 8192 + 5;
    let chromatograms: Vec<Chromatogram> = (0..chromatogram_count)
        .map(|i| Chromatogram {
            id: format!("chrom={i}"),
            binary_data_array_list: Some(BinaryDataArrayList {
                count: Some(2),
                binary_data_arrays: vec![
                    make_split_bda("MS:1000595", "time array", vec![0.0, 1.0]),
                    make_split_bda("MS:1000515", "intensity array", vec![10.0, 20.0]),
                ],
            }),
            ..Default::default()
        })
        .collect();

    let mzml_in = MzML {
        run: Run {
            chromatogram_list: Some(ChromatogramList {
                count: Some(chromatogram_count),
                default_data_processing_ref: None,
                chromatograms,
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 3,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            ..Default::default()
        },
        &mut encoded,
    )
    .unwrap();

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    assert_eq!(decoder.header.chrom_count, chromatogram_count as u64);

    let out = decoder.to_mzml().unwrap();
    let chroms_out = out.run.chromatogram_list.unwrap().chromatograms;
    assert_eq!(
        chroms_out.len(),
        chromatogram_count,
        "to_mzml must not drop chromatograms past the first metadata group"
    );
    assert_eq!(chroms_out[0].id, "chrom=0");
    assert_eq!(
        chroms_out[chromatogram_count - 1].id,
        format!("chrom={}", chromatogram_count - 1)
    );
}

#[test]
fn split_mz_array_roundtrips_through_to_mzml() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| (i % 1000) as f64).collect();

    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    assert!(
        decoder.header.spec_block_count >= 4,
        "splitting must produce several window blocks, got {}",
        decoder.header.spec_block_count
    );

    let mzml_out = decoder.to_mzml().unwrap();
    let spectra = mzml_out.run.spectrum_list.unwrap().spectra;
    assert_eq!(spectra.len(), 1);

    let arrays = &spectra[0]
        .binary_data_array_list
        .as_ref()
        .unwrap()
        .binary_data_arrays;
    let mz_arrays: Vec<_> = arrays
        .iter()
        .filter(|a| {
            a.cv_params
                .iter()
                .any(|cv| cv.accession.as_deref() == Some("MS:1000514"))
        })
        .collect();
    assert_eq!(
        mz_arrays.len(),
        1,
        "split windows must reconstruct one logical m/z array"
    );

    let NumericArray::F64(mz_out) = mz_arrays[0].binary.as_ref().unwrap() else {
        panic!("expected F64 mz");
    };
    assert_eq!(mz_out, &mz);
}

#[test]
fn split_mz_array_roundtrips_through_get_spectrum() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| (i % 777) as f64).collect();

    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let spectrum = decoder.spectrum(0).unwrap().unwrap();
    let arrays = &spectrum
        .binary_data_array_list
        .as_ref()
        .unwrap()
        .binary_data_arrays;

    let mz_arrays: Vec<_> = arrays
        .iter()
        .filter(|a| {
            a.cv_params
                .iter()
                .any(|cv| cv.accession.as_deref() == Some("MS:1000514"))
        })
        .collect();
    assert_eq!(mz_arrays.len(), 1);
    let NumericArray::F64(mz_out) = mz_arrays[0].binary.as_ref().unwrap() else {
        panic!("expected F64 mz");
    };
    assert_eq!(mz_out, &mz);

    let int_arrays: Vec<_> = arrays
        .iter()
        .filter(|a| {
            a.cv_params
                .iter()
                .any(|cv| cv.accession.as_deref() == Some("MS:1000515"))
        })
        .collect();
    assert_eq!(int_arrays.len(), 1);
    let NumericArray::F64(int_out) = int_arrays[0].binary.as_ref().unwrap() else {
        panic!("expected F64 intensity");
    };
    assert_eq!(int_out, &int);
}

#[test]
fn read_spectrum_logical_array_joins_split_segments() {
    let n = 40_000;
    let mz: Vec<f64> = (0..n).map(|i| 200.0 + i as f64 * 0.002).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();

    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let mz_out = decoder
        .get_spectrum_array(0, crate::accessions::MZ_ARRAY)
        .unwrap();
    assert_eq!(mz_out, mz);
}

#[test]
fn centroided_small_arrays_are_encoded_correctly() {
    let mz: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();
    let int: Vec<f64> = (0..10).map(|i| i as f64).collect();

    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    IonReader::open(&encoded, ReadOptions::default()).unwrap();
}

#[test]
fn split_mz_array_roundtrips_with_disk_staged_bounds() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| (i % 1000) as f64).collect();

    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let mz_out = decoder
        .get_spectrum_array(0, crate::accessions::MZ_ARRAY)
        .unwrap();
    assert_eq!(mz_out, mz);
}

#[test]
fn read_window_handles_fractional_mz_window() {
    let mz: Vec<f64> = (0..2000).map(|i| 100.0 + i as f64 * 0.01).collect();
    let int: Vec<f64> = (0..2000).map(|i| (i * 3) as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 2.5);
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    for (low, high) in [(103.0, 107.0), (100.0, 120.0), (115.5, 116.5)] {
        let got = decoder
            .read_window(
                0,
                Range {
                    from: low,
                    to: high,
                },
            )
            .unwrap();
        let (expected_mz, expected_int) = brute_force_window(&mz, &int, low, high);
        assert_eq!(got.x.to_f64(), expected_mz, "mz for [{low}, {high}]");
        assert_eq!(got.y.to_f64(), expected_int, "int for [{low}, {high}]");
    }
}

fn brute_force_window(mz: &[f64], int: &[f64], low: f64, high: f64) -> (Vec<f64>, Vec<f64>) {
    let mut mz_out = Vec::new();
    let mut int_out = Vec::new();
    for (index, &value) in mz.iter().enumerate() {
        if value >= low && value <= high {
            mz_out.push(value);
            int_out.push(int[index]);
        }
    }
    (mz_out, int_out)
}

#[test]
fn window_fast_path_matches_brute_force() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let windows = [
        (120.0, 130.0),
        (100.0, 149.999),
        (100.0001, 100.0009),
        (130.5, 130.5),
        (200.0, 300.0),
        (0.0, 50.0),
    ];
    for (low, high) in windows {
        let got = decoder
            .read_window(
                0,
                Range {
                    from: low,
                    to: high,
                },
            )
            .unwrap();
        let (expected_mz, expected_int) = brute_force_window(&mz, &int, low, high);
        assert_eq!(
            got.x.to_f64(),
            expected_mz,
            "mz mismatch for window {low}..{high}"
        );
        assert_eq!(
            got.y.to_f64(),
            expected_int,
            "intensity mismatch for window {low}..{high}"
        );
    }

    assert!(
        matches!(
            decoder.spec_window_directory,
            WindowDirectoryCache::Loaded(_)
        ),
        "fast path should have loaded A0"
    );
}

#[test]
fn window_errors_when_bounds_missing() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    decoder.spec_window_directory = WindowDirectoryCache::Missing;

    assert_eq!(
        decoder.read_window(
            0,
            Range {
                from: 120.0,
                to: 130.0
            }
        ),
        Err(IonError::MissingSpectrumBounds)
    );
}

#[test]
fn window_on_unsplit_array_uses_fallback_and_is_correct() {
    let mz: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();
    let int: Vec<f64> = (0..10).map(|i| (i * 7) as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 0.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let got = decoder
        .read_window(
            0,
            Range {
                from: 102.0,
                to: 105.0,
            },
        )
        .unwrap();
    let (expected_mz, expected_int) = brute_force_window(&mz, &int, 102.0, 105.0);
    assert_eq!(got.x.to_f64(), expected_mz);
    assert_eq!(got.y.to_f64(), expected_int);
}

#[test]
fn window_out_of_range_index_errors() {
    let mz: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();
    let int: Vec<f64> = (0..10).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    assert!(
        decoder
            .read_window(
                5,
                Range {
                    from: 100.0,
                    to: 200.0
                }
            )
            .is_err()
    );
}

#[test]
fn reader_reads_mz_range() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 10.0);

    let bytes_arc: Arc<[u8]> = Arc::from(encoded.as_slice());
    let mut reader = IonReader::open_source(
        Arc::new(BytesSource::new(bytes_arc)),
        ReadOptions::default(),
    )
    .unwrap();
    let got = reader
        .read_window(
            0,
            Range {
                from: 120.0,
                to: 130.0,
            },
        )
        .unwrap();
    let (expected_mz, expected_int) = brute_force_window(&mz, &int, 120.0, 130.0);
    assert_eq!(got.x.to_f64(), expected_mz);
    assert_eq!(got.y.to_f64(), expected_int);
}

#[test]
fn a1_window_directory_pairs_windows_with_mz_and_intensity_refs() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let _ = decoder
        .read_window(
            0,
            Range {
                from: 120.0,
                to: 130.0,
            },
        )
        .unwrap();
    let WindowDirectoryCache::Loaded(index) = &decoder.spec_window_directory else {
        panic!("A0 should be loaded");
    };

    assert!(
        index.window_count() >= 2,
        "expected the spectrum to be split into windows"
    );

    let array_type_of = |ref_index: u32| -> u32 {
        let base = ref_index as usize * ARRAY_ADDRESS_BYTES;
        u32::from_le_bytes(
            decoder.spec_array_addresses[base + 20..base + 24]
                .try_into()
                .unwrap(),
        )
    };

    let mut found = 0;
    for window in 0..index.window_count() {
        let Some(row) = index.find_in_window(window, 0) else {
            continue;
        };
        assert_eq!(array_type_of(row.mz_address), crate::accessions::MZ_ARRAY);
        assert_eq!(
            array_type_of(row.intensity_address),
            crate::accessions::INTENSITY_ARRAY
        );
        found += 1;
    }
    assert!(found >= 2, "expected refs for several windows");
}

#[test]
fn generic_window_matches_read_mz_range() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let generic = decoder
        .read_spectrum_window(0, crate::accessions::MZ_ARRAY, ACC_INT, 120.0, 130.0)
        .unwrap();

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let mz_range = decoder
        .read_window(
            0,
            Range {
                from: 120.0,
                to: 130.0,
            },
        )
        .unwrap();

    assert_eq!(generic.x, mz_range.x);
    assert_eq!(generic.y, mz_range.y);
}

#[test]
fn generic_window_missing_accession_returns_empty() {
    let mz: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();
    let int: Vec<f64> = (0..10).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let got = decoder
        .read_spectrum_window(0, 99_999_999u32, ACC_INT, 1.0, 2.0)
        .unwrap();

    assert!(got.x.is_empty());
    assert!(got.y.is_empty());
}

fn spec_directory_range(header: &Header) -> (usize, usize) {
    let entry_size =
        crate::ion::encoder::utilities::block_writer::BLOCK_DIRECTORY_ENTRY_SIZE as u64;
    let directory_size = header.spec_block_count * entry_size;
    let end = header.off_spec_container + header.len_spec_container;
    let start = end - directory_size;
    (start as usize, end as usize)
}

#[test]
fn directory_crc_roundtrips() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let header = parse_header(&encoded[..1024]).unwrap();
    let (start, end) = spec_directory_range(&header);
    let computed = crc32fast::hash(&encoded[start..end]);

    assert_eq!(computed, header.spec_directory_crc32);
    assert!(IonReader::open(&encoded, ReadOptions::default()).is_ok());
}

#[test]
fn flipped_directory_offset_is_caught_before_any_read() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let mut encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let header = parse_header(&encoded[..1024]).unwrap();
    let (start, _end) = spec_directory_range(&header);
    encoded[start] ^= 0xFF;

    let result = IonReader::open(&encoded, ReadOptions::default());
    assert!(result.is_err());
    let message = format!("{}", result.err().unwrap());
    assert!(message.contains("directory checksum mismatch"));
}

#[test]
fn verify_off_skips_directory_check() {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let mut encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let header = parse_header(&encoded[..1024]).unwrap();
    let (start, _end) = spec_directory_range(&header);
    encoded[start] ^= 0xFF;

    let config = ReadOptions {
        verify_checksums: false,
        ..ReadOptions::default()
    };
    assert!(IonReader::open(&encoded, config).is_ok());
}

#[test]
fn empty_container_directory_crc_is_consistent() {
    let mz: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();
    let int: Vec<f64> = (0..10).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let header = parse_header(&encoded[..1024]).unwrap();
    assert_eq!(header.chrom_block_count, 0);
    assert_eq!(header.chrom_directory_crc32, crc32fast::hash(&[]));
    assert!(IonReader::open(&encoded, ReadOptions::default()).is_ok());
}

#[test]
fn a1_window_directory_section_is_written() {
    let mz: Vec<f64> = (0..1000).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..1000).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let header = parse_header(&encoded[..1024]).unwrap();

    assert!(
        header.len_spec_window_directory > 0,
        "A0 window directory should be written"
    );
}

#[test]
fn candidate_items_filters_by_axis_accession() {
    use crate::accessions::INTENSITY_ARRAY;

    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let mz_candidates = decoder
        .candidate_items(
            ItemKind::Spectrum,
            crate::accessions::MZ_ARRAY,
            120.0,
            130.0,
        )
        .unwrap();
    let int_candidates = decoder
        .candidate_items(ItemKind::Spectrum, INTENSITY_ARRAY, 1000.0, 2000.0)
        .unwrap();

    assert!(
        !mz_candidates.is_empty(),
        "m/z candidates should be found with bounds"
    );
    assert!(
        int_candidates.is_empty(),
        "intensity is not an axis, so no candidates"
    );
}

#[test]
fn candidate_items_returns_empty_when_window_directory_missing() {
    let n = 1000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    decoder.spec_window_directory = WindowDirectoryCache::Missing;

    let candidates = decoder
        .candidate_items(
            ItemKind::Spectrum,
            crate::accessions::MZ_ARRAY,
            120.0,
            130.0,
        )
        .unwrap();

    assert!(
        candidates.is_empty(),
        "no fallback: a missing window directory yields no candidates"
    );
}

#[test]
fn candidate_items_empty_on_a1_crc_failure() {
    let n = 5000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let mut encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let a1_offset = {
        let header = parse_header(&encoded[..1024]).unwrap();
        header.off_spec_window_directory as usize
    };

    if a1_offset > 0 {
        encoded[a1_offset] ^= 0xFF;

        let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
        let candidates = decoder
            .candidate_items(
                ItemKind::Spectrum,
                crate::accessions::MZ_ARRAY,
                120.0,
                130.0,
            )
            .unwrap();

        assert!(
            candidates.is_empty(),
            "no fallback: a corrupt window directory yields no candidates"
        );
        assert!(
            matches!(
                decoder.spec_window_directory,
                WindowDirectoryCache::BadChecksum
            ),
            "A0 should be marked bad checksum after CRC failure"
        );
    }
}

#[test]
fn ensure_chrom_window_directory_distinguishes_missing_from_bad_checksum() {
    let n = 5000;
    let time: Vec<f64> = (0..n).map(|i| i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let mut encoded = encode_one_chromatogram_windowed(time, int, 10.0);

    let b1_offset = {
        let header = parse_header(&encoded[..1024]).unwrap();
        header.off_chrom_window_directory as usize
    };
    assert!(b1_offset > 0, "chromatogram window directory should exist");

    encoded[b1_offset] ^= 0xFF;

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    decoder.ensure_chrom_window_directory();

    assert!(
        matches!(
            decoder.chrom_window_directory,
            WindowDirectoryCache::BadChecksum
        ),
        "B0 should be marked bad checksum after CRC failure"
    );

    let spectrum_only = encode_one_spectrum_windowed(vec![100.0, 200.0], vec![1.0, 2.0], 10.0);
    let mut missing_decoder = IonReader::open(&spectrum_only, ReadOptions::default()).unwrap();
    missing_decoder.ensure_chrom_window_directory();

    assert!(
        matches!(
            missing_decoder.chrom_window_directory,
            WindowDirectoryCache::Missing
        ),
        "a file with no chromatogram window directory must stay Missing, not BadChecksum"
    );
}

#[test]
fn b1_header_fields_are_populated() {
    let n = 5000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let header = parse_header(&encoded[..1024]).unwrap();

    assert_eq!(
        header.off_chrom_window_directory, 0,
        "B0 offset should be 0 (no chromatograms)"
    );
    assert_eq!(
        header.len_chrom_window_directory, 0,
        "B0 length should be 0 (no chromatograms)"
    );
    assert_eq!(
        header.plain_len_chrom_window_directory, 0,
        "B0 plain_len should be 0 (no chromatograms)"
    );
}

#[test]
fn candidate_items_for_chrom_axis_without_bounds() {
    let n = 1000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz, int, 10.0);

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let candidates = decoder
        .candidate_items(
            ItemKind::Chromatogram,
            crate::accessions::TIME_ARRAY,
            0.0,
            1000.0,
        )
        .unwrap();

    assert!(
        candidates.is_empty(),
        "chromatogram query should return empty when file has no chromatograms"
    );
}

fn split_file_with_a1() -> (Vec<f64>, Vec<f64>, Vec<u8>) {
    let n = 50_000;
    let mz: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.001).collect();
    let int: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let encoded = encode_one_spectrum_windowed(mz.clone(), int.clone(), 10.0);
    (mz, int, encoded)
}

fn kept_block_ids(
    decoder: &IonReader,
    scan_index: usize,
    low: f64,
    high: f64,
) -> (Vec<u32>, Vec<u32>) {
    let WindowDirectoryCache::Loaded(index) = &decoder.spec_window_directory else {
        panic!("A0 must be loaded before computing kept blocks");
    };
    let width = decoder.header.target_mz_window as f64;
    let window = |mz: f64| -> usize {
        if width > 0.0 && mz > 0.0 {
            (mz / width).floor() as usize
        } else {
            0
        }
    };
    let block_of = |ref_index: u32| -> u32 {
        let base = ref_index as usize * ARRAY_ADDRESS_BYTES;
        u32::from_le_bytes(
            decoder.spec_array_addresses[base + 16..base + 20]
                .try_into()
                .unwrap(),
        )
    };

    let window_count = index.window_count();
    let mut mz_blocks = Vec::new();
    let mut intensity_blocks = Vec::new();
    if window_count > 0 && window(low) < window_count {
        let window_high = window(high).min(window_count - 1);
        for current in window(low)..=window_high {
            if let Some(row) = index.find_in_window(current, scan_index as u32) {
                mz_blocks.push(block_of(row.mz_address));
                intensity_blocks.push(block_of(row.intensity_address));
            }
        }
    }
    mz_blocks.sort_unstable();
    mz_blocks.dedup();
    intensity_blocks.sort_unstable();
    intensity_blocks.dedup();
    (mz_blocks, intensity_blocks)
}

fn block_ranges_for(decoder: &IonReader, block_ids: &[u32]) -> Vec<ByteRange> {
    let mut ranges: Vec<ByteRange> = block_ids
        .iter()
        .map(|&id| decoder.spec_container.block_byte_range(id).unwrap())
        .collect();
    ranges.sort_unstable_by_key(|range| (range.offset, range.length));
    ranges.dedup();
    ranges
}

#[test]
fn require_bounds_passes_on_file_with_a1() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    assert!(decoder.require_bounds().is_ok());
}

#[test]
fn require_bounds_errors_when_a1_missing() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    decoder.spec_window_directory = WindowDirectoryCache::Missing;
    assert_eq!(
        decoder.require_bounds(),
        Err(IonError::MissingSpectrumBounds)
    );
}

#[test]
fn require_bounds_errors_on_bad_checksum() {
    let (_, _, mut encoded) = split_file_with_a1();
    let a1_offset = parse_header(&encoded[..1024])
        .unwrap()
        .off_spec_window_directory as usize;
    assert!(a1_offset > 0);
    encoded[a1_offset] ^= 0xFF;

    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    assert_eq!(
        decoder.require_bounds(),
        Err(IonError::BadSpectrumBoundsChecksum)
    );
}

#[test]
fn require_bounds_errors_on_malformed_rows() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    decoder.spec_window_directory = WindowDirectoryCache::Malformed("bad rows".to_string());
    assert_eq!(
        decoder.require_bounds(),
        Err(IonError::MalformedSpectrumBounds("bad rows".to_string()))
    );
}

#[test]
fn read_mz_range_matches_brute_force() {
    let (mz, int, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    for (low, high) in [(120.0, 130.0), (100.0, 149.999), (130.5, 130.5)] {
        let got = decoder
            .read_window(
                0,
                Range {
                    from: low,
                    to: high,
                },
            )
            .unwrap();
        let (expected_mz, expected_int) = brute_force_window(&mz, &int, low, high);
        assert_eq!(
            got.x.to_f64(),
            expected_mz,
            "mz mismatch for window {low}..{high}"
        );
        assert_eq!(
            got.y.to_f64(),
            expected_int,
            "intensity mismatch for window {low}..{high}"
        );
    }
}

#[test]
fn read_mz_range_errors_when_a1_missing() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    decoder.spec_window_directory = WindowDirectoryCache::Missing;
    assert_eq!(
        decoder.read_window(
            0,
            Range {
                from: 120.0,
                to: 130.0
            }
        ),
        Err(IonError::MissingSpectrumBounds)
    );
}

#[test]
fn read_mz_range_errors_when_window_directory_malformed() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    decoder.spec_window_directory =
        WindowDirectoryCache::Malformed("bad window directory".to_string());

    let result = decoder.read_window(
        0,
        Range {
            from: 100.0,
            to: 200.0,
        },
    );
    assert!(matches!(result, Err(IonError::MalformedSpectrumBounds(_))));
}

#[test]
fn read_mz_range_errors_on_low_above_high() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    assert!(
        decoder
            .read_window(
                0,
                Range {
                    from: 130.0,
                    to: 120.0
                }
            )
            .is_err()
    );
}

#[test]
fn read_mz_range_errors_on_non_finite_bounds() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    assert!(
        decoder
            .read_window(
                0,
                Range {
                    from: f64::NAN,
                    to: 130.0
                }
            )
            .is_err()
    );
    assert!(
        decoder
            .read_window(
                0,
                Range {
                    from: 120.0,
                    to: f64::INFINITY
                }
            )
            .is_err()
    );
}

#[test]
fn plan_mz_range_returns_mz_and_intensity_blocks() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let plan = decoder
        .byte_ranges(
            0,
            Range {
                from: 120.0,
                to: 130.0,
            },
        )
        .unwrap();

    let (mz_blocks, intensity_blocks) = kept_block_ids(&decoder, 0, 120.0, 130.0);
    assert!(!mz_blocks.is_empty(), "expected kept m/z blocks");
    assert!(
        !intensity_blocks.is_empty(),
        "expected kept intensity blocks"
    );

    for range in block_ranges_for(&decoder, &intensity_blocks) {
        assert!(
            plan.contains(&range),
            "plan must include intensity block range {range:?}"
        );
    }
    for range in block_ranges_for(&decoder, &mz_blocks) {
        assert!(
            plan.contains(&range),
            "plan must include m/z block range {range:?}"
        );
    }
}

#[test]
fn plan_and_read_mz_range_use_same_segments() {
    let (_, _, encoded) = split_file_with_a1();
    let mut decoder = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let plan = decoder
        .byte_ranges(
            0,
            Range {
                from: 120.0,
                to: 130.0,
            },
        )
        .unwrap();

    let (mz_blocks, intensity_blocks) = kept_block_ids(&decoder, 0, 120.0, 130.0);
    let mut all_blocks = mz_blocks;
    all_blocks.extend(intensity_blocks);
    all_blocks.sort_unstable();
    all_blocks.dedup();

    let expected = block_ranges_for(&decoder, &all_blocks);
    assert_eq!(plan, expected);
}

#[test]
fn plan_open_ranges_includes_spec_a1() {
    let (_, _, encoded) = split_file_with_a1();
    let header = parse_header(&encoded[..1024]).unwrap();
    assert!(header.len_spec_window_directory > 0);

    let ranges = open_ranges(&encoded[..1024]).unwrap();
    assert!(ranges.contains(&ByteRange {
        offset: header.off_spec_window_directory,
        length: header.len_spec_window_directory,
    }));
}

#[test]
fn candidate_items_carries_intensity_address() {
    use crate::accessions::INTENSITY_ARRAY;

    let rts: Vec<f64> = (0..20).map(|step| step as f64).collect();
    let encoded = encode_spectra_over_windows(&rts, 100.0);
    let mut reader = open_reader(&encoded);

    let address_at = |reader: &IonReader, index: u64| -> ArrayAddress {
        let start = index as usize * ARRAY_ADDRESS_BYTES;
        parse_array_address(&reader.spec_array_addresses[start..start + ARRAY_ADDRESS_BYTES])
    };

    let slices = reader
        .candidate_items(ItemKind::Spectrum, ACC_MZ, 150.0, 260.0)
        .unwrap();
    assert!(!slices.is_empty());

    for slice in &slices {
        let mz_address = address_at(&reader, slice.array_address_index);
        let intensity_address = address_at(&reader, slice.intensity_address_index);
        assert_eq!(mz_address.array_type, ACC_MZ);
        assert_eq!(intensity_address.array_type, INTENSITY_ARRAY);

        let windows = reader
            .get_spectrum_mz_windows(slice.item_index as usize, 150.0, 260.0)
            .unwrap();
        let paired = windows.iter().any(|window| {
            window.mz_address.block_id == mz_address.block_id
                && window.mz_address.element_offset == mz_address.element_offset
                && window.intensity_address.block_id == intensity_address.block_id
                && window.intensity_address.element_offset == intensity_address.element_offset
        });
        assert!(
            paired,
            "slice {slice:?} does not match any read_window pair"
        );
    }
}

fn make_spectrum_with_rt(id: String, rt: f64, mz: Vec<f64>, int: Vec<f64>) -> Spectrum {
    use crate::mzml::structs::{Scan, ScanList};

    let mut spectrum = make_spectrum_with_arrays(id, mz, int);
    spectrum.scan_list = Some(ScanList {
        count: Some(1),
        scans: vec![Scan {
            cv_params: vec![CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some("MS:1000016".to_string()),
                name: "scan start time".to_string(),
                value: Some(rt.to_string()),
                unit_cv_ref: Some("UO".to_string()),
                unit_name: Some("minute".to_string()),
                unit_accession: Some("UO:0000031".to_string()),
            }],
            ..Default::default()
        }],
        ..Default::default()
    });
    spectrum
}

fn encode_spectra_over_windows(rts: &[f64], mz_window: f64) -> Vec<u8> {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{MzML, Run, SpectrumList},
    };

    let point_count = 600;
    let mz: Vec<f64> = (0..point_count)
        .map(|step| 100.0 + step as f64 * 0.5)
        .collect();
    let int: Vec<f64> = (0..point_count).map(|step| step as f64).collect();

    let spectra: Vec<Spectrum> = rts
        .iter()
        .enumerate()
        .map(|(index, rt)| {
            make_spectrum_with_rt(format!("scan={index}"), *rt, mz.clone(), int.clone())
        })
        .collect();

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 3,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: true,
            section_storage: SectionStorage::Memory,
            mz_window,
        },
        &mut encoded,
    )
    .unwrap();
    encoded
}

fn open_reader(encoded: &[u8]) -> IonReader {
    IonReader::open(encoded, ReadOptions::default()).unwrap()
}

#[test]
fn eic_plan_covers_per_spectrum_plans() {
    let rts: Vec<f64> = (0..40).map(|step| step as f64).collect();
    let encoded = encode_spectra_over_windows(&rts, 100.0);
    let mut reader = open_reader(&encoded);

    let mz = Range {
        from: 150.0,
        to: 260.0,
    };
    let planned = reader.eic_byte_ranges(mz, None, 0).unwrap();
    assert!(!planned.is_empty());

    for scan_index in 0..rts.len() {
        for range in reader.byte_ranges(scan_index, mz).unwrap() {
            let range_end = range.offset + range.length;
            let covered = planned
                .iter()
                .any(|plan| plan.offset <= range.offset && range_end <= plan.offset + plan.length);
            assert!(
                covered,
                "scan {scan_index} range {range:?} missing from {planned:?}"
            );
        }
    }
}

#[test]
fn eic_plan_respects_rt_bounds() {
    let rts: Vec<f64> = (0..40).map(|step| step as f64).collect();
    let encoded = encode_spectra_over_windows(&rts, 100.0);
    let mut reader = open_reader(&encoded);

    let mz = Range {
        from: 150.0,
        to: 160.0,
    };
    let everything = reader.eic_byte_ranges(mz, None, 0).unwrap();
    let narrow = reader
        .eic_byte_ranges(mz, Some(Range { from: 0.0, to: 2.0 }), 0)
        .unwrap();

    assert!(!narrow.is_empty());
    assert!(narrow.len() <= everything.len());
    for range in &narrow {
        let range_end = range.offset + range.length;
        let covered = everything
            .iter()
            .any(|plan| plan.offset <= range.offset && range_end <= plan.offset + plan.length);
        assert!(covered, "range {range:?} is outside the unbounded plan");
    }

    let outside = reader
        .eic_byte_ranges(
            mz,
            Some(Range {
                from: 900.0,
                to: 1000.0,
            }),
            0,
        )
        .unwrap();
    assert!(outside.is_empty());
}

#[test]
fn eic_plan_straddles_window_boundary() {
    let rts: Vec<f64> = (0..20).map(|step| step as f64).collect();
    let encoded = encode_spectra_over_windows(&rts, 100.0);
    let mut reader = open_reader(&encoded);

    let low_window = reader
        .eic_byte_ranges(
            Range {
                from: 150.0,
                to: 160.0,
            },
            None,
            0,
        )
        .unwrap();
    let high_window = reader
        .eic_byte_ranges(
            Range {
                from: 250.0,
                to: 260.0,
            },
            None,
            0,
        )
        .unwrap();
    let straddling = reader
        .eic_byte_ranges(
            Range {
                from: 150.0,
                to: 260.0,
            },
            None,
            0,
        )
        .unwrap();

    let total_length = |ranges: &[ByteRange]| -> u64 { ranges.iter().map(|r| r.length).sum() };
    assert!(total_length(&straddling) >= total_length(&low_window));
    assert!(total_length(&straddling) >= total_length(&high_window));

    for side in [low_window, high_window] {
        for range in side {
            let range_end = range.offset + range.length;
            let covered = straddling
                .iter()
                .any(|plan| plan.offset <= range.offset && range_end <= plan.offset + plan.length);
            assert!(covered, "range {range:?} missing from the straddling plan");
        }
    }
}

#[test]
fn eic_plan_linear_fallback_for_nonfinite_rt() {
    let rts = vec![f64::NAN; 20];
    let encoded = encode_spectra_over_windows(&rts, 100.0);
    let mut reader = open_reader(&encoded);
    assert!(!reader.spec_rt_is_finite_ascending());

    let mz = Range {
        from: 150.0,
        to: 260.0,
    };
    let unbounded = reader.eic_byte_ranges(mz, None, 0).unwrap();
    let filtered = reader
        .eic_byte_ranges(mz, Some(Range { from: 0.0, to: 5.0 }), 0)
        .unwrap();

    assert!(!unbounded.is_empty());
    assert_eq!(unbounded, filtered);
}

#[test]
fn window_major_roundtrip_decodes_correctly() {
    let rts: Vec<f64> = (0..60).map(|step| step as f64).collect();
    let encoded = encode_spectra_over_windows(&rts, 100.0);
    let mut reader = open_reader(&encoded);

    let point_count = 600;
    let expected_mz: Vec<f64> = (0..point_count)
        .map(|step| 100.0 + step as f64 * 0.5)
        .collect();
    let expected_int: Vec<f64> = (0..point_count).map(|step| step as f64).collect();

    let everything = Range {
        from: 0.0,
        to: f64::MAX,
    };
    for scan_index in 0..rts.len() {
        let read = reader.read_window(scan_index, everything).unwrap();
        let NumericArray::F64(mz) = read.x else {
            panic!("expected an f64 mz array");
        };
        let NumericArray::F64(int) = read.y else {
            panic!("expected an f64 intensity array");
        };
        assert_eq!(mz, expected_mz, "scan {scan_index} mz mismatch");
        assert_eq!(int, expected_int, "scan {scan_index} intensity mismatch");
    }

    for (index, rt) in rts.iter().enumerate() {
        assert_eq!(reader.spectrum_rt(index), *rt);
    }

    let full_window = reader
        .eic_byte_ranges(
            Range {
                from: 200.0,
                to: 299.0,
            },
            None,
            0,
        )
        .unwrap();
    assert_eq!(
        full_window.len(),
        1,
        "a full window must resolve to one contiguous range"
    );
}

#[test]
fn eic_plan_full_window_is_single_range() {
    let rts: Vec<f64> = (0..200).map(|step| step as f64).collect();
    let encoded = encode_spectra_over_windows(&rts, 100.0);
    let mut reader = open_reader(&encoded);

    let full_window = reader
        .eic_byte_ranges(
            Range {
                from: 200.0,
                to: 299.0,
            },
            None,
            0,
        )
        .unwrap();
    assert_eq!(full_window.len(), 1);
}

#[test]
fn coalesce_merges_adjacent_and_respects_gap() {
    let mut overlapping = vec![
        ByteRange {
            offset: 0,
            length: 100,
        },
        ByteRange {
            offset: 50,
            length: 100,
        },
    ];
    coalesce_byte_ranges(&mut overlapping, 0);
    assert_eq!(
        overlapping,
        vec![ByteRange {
            offset: 0,
            length: 150
        }]
    );

    let mut touching = vec![
        ByteRange {
            offset: 0,
            length: 100,
        },
        ByteRange {
            offset: 100,
            length: 100,
        },
    ];
    coalesce_byte_ranges(&mut touching, 0);
    assert_eq!(
        touching,
        vec![ByteRange {
            offset: 0,
            length: 200
        }]
    );

    let mut inside_gap = vec![
        ByteRange {
            offset: 0,
            length: 100,
        },
        ByteRange {
            offset: 140,
            length: 60,
        },
    ];
    coalesce_byte_ranges(&mut inside_gap, 40);
    assert_eq!(
        inside_gap,
        vec![ByteRange {
            offset: 0,
            length: 200
        }]
    );

    let mut beyond_gap = vec![
        ByteRange {
            offset: 0,
            length: 100,
        },
        ByteRange {
            offset: 141,
            length: 60,
        },
    ];
    coalesce_byte_ranges(&mut beyond_gap, 40);
    assert_eq!(beyond_gap.len(), 2);

    let mut contained = vec![
        ByteRange {
            offset: 0,
            length: 500,
        },
        ByteRange {
            offset: 100,
            length: 50,
        },
    ];
    coalesce_byte_ranges(&mut contained, 0);
    assert_eq!(
        contained,
        vec![ByteRange {
            offset: 0,
            length: 500
        }]
    );

    let mut with_empty = vec![
        ByteRange {
            offset: 10,
            length: 0,
        },
        ByteRange {
            offset: 400,
            length: 10,
        },
    ];
    coalesce_byte_ranges(&mut with_empty, 0);
    assert_eq!(
        with_empty,
        vec![ByteRange {
            offset: 400,
            length: 10
        }]
    );

    let mut unsorted = vec![
        ByteRange {
            offset: 300,
            length: 50,
        },
        ByteRange {
            offset: 0,
            length: 100,
        },
        ByteRange {
            offset: 100,
            length: 50,
        },
    ];
    coalesce_byte_ranges(&mut unsorted, 0);
    assert_eq!(
        unsorted,
        vec![
            ByteRange {
                offset: 0,
                length: 150
            },
            ByteRange {
                offset: 300,
                length: 50
            },
        ]
    );

    let mut nothing: Vec<ByteRange> = Vec::new();
    coalesce_byte_ranges(&mut nothing, 16);
    assert!(nothing.is_empty());
}

#[test]
fn coalesced_open_ranges_cover_every_open_range() {
    let (_, _, encoded) = split_file_with_a1();
    let header = parse_header(&encoded[..1024]).unwrap();
    let planned = open_ranges(&encoded[..1024]).unwrap();

    let alignment_gap = 8;
    let mut coalesced = planned.clone();
    coalesce_byte_ranges(&mut coalesced, alignment_gap);

    for range in &planned {
        let range_end = range.offset + range.length;
        let covered = coalesced.iter().any(|merged| {
            merged.offset <= range.offset && range_end <= merged.offset + merged.length
        });
        assert!(covered, "range {range:?} is not covered by {coalesced:?}");
    }

    assert_eq!(coalesced.len(), 2);
    assert_eq!(coalesced[0].offset, 0);
    assert_eq!(coalesced[0].length, 1024);

    let (spec_directory_start, _) = spec_directory_range(&header);
    assert_eq!(coalesced[1].offset, spec_directory_start as u64);
    assert_eq!(
        coalesced[1].offset + coalesced[1].length,
        header.total_file_size
    );
}

#[test]
fn plan_open_ranges_includes_container_directories() {
    let (_, _, encoded) = split_file_with_a1();
    let header = parse_header(&encoded[..1024]).unwrap();
    let (start, end) = spec_directory_range(&header);

    let ranges = open_ranges(&encoded[..1024]).unwrap();
    assert!(ranges.contains(&ByteRange {
        offset: start as u64,
        length: (end - start) as u64,
    }));
}

#[test]
fn reader_plans_and_reads_mz_range() {
    let (mz, int, encoded) = split_file_with_a1();
    let bytes: Arc<[u8]> = Arc::from(encoded.as_slice());
    let mut reader =
        IonReader::open_source(Arc::new(BytesSource::new(bytes)), ReadOptions::default()).unwrap();

    reader.require_bounds().unwrap();

    let plan = reader
        .byte_ranges(
            0,
            Range {
                from: 120.0,
                to: 130.0,
            },
        )
        .unwrap();
    assert!(!plan.is_empty());

    let got = reader
        .read_window(
            0,
            Range {
                from: 120.0,
                to: 130.0,
            },
        )
        .unwrap();
    let (expected_mz, expected_int) = brute_force_window(&mz, &int, 120.0, 130.0);
    assert_eq!(got.x.to_f64(), expected_mz);
    assert_eq!(got.y.to_f64(), expected_int);
}

fn make_spectrum_with_arrays(id: String, mz: Vec<f64>, int: Vec<f64>) -> Spectrum {
    use crate::mzml::structs::BinaryDataArrayList;
    Spectrum {
        id,
        binary_data_array_list: Some(BinaryDataArrayList {
            count: Some(2),
            binary_data_arrays: vec![
                make_split_bda("MS:1000514", "m/z array", mz),
                make_split_bda("MS:1000515", "intensity array", int),
            ],
        }),
        ..Default::default()
    }
}

#[test]
fn roundtrip_compressed_sections_are_smaller_and_lossless() {
    use crate::{
        ion::encoder::{encode::WriteOptions, ion_writer::write_mzml_to_ion},
        mzml::structs::{MzML, Run, SpectrumList},
    };

    let spectrum_count = 200;
    let mz: Vec<f64> = (0..40).map(|i| 100.0 + i as f64 * 0.1).collect();
    let int: Vec<f64> = (0..40).map(|i| (i * 10) as f64).collect();

    let spectra: Vec<Spectrum> = (0..spectrum_count)
        .map(|i| make_spectrum_with_arrays(format!("scan={i}"), mz.clone(), int.clone()))
        .collect();

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 22,
            ..WriteOptions::default()
        },
        &mut encoded,
    )
    .unwrap();

    let header = parse_header(&encoded[..1024]).unwrap();
    assert_eq!(header.compression_codec, CODEC_ZSTD);
    assert_eq!(header.spectrum_count, spectrum_count as u64);

    let mut reader = IonReader::open(&encoded, ReadOptions::default()).unwrap();

    let address_count = reader.spec_array_addresses.len() as u64 / ARRAY_ADDRESS_BYTES as u64;
    assert_eq!(address_count, spectrum_count as u64 * 2);

    assert!(
        header.len_spec_summary < spectrum_count as u64 * SPEC_SUMMARY_SIZE as u64,
        "spec summary should shrink under compression: stored={}, raw={}",
        header.len_spec_summary,
        spectrum_count as u64 * SPEC_SUMMARY_SIZE as u64
    );
    assert!(
        header.len_spec_entries < spectrum_count as u64 * INDEX_ENTRY_BYTES as u64,
        "spec entries should shrink under compression: stored={}, raw={}",
        header.len_spec_entries,
        spectrum_count as u64 * INDEX_ENTRY_BYTES as u64
    );
    assert!(
        header.len_spec_array_addresses < address_count * ARRAY_ADDRESS_BYTES as u64,
        "spec array addresses should shrink under compression: stored={}, raw={}",
        header.len_spec_array_addresses,
        address_count * ARRAY_ADDRESS_BYTES as u64
    );

    assert_eq!(
        reader.spec_summary_buf.len(),
        spectrum_count * SPEC_SUMMARY_SIZE
    );
    assert_eq!(
        reader.spec_entries_buf.len(),
        spectrum_count * INDEX_ENTRY_BYTES
    );
    assert_eq!(
        reader.spec_array_addresses.len(),
        address_count as usize * ARRAY_ADDRESS_BYTES
    );

    let summaries = reader.spectrum_summaries().unwrap();
    assert_eq!(summaries.len(), spectrum_count);

    let mzml_out = reader.to_mzml().unwrap();
    let spectra_out = mzml_out.run.spectrum_list.unwrap().spectra;
    assert_eq!(spectra_out.len(), spectrum_count);
    for (index, spectrum) in spectra_out.iter().enumerate() {
        assert_eq!(spectrum.id, format!("scan={index}"));
        let arrays = &spectrum
            .binary_data_array_list
            .as_ref()
            .unwrap()
            .binary_data_arrays;
        let mz_out = arrays
            .iter()
            .find(|a| {
                a.cv_params
                    .iter()
                    .any(|cv| cv.accession.as_deref() == Some("MS:1000514"))
            })
            .and_then(|a| a.binary.as_ref())
            .unwrap();
        let int_out = arrays
            .iter()
            .find(|a| {
                a.cv_params
                    .iter()
                    .any(|cv| cv.accession.as_deref() == Some("MS:1000515"))
            })
            .and_then(|a| a.binary.as_ref())
            .unwrap();
        let NumericArray::F64(mz_vec) = mz_out else {
            panic!("expected F64 mz array");
        };
        let NumericArray::F64(int_vec) = int_out else {
            panic!("expected F64 intensity array");
        };
        assert_eq!(mz_vec, &mz);
        assert_eq!(int_vec, &int);
    }
}

#[test]
fn compression_disabled_keeps_sections_raw() {
    use crate::{
        ion::encoder::{encode::WriteOptions, ion_writer::write_mzml_to_ion},
        mzml::structs::{MzML, Run, SpectrumList},
    };

    let spectrum_count = 5;
    let mz = vec![100.0, 100.5, 101.0];
    let int = vec![10.0, 20.0, 30.0];

    let spectra: Vec<Spectrum> = (0..spectrum_count)
        .map(|i| make_spectrum_with_arrays(format!("scan={i}"), mz.clone(), int.clone()))
        .collect();

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(
        &mzml_in,
        WriteOptions {
            compression_level: 0,
            ..WriteOptions::default()
        },
        &mut encoded,
    )
    .unwrap();

    let header = parse_header(&encoded[..1024]).unwrap();
    assert_eq!(header.compression_codec, CODEC_NONE);
    assert_eq!(
        header.len_spec_summary,
        spectrum_count as u64 * SPEC_SUMMARY_SIZE as u64
    );

    let mut reader = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let mzml_out = reader.to_mzml().unwrap();
    assert_eq!(
        mzml_out.run.spectrum_list.unwrap().spectra.len(),
        spectrum_count
    );
}

#[test]
fn compressed_roundtrip_edge_cases() {
    use crate::{
        ion::encoder::{encode::WriteOptions, ion_writer::write_mzml_to_ion},
        mzml::structs::{BinaryDataArrayList, MzML, Run, SpectrumList},
    };

    let compressed_options = WriteOptions {
        compression_level: 22,
        ..WriteOptions::default()
    };

    {
        let spectrum =
            make_spectrum_with_arrays("scan=1".to_string(), vec![100.0, 100.5], vec![1.0, 2.0]);
        let mzml_in = MzML {
            run: Run {
                spectrum_list: Some(SpectrumList {
                    spectra: vec![spectrum],
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut encoded = Vec::new();
        write_mzml_to_ion(&mzml_in, compressed_options, &mut encoded).unwrap();

        let header = parse_header(&encoded[..1024]).unwrap();
        assert_eq!(header.compression_codec, CODEC_ZSTD);
        assert_eq!(header.len_chrom_summary, 0);
        assert_eq!(header.len_chrom_entries, 0);
        assert_eq!(header.len_chrom_array_addresses, 0);

        let mut reader = IonReader::open(&encoded, ReadOptions::default()).unwrap();
        let mzml_out = reader.to_mzml().unwrap();
        assert_eq!(mzml_out.run.spectrum_list.unwrap().spectra.len(), 1);
        assert_eq!(reader.chromatogram_count(), 0);
    }

    {
        let mzml_in = MzML::default();

        let mut encoded = Vec::new();
        write_mzml_to_ion(&mzml_in, compressed_options, &mut encoded).unwrap();

        let header = parse_header(&encoded[..1024]).unwrap();
        assert_eq!(header.compression_codec, CODEC_ZSTD);
        assert_eq!(header.len_spec_summary, 0);
        assert_eq!(header.len_spec_entries, 0);
        assert_eq!(header.len_spec_array_addresses, 0);
        assert_eq!(header.len_chrom_summary, 0);
        assert_eq!(header.len_chrom_entries, 0);
        assert_eq!(header.len_chrom_array_addresses, 0);

        let reader = IonReader::open(&encoded, ReadOptions::default()).unwrap();
        assert_eq!(reader.spectrum_count(), 0);
        assert_eq!(reader.chromatogram_count(), 0);
    }

    {
        let empty_array_spectrum = Spectrum {
            id: "scan=1".to_string(),
            binary_data_array_list: Some(BinaryDataArrayList {
                count: Some(0),
                binary_data_arrays: vec![],
            }),
            ..Default::default()
        };
        let mzml_in = MzML {
            run: Run {
                spectrum_list: Some(SpectrumList {
                    spectra: vec![empty_array_spectrum],
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut encoded = Vec::new();
        write_mzml_to_ion(&mzml_in, compressed_options, &mut encoded).unwrap();

        let header = parse_header(&encoded[..1024]).unwrap();
        assert_eq!(header.compression_codec, CODEC_ZSTD);
        assert_eq!(header.len_spec_array_addresses, 0);

        let mut reader = IonReader::open(&encoded, ReadOptions::default()).unwrap();
        let mzml_out = reader.to_mzml().unwrap();
        assert_eq!(mzml_out.run.spectrum_list.unwrap().spectra.len(), 1);
    }
}

#[test]
fn ms_level_round_trips_without_cvparam_3() {
    use crate::{
        ion::encoder::{encode::WriteOptions, ion_writer::write_mzml_to_ion},
        mzml::structs::{MzML, Run, SpectrumList},
    };

    let spectrum = Spectrum {
        ms_level: Some(2),
        ..make_spectrum_with_arrays("scan=1".to_string(), vec![100.0, 100.5], vec![1.0, 2.0])
    };
    assert!(
        spectrum
            .cv_params
            .iter()
            .all(|cv| cv.accession.as_deref() != Some("MS:1000511")),
        "test setup must not already carry an ms-level cvParam"
    );

    let mzml_in = MzML {
        run: Run {
            spectrum_list: Some(SpectrumList {
                spectra: vec![spectrum],
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut encoded = Vec::new();
    write_mzml_to_ion(&mzml_in, WriteOptions::default(), &mut encoded).unwrap();

    let mut reader = IonReader::open(&encoded, ReadOptions::default()).unwrap();
    let mzml_out = reader.to_mzml().unwrap();
    let decoded_spectrum = mzml_out
        .run
        .spectrum_list
        .unwrap()
        .spectra
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(decoded_spectrum.ms_level, Some(2));
}
