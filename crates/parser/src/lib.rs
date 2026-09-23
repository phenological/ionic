mod api;
pub(crate) mod accessions;
pub(crate) mod ion;
pub mod mzml;
pub(crate) mod utilities;

pub use api::{ConvertKind, ConvertOptions, IonReader, IonWriter, ReadOptions, ScanQuery, WriteOptions, convert};
#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub use api::convert_file;
pub use ion::{
    ArrayKind, ChromatogramSummary, DataXY, DecompressionLimit, IonError, IonResult, Range,
    ScanStream, ScanSummary, SectionStorage, Select, SpectrumSummary, TimeUnit, Window,
    decoder::decode::{Metadatum, MetadatumValue},
};

pub mod format;
pub mod source;
