use serde::{Deserialize, Serialize};

use crate::accessions::{
    ACC_COMPRESSION_NONE, FLOAT_64BIT, INTENSITY_ARRAY, MZ_ARRAY, format_accession,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MzML {
    pub cv_list: Option<CvList>,
    pub file_description: Option<FileDescription>,
    pub referenceable_param_group_list: Option<ReferenceableParamGroupList>,
    pub sample_list: Option<SampleList>,
    pub instrument_list: Option<InstrumentList>,
    pub software_list: Option<SoftwareList>,
    pub data_processing_list: Option<DataProcessingList>,
    pub scan_settings_list: Option<ScanSettingsList>,
    pub run: Run,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CvList {
    pub count: Option<usize>,
    pub cv: Vec<CvEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvEntry {
    pub id: String,
    pub full_name: Option<String>,
    pub version: Option<String>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexOffset {
    pub id_ref: Option<String>,
    pub offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexList {
    pub spectrum: Vec<IndexOffset>,
    pub chromatogram: Vec<IndexOffset>,
    pub index_list_offset: Option<u64>,
    pub file_checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedmzML {
    pub mzml: MzML,
    pub index_list: IndexList,
    pub index_list_offset: Option<u64>,
    pub file_checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CvParam {
    pub cv_ref: Option<String>,
    pub accession: Option<String>,
    pub name: String,
    pub value: Option<String>,
    pub unit_cv_ref: Option<String>,
    pub unit_name: Option<String>,
    pub unit_accession: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserParam {
    pub name: String,
    pub r#type: Option<String>,
    pub unit_accession: Option<String>,
    pub unit_cv_ref: Option<String>,
    pub unit_name: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReferenceableParamGroupRef {
    pub r#ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DataProcessingList {
    pub count: Option<usize>,
    pub data_processing: Vec<DataProcessing>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DataProcessing {
    pub id: String,
    pub software_ref: Option<String>,
    pub processing_method: Vec<ProcessingMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingMethod {
    pub order: Option<u32>,
    pub software_ref: Option<String>,
    pub referenceable_param_group_ref: Vec<ReferenceableParamGroupRef>,
    pub cv_param: Vec<CvParam>,
    pub user_param: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileDescription {
    pub file_content: FileContent,
    pub source_file_list: SourceFileList,
    pub contacts: Vec<Contact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceFileList {
    pub count: Option<usize>,
    pub source_file: Vec<SourceFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceFile {
    pub id: String,
    pub name: String,
    pub location: String,
    pub referenceable_param_group_ref: Vec<ReferenceableParamGroupRef>,
    pub cv_param: Vec<CvParam>,
    pub user_param: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileContent {
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Contact {
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstrumentList {
    pub count: Option<usize>,
    pub instrument: Vec<Instrument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Instrument {
    pub id: String,
    pub scan_settings_ref: Option<ScanSettingsRef>,
    pub cv_param: Vec<CvParam>,
    pub user_param: Vec<UserParam>,
    pub referenceable_param_group_ref: Vec<ReferenceableParamGroupRef>,
    pub component_list: Option<ComponentList>,
    pub software_ref: Option<InstrumentSoftwareRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanSettingsRef {
    pub r#ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComponentList {
    pub count: Option<usize>,
    pub source: Vec<Source>,
    pub analyzer: Vec<Analyzer>,
    pub detector: Vec<Detector>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Source {
    pub order: Option<u32>,
    pub referenceable_param_group_ref: Vec<ReferenceableParamGroupRef>,
    pub cv_param: Vec<CvParam>,
    pub user_param: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Analyzer {
    pub order: Option<u32>,
    pub referenceable_param_group_ref: Vec<ReferenceableParamGroupRef>,
    pub cv_param: Vec<CvParam>,
    pub user_param: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Detector {
    pub order: Option<u32>,
    pub referenceable_param_group_ref: Vec<ReferenceableParamGroupRef>,
    pub cv_param: Vec<CvParam>,
    pub user_param: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstrumentSoftwareRef {
    pub r#ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReferenceableParamGroupList {
    pub count: Option<usize>,
    pub referenceable_param_groups: Vec<ReferenceableParamGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReferenceableParamGroup {
    pub id: String,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SampleList {
    pub count: Option<u32>,
    pub samples: Vec<Sample>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Sample {
    pub id: String,
    pub name: String,
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanSettingsList {
    pub count: Option<usize>,
    pub scan_settings: Vec<ScanSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanSettings {
    pub id: Option<String>,
    pub instrument_configuration_ref: Option<String>,
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
    pub source_file_ref_list: Option<SourceFileRefList>,
    pub target_list: Option<TargetList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceFileRefList {
    pub count: Option<usize>,
    pub source_file_refs: Vec<SourceFileRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceFileRef {
    pub r#ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetList {
    pub count: Option<usize>,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Target {
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SoftwareList {
    pub count: Option<usize>,
    pub software: Vec<Software>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Software {
    pub id: String,
    pub version: Option<String>,
    pub software_param: Vec<SoftwareParam>,
    pub cv_param: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SoftwareParam {
    pub cv_ref: Option<String>,
    pub accession: String,
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Run {
    pub id: String,
    pub start_time_stamp: Option<String>,
    pub default_instrument_configuration_ref: Option<String>,
    pub default_source_file_ref: Option<String>,
    pub sample_ref: Option<String>,

    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,

    pub source_file_ref_list: Option<SourceFileRefList>,
    pub spectrum_list: Option<SpectrumList>,
    pub chromatogram_list: Option<ChromatogramList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpectrumList {
    pub count: Option<usize>,
    pub default_data_processing_ref: Option<String>,
    pub spectra: Vec<Spectrum>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpectrumDescription {
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,

    pub scan_list: Option<ScanList>,
    pub precursor_list: Option<PrecursorList>,
    pub product_list: Option<ProductList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanList {
    pub count: Option<usize>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub scans: Vec<Scan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Scan {
    pub instrument_configuration_ref: Option<String>,
    pub external_spectrum_id: Option<String>,
    pub source_file_ref: Option<String>,
    pub spectrum_ref: Option<String>,

    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,

    pub scan_window_list: Option<ScanWindowList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanWindowList {
    pub count: Option<usize>,
    pub scan_windows: Vec<ScanWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanWindow {
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrecursorList {
    pub count: Option<usize>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
    pub precursors: Vec<Precursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Precursor {
    pub spectrum_ref: Option<String>,
    pub source_file_ref: Option<String>,
    pub external_spectrum_id: Option<String>,

    pub isolation_window: Option<IsolationWindow>,
    pub selected_ion_list: Option<SelectedIonList>,
    pub activation: Option<Activation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IsolationWindow {
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelectedIonList {
    pub count: Option<usize>,
    pub selected_ions: Vec<SelectedIon>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelectedIon {
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Activation {
    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductList {
    pub count: Option<usize>,
    pub products: Vec<Product>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Product {
    pub spectrum_ref: Option<String>,
    pub source_file_ref: Option<String>,
    pub external_spectrum_id: Option<String>,
    pub isolation_window: Option<IsolationWindow>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BinaryDataArrayList {
    pub count: Option<usize>,
    pub binary_data_arrays: Vec<BinaryDataArray>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Copy, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NumericType {
    Float16,
    Float32,
    Float64,
    Int64,
    Int32,
    Int16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumericArray {
    F64(Vec<f64>),
    F32(Vec<f32>),
    F16(Vec<u16>),
    I64(Vec<i64>),
    I32(Vec<i32>),
    I16(Vec<i16>),
}

impl NumericArray {
    pub fn len(&self) -> usize {
        match self {
            Self::F64(v) => v.len(),
            Self::F32(v) => v.len(),
            Self::F16(v) => v.len(),
            Self::I64(v) => v.len(),
            Self::I32(v) => v.len(),
            Self::I16(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BinaryDataArray {
    pub array_length: Option<usize>,
    pub encoded_length: Option<usize>,
    pub data_processing_ref: Option<String>,

    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,

    pub numeric_type: Option<NumericType>,
    pub binary: Option<NumericArray>,

    #[serde(skip)]
    pub pending_base64: Option<Vec<u8>>,
    #[serde(skip)]
    pub pending_zlib: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChromatogramList {
    pub count: Option<usize>,
    pub default_data_processing_ref: Option<String>,
    pub chromatograms: Vec<Chromatogram>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Chromatogram {
    pub id: String,
    pub native_id: Option<String>,
    pub index: Option<u32>,
    pub default_array_length: Option<usize>,
    pub data_processing_ref: Option<String>,

    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,

    pub precursor: Option<Precursor>,
    pub product: Option<Product>,

    pub binary_data_array_list: Option<BinaryDataArrayList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Spectrum {
    pub id: String,
    pub index: Option<u32>,
    pub scan_number: Option<u32>,
    pub default_array_length: Option<usize>,
    pub native_id: Option<String>,
    pub data_processing_ref: Option<String>,
    pub source_file_ref: Option<String>,
    pub spot_id: Option<String>,
    pub ms_level: Option<u32>,

    pub referenceable_param_group_refs: Vec<ReferenceableParamGroupRef>,
    pub cv_params: Vec<CvParam>,
    pub user_params: Vec<UserParam>,

    pub spectrum_description: Option<SpectrumDescription>,

    pub scan_list: Option<ScanList>,
    pub precursor_list: Option<PrecursorList>,
    pub product_list: Option<ProductList>,
    pub binary_data_array_list: Option<BinaryDataArrayList>,
}

impl Spectrum {
    pub fn new(id: impl Into<String>, mz: Vec<f64>, intensity: Vec<f64>) -> Self {
        let default_array_length = mz.len();
        let mz_array = float64_array(MZ_ARRAY, "m/z array", mz);
        let intensity_array = float64_array(INTENSITY_ARRAY, "intensity array", intensity);
        Self {
            id: id.into(),
            default_array_length: Some(default_array_length),
            binary_data_array_list: Some(BinaryDataArrayList {
                count: Some(2),
                binary_data_arrays: vec![mz_array, intensity_array],
            }),
            ..Default::default()
        }
    }
}

fn float64_array(accession_tail: u32, name: &str, values: Vec<f64>) -> BinaryDataArray {
    BinaryDataArray {
        array_length: Some(values.len()),
        numeric_type: Some(NumericType::Float64),
        cv_params: vec![
            CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(format_accession(accession_tail)),
                name: name.to_string(),
                ..Default::default()
            },
            CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(format_accession(FLOAT_64BIT)),
                name: "64-bit float".to_string(),
                ..Default::default()
            },
            CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(ACC_COMPRESSION_NONE.to_string()),
                name: "no compression".to_string(),
                ..Default::default()
            },
        ],
        binary: Some(NumericArray::F64(values)),
        ..Default::default()
    }
}
