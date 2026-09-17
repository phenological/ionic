use std::sync::Arc;

use super::*;
use crate::{
    ion::encoder::utilities::{SectionChunk, SectionStorage},
    mzml::structs::CvParam,
};

#[test]
fn value_pool_empty_value_gives_kind_2() {
    let mut pool = ValuePool::new();
    assert_eq!(pool.encode(None), ValueEncoding { kind: 2, index: 0 });
    assert_eq!(pool.encode(Some("")), ValueEncoding { kind: 2, index: 0 });
}

#[test]
fn value_pool_numeric_increments_index() {
    let mut pool = ValuePool::new();
    assert_eq!(
        pool.encode(Some("1.5")),
        ValueEncoding { kind: 0, index: 0 }
    );
    assert_eq!(
        pool.encode(Some("2.5")),
        ValueEncoding { kind: 0, index: 1 }
    );
    assert_eq!(pool.numeric_values, vec![1.5f64, 2.5f64]);
}

#[test]
fn value_pool_string_increments_index() {
    let mut pool = ValuePool::new();
    assert_eq!(
        pool.encode(Some("hello")),
        ValueEncoding { kind: 1, index: 0 }
    );
    assert_eq!(
        pool.encode(Some("world")),
        ValueEncoding { kind: 1, index: 1 }
    );
    assert_eq!(&pool.string_bytes[..5], b"hello");
}

#[test]
fn value_pool_string_offsets_are_cumulative() {
    let mut pool = ValuePool::new();
    pool.encode(Some("ab"));
    pool.encode(Some("cde"));
    assert_eq!(pool.string_offsets, vec![0, 2]);
    assert_eq!(pool.string_lengths, vec![2, 3]);
}

#[test]
fn meta_param_buffer_push_records_id() {
    let mut buffer = MetaParamBuffer::new();
    buffer.push(TagId::CvParam, 1, 0, empty_cv_param());
    assert_eq!(buffer.rows.len(), 1);
    assert_eq!(buffer.rows[0].id, 1);
}

#[test]
fn meta_param_buffer_normalize_moves_name_to_value_for_attr_cv() {
    let mut buffer = MetaParamBuffer::new();
    buffer.rows.push(MetaParamRow {
        cv_param: CvParam {
            cv_ref: Some(CV_REF_ATTR.to_string()),
            accession: Some(format!("{}:9910001", CV_REF_ATTR)),
            name: "my-id".to_string(),
            value: None,
            unit_cv_ref: None,
            unit_name: None,
            unit_accession: None,
        },
        tag_id: TagId::CvParam as u8,
        id: 1,
        parent_id: 0,
    });
    buffer.normalize_attr_cv_values();
    assert_eq!(buffer.rows[0].cv_param.value.as_deref(), Some("my-id"));
    assert!(buffer.rows[0].cv_param.name.is_empty());
}

#[test]
fn meta_param_buffer_normalize_skips_non_attr_cv() {
    let mut buffer = MetaParamBuffer::new();
    buffer.rows.push(MetaParamRow {
        cv_param: CvParam {
            cv_ref: Some("MS".to_string()),
            accession: Some("MS:1000514".to_string()),
            name: "m/z array".to_string(),
            value: None,
            unit_cv_ref: None,
            unit_name: None,
            unit_accession: None,
        },
        tag_id: TagId::CvParam as u8,
        id: 1,
        parent_id: 0,
    });
    buffer.normalize_attr_cv_values();
    assert_eq!(buffer.rows[0].cv_param.name, "m/z array");
    assert!(buffer.rows[0].cv_param.value.is_none());
}

#[test]
fn packed_meta_builder_empty_produces_single_sentinel() {
    let meta = PackedMetaBuilder::new().build();
    assert_eq!(meta.index_offsets, vec![0]);
    assert!(meta.ids.is_empty());
}

#[test]
fn packed_meta_builder_flush_buffer_advances_index_offsets() {
    let mut builder = PackedMetaBuilder::new();
    let mut buffer = MetaParamBuffer::new();
    buffer.push(TagId::CvParam, 1, 0, empty_cv_param());
    buffer.push(TagId::CvParam, 1, 0, empty_cv_param());
    builder.flush_buffer(&buffer);
    let meta = builder.build();
    assert_eq!(meta.index_offsets, vec![0, 2]);
    assert_eq!(meta.ids.len(), 2);
}

#[test]
fn encode_user_param_with_value_uses_separator() {
    use crate::mzml::structs::UserParam;
    let p = UserParam {
        name: "my-param".to_string(),
        value: Some("42".to_string()),
        r#type: Some("xsd:float".to_string()),
        unit_cv_ref: None,
        unit_name: None,
        unit_accession: None,
    };
    let encoded = encode_user_param_as_cv(&p).value.unwrap();
    let parts: Vec<&str> = encoded.splitn(3, USER_PARAM_NAME_VALUE_SEPARATOR).collect();
    assert_eq!(parts[0], "my-param");
    assert_eq!(parts[1], "42");
    assert_eq!(parts[2], "xsd:float");
}

#[test]
fn compress_bytes_if_enabled_level_zero_is_identity() {
    let input = vec![1u8, 2, 3, 4];
    assert_eq!(compress_bytes_if_enabled(input.clone(), 0), input);
}

#[test]
fn array_type_accession_from_bda_returns_mz_array() {
    use crate::{accessions::MZ_ARRAY, mzml::structs::BinaryDataArray};
    let bda = BinaryDataArray {
        cv_params: vec![CvParam {
            cv_ref: Some("MS".to_string()),
            accession: Some("MS:1000514".to_string()),
            name: "m/z array".to_string(),
            value: None,
            unit_cv_ref: None,
            unit_name: None,
            unit_accession: None,
        }],
        ..Default::default()
    };
    assert_eq!(array_type_accession_from_binary_data_array(&bda), MZ_ARRAY);
}

#[test]
fn array_type_accession_from_bda_returns_zero_when_absent() {
    use crate::mzml::structs::BinaryDataArray;
    assert_eq!(
        array_type_accession_from_binary_data_array(&BinaryDataArray::default()),
        0
    );
}

#[test]
fn collector_global_meta_on_empty_mzml_produces_run_buffer() {
    use crate::mzml::structs::MzML;
    let mzml = MzML::default();
    let mut collector = MetaCollector::new();
    let (meta, counts) = collector.collect_global_meta(&mzml);
    assert_eq!(counts.n_run, 1);
    assert!(!meta.ids.is_empty());
}

#[test]
fn grouped_metadata_keeps_values_local_to_each_group() {
    use crate::ion::{
        DecompressionLimit,
        decoder::decode::MetadatumValue,
        format::CODEC_NONE,
        meta_groups::MetaTotals,
        utilities::{MetaGroupReader, meta_column_layout::MetaColumnLayout},
    };

    let grouped = three_item_grouped();
    assert_eq!(grouped.group_count, 3);

    let mut reader = MetaGroupReader::new(
        Arc::from(grouped_bytes(&grouped)),
        grouped.group_count,
        1,
        3,
        MetaTotals {
            rows: 3,
            numeric: 3,
            string: 0,
            uncompressed: grouped.uncompressed_size,
        },
        CODEC_NONE,
        true,
        DecompressionLimit::default(),
        64 * 1024 * 1024,
        MetaColumnLayout::new(),
    )
    .unwrap();

    let all = reader.read_all().unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].item_index, 0);
    assert_eq!(all[1].item_index, 1);
    assert_eq!(all[2].item_index, 2);
    assert_eq!(all[0].value, MetadatumValue::Number(10.5));
    assert_eq!(all[1].value, MetadatumValue::Number(20.5));
    assert_eq!(all[2].value, MetadatumValue::Number(30.5));

    let second = reader.read_item(1).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].item_index, 1);
    assert_eq!(second[0].value, MetadatumValue::Number(20.5));

    let third = reader.read_item(2).unwrap();
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].value, MetadatumValue::Number(30.5));
}

fn three_item_grouped() -> grouper::GroupedSection {
    let make_cv = |value: &str| CvParam {
        cv_ref: Some("MS".to_string()),
        accession: Some("MS:1000285".to_string()),
        name: String::new(),
        value: Some(value.to_string()),
        unit_cv_ref: None,
        unit_name: None,
        unit_accession: None,
    };
    let mut grouper = grouper::MetaGrouper::new(1, 0, SectionChunk::memory(0));
    for (index, value) in ["10.5", "20.5", "30.5"].iter().enumerate() {
        let mut buffer = MetaParamBuffer::new();
        buffer.push(TagId::CvParam, (index + 1) as u32, 0, make_cv(value));
        buffer.normalize_attr_cv_values();
        grouper.write_metadata_item(&buffer).unwrap();
    }
    grouper.finish().unwrap()
}

fn grouped_bytes(grouped: &grouper::GroupedSection) -> &[u8] {
    grouped.section.as_slice()
}

#[test]
fn metadata_reader_rejects_wrong_uncompressed_total() {
    use crate::ion::{
        DecompressionLimit,
        format::CODEC_NONE,
        meta_groups::MetaTotals,
        utilities::{MetaGroupReader, meta_column_layout::MetaColumnLayout},
    };

    let grouped = three_item_grouped();
    let result = MetaGroupReader::new(
        Arc::from(grouped_bytes(&grouped)),
        grouped.group_count,
        1,
        3,
        MetaTotals {
            rows: 3,
            numeric: 3,
            string: 0,
            uncompressed: grouped.uncompressed_size + 1,
        },
        CODEC_NONE,
        true,
        DecompressionLimit::default(),
        64 * 1024 * 1024,
        MetaColumnLayout::new(),
    );
    assert!(result.is_err());
}

#[test]
fn metadata_reader_rejects_wrong_row_total() {
    use crate::ion::{
        DecompressionLimit,
        format::CODEC_NONE,
        meta_groups::MetaTotals,
        utilities::{MetaGroupReader, meta_column_layout::MetaColumnLayout},
    };

    let grouped = three_item_grouped();
    let reader = MetaGroupReader::new(
        Arc::from(grouped_bytes(&grouped)),
        grouped.group_count,
        1,
        3,
        MetaTotals {
            rows: 99,
            numeric: 3,
            string: 0,
            uncompressed: grouped.uncompressed_size,
        },
        CODEC_NONE,
        true,
        DecompressionLimit::default(),
        64 * 1024 * 1024,
        MetaColumnLayout::new(),
    )
    .unwrap();
    assert!(reader.read_all().is_err());
}

#[test]
fn metadata_reader_rejects_payload_into_directory() {
    use crate::ion::{
        DecompressionLimit,
        format::CODEC_NONE,
        meta_groups::{META_GROUP_ENTRY_SIZE, MetaTotals},
        utilities::{MetaGroupReader, meta_column_layout::MetaColumnLayout},
    };

    let grouped = three_item_grouped();
    let mut bytes = grouped_bytes(&grouped).to_vec();
    let directory_start = bytes.len() - grouped.group_count as usize * META_GROUP_ENTRY_SIZE;
    bytes[directory_start..directory_start + 8]
        .copy_from_slice(&(directory_start as u64).to_le_bytes());

    let mut reader = MetaGroupReader::new(
        Arc::from(bytes.as_slice()),
        grouped.group_count,
        1,
        3,
        MetaTotals {
            rows: 3,
            numeric: 3,
            string: 0,
            uncompressed: grouped.uncompressed_size,
        },
        CODEC_NONE,
        true,
        DecompressionLimit::default(),
        64 * 1024 * 1024,
        MetaColumnLayout::new(),
    )
    .unwrap();
    assert!(reader.read_item(0).is_err());
}

#[test]
fn array_policy_identifies_xy_arrays() {
    use crate::accessions::{INTENSITY_ARRAY, MZ_ARRAY, TIME_ARRAY};
    let policy = ArrayPolicy {
        x_array_accession: MZ_ARRAY,
        y_array_accession: INTENSITY_ARRAY,
        force_f32: true,
    };
    assert!(policy.is_xy_array(MZ_ARRAY));
    assert!(policy.is_xy_array(INTENSITY_ARRAY));
    assert!(!policy.is_xy_array(TIME_ARRAY));
    assert!(policy.should_force_f32(MZ_ARRAY));
}

#[test]
fn array_policy_no_force_when_disabled() {
    use crate::accessions::{INTENSITY_ARRAY, MZ_ARRAY};
    let policy = ArrayPolicy {
        x_array_accession: MZ_ARRAY,
        y_array_accession: INTENSITY_ARRAY,
        force_f32: false,
    };
    assert!(!policy.should_force_f32(MZ_ARRAY));
}

#[test]
fn group_local_node_ids_across_group_boundaries() {
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

    fn make_array(accession: &str, data: Vec<f64>) -> BinaryDataArray {
        BinaryDataArray {
            cv_params: vec![CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(accession.to_string()),
                name: String::new(),
                ..Default::default()
            }],
            binary: Some(NumericArray::F64(data)),
            ..Default::default()
        }
    }

    let mut spectra = Vec::new();
    for i in 0..9000 {
        let mz_data: Vec<f64> = (0..10).map(|j| 100.0 + i as f64 + j as f64 * 0.1).collect();
        let intensity_data: Vec<f64> = (0..10).map(|j| 1000.0 + i as f64 + j as f64).collect();

        let spectrum = Spectrum {
            id: format!("spectrum={}", i),
            index: Some(i as u32),
            binary_data_array_list: Some(BinaryDataArrayList {
                count: Some(2),
                binary_data_arrays: vec![
                    make_array("MS:1000514", mz_data),
                    make_array("MS:1000515", intensity_data),
                ],
            }),
            ..Default::default()
        };
        spectra.push(spectrum);
    }

    let run = Run {
        id: "run1".to_string(),
        spectrum_list: Some(SpectrumList {
            count: Some(spectra.len()),
            spectra,
            ..Default::default()
        }),
        ..Default::default()
    };

    let mzml = MzML {
        run,
        ..Default::default()
    };

    let mut output = Vec::new();
    write_mzml_to_ion(
        &mzml,
        WriteOptions {
            compression_level: 0,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: false,
            section_storage: SectionStorage::Memory,
            mz_window: 0.0,
        },
        &mut output,
    )
    .unwrap();

    use crate::{
        ion::decoder::decode::{IonReader, Metadatum, ReadOptions},
        mzml::schema::TagId,
    };

    fn find_row(rows: &[Metadatum], tag: TagId, id: u32) -> &Metadatum {
        rows.iter()
            .find(|row| row.tag_id == tag && row.id == id)
            .unwrap_or_else(|| panic!("row not found: tag={:?}, id={}", tag, id))
    }

    let mut decoder =
        IonReader::open(&output, ReadOptions::default()).expect("failed to open decoder");

    let first_rows = decoder
        .spectrum_metadata_at(0)
        .expect("failed to read first spectrum metadata");
    let second_rows = decoder
        .spectrum_metadata_at(8192)
        .expect("failed to read second spectrum metadata");

    let first_list = find_row(&first_rows, TagId::SpectrumList, LOCAL_LIST_NODE_ID);
    let second_list = find_row(&second_rows, TagId::SpectrumList, LOCAL_LIST_NODE_ID);

    assert_eq!(
        first_list.parent_id, 0,
        "first group list should have parent_id=0"
    );
    assert_eq!(
        second_list.parent_id, 0,
        "second group list should have parent_id=0"
    );

    let first_spectrum = find_row(&first_rows, TagId::Spectrum, FIRST_LOCAL_ITEM_NODE_ID);
    let second_spectrum = find_row(&second_rows, TagId::Spectrum, FIRST_LOCAL_ITEM_NODE_ID);

    assert_eq!(
        first_spectrum.parent_id, LOCAL_LIST_NODE_ID,
        "first group spectrum should parent to list id=1"
    );
    assert_eq!(
        second_spectrum.parent_id, LOCAL_LIST_NODE_ID,
        "second group spectrum should parent to list id=1"
    );

    let first_bda_list = first_rows
        .iter()
        .find(|row| row.tag_id == TagId::BinaryDataArrayList)
        .expect("first group should have BinaryDataArrayList");
    let second_bda_list = second_rows
        .iter()
        .find(|row| row.tag_id == TagId::BinaryDataArrayList)
        .expect("second group should have BinaryDataArrayList");

    assert_eq!(
        first_bda_list.parent_id, FIRST_LOCAL_ITEM_NODE_ID,
        "first group BinaryDataArrayList should parent to spectrum id=2"
    );
    assert_eq!(
        second_bda_list.parent_id, FIRST_LOCAL_ITEM_NODE_ID,
        "second group BinaryDataArrayList should parent to spectrum id=2"
    );
}

#[test]
fn product_own_cv_params_parent_to_product_list_not_to_the_product_itself() {
    use crate::{
        ion::encoder::{
            encode::{TARGET_BLOCK_UNCOMPRESSED_BYTES, WriteOptions},
            ion_writer::write_mzml_to_ion,
        },
        mzml::structs::{CvParam, MzML, Product, ProductList, Run, Spectrum, SpectrumList},
    };

    let product = Product {
        cv_params: vec![CvParam {
            cv_ref: Some("MS".to_string()),
            accession: Some("MS:1000827".to_string()),
            name: "selected reaction monitoring transition".to_string(),
            value: Some("1.0".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let spectrum = Spectrum {
        id: "spectrum=0".to_string(),
        index: Some(0),
        product_list: Some(ProductList {
            count: Some(1),
            products: vec![product],
            ..Default::default()
        }),
        ..Default::default()
    };

    let run = Run {
        id: "run1".to_string(),
        spectrum_list: Some(SpectrumList {
            count: Some(1),
            spectra: vec![spectrum],
            ..Default::default()
        }),
        ..Default::default()
    };

    let mzml = MzML {
        run,
        ..Default::default()
    };

    let mut output = Vec::new();
    write_mzml_to_ion(
        &mzml,
        WriteOptions {
            compression_level: 0,
            force_f32: false,
            block_size: TARGET_BLOCK_UNCOMPRESSED_BYTES,
            parallel: false,
            section_storage: SectionStorage::Memory,
            mz_window: 0.0,
        },
        &mut output,
    )
    .unwrap();

    use crate::{
        ion::decoder::decode::{IonReader, ReadOptions},
        mzml::schema::TagId,
    };

    let mut decoder =
        IonReader::open(&output, ReadOptions::default()).expect("failed to open decoder");
    let rows = decoder
        .spectrum_metadata_at(0)
        .expect("failed to read spectrum metadata");

    let product_row = rows
        .iter()
        .find(|row| row.tag_id == TagId::Product)
        .expect("product touch row not found");

    let product_own_param = rows
        .iter()
        .find(|row| row.tag_id == TagId::CvParam && row.id == product_row.id)
        .expect("product cv_param row not found");

    assert_ne!(
        product_own_param.parent_id, product_row.id,
        "product's own cv_param must not be parented to the product's own node id"
    );
    assert_eq!(
        product_own_param.parent_id, product_row.parent_id,
        "product's own cv_param must share the product's parent node id"
    );
}
