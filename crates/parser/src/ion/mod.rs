#![allow(unused_imports)]
pub(crate) mod array_kind;
pub(crate) mod byte_transpose;
pub(crate) mod filter_summary;
pub(crate) mod header;
pub(crate) mod meta_groups;
pub(crate) mod packing;
pub(crate) mod range;
pub(crate) mod windowing;
pub use array_kind::ArrayKind;
pub use filter_summary::{ChromatogramSummary, SpectrumSummary};
pub use range::{ByteRange, Range};
pub(crate) mod decoder;
pub(crate) mod format;
pub(crate) mod scan;
pub use scan::{ScanSummary, TimeUnit};
#[cfg(test)]
pub(crate) use scan::ScanSource;
pub(crate) mod version_generated;
pub(crate) use decoder::utilities;
#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub(crate) use decoder::utilities::byte_source::FileSource;
pub use decoder::decode::{DataXY, IonReader, ReadOptions, ScanQuery, Select, Window, header_ranges, merge_ranges};
pub use decoder::utilities::{
    byte_source::{CallbackSource, ReadBytes},
    decompression_limit::{DEFAULT_MAX_UNCOMPRESSED_SIZE, DecompressionLimit},
};
pub(crate) use decoder::utilities::byte_source::{BytesSource, SourceBytes};
pub use header::{HEADER_FORMAT_VERSION_OFFSET, set_version, version_of};
pub(crate) use header::get_total_file_size_from_header;
pub(crate) mod encoder;
#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub(crate) use encoder::utilities::FileWriter;
pub use encoder::encode::{DEFAULT_MZ_WINDOW, WriteOptions};
pub(crate) use encoder::ion_writer::IonWriter;
pub(crate) use encoder::scan_stream::MemoryReader;
pub use encoder::scan_stream::ScanStream;
pub use encoder::utilities::{SectionStorage, WriteBytes};
pub use format::{
    CODEC_NONE, CODEC_ZSTD, CURRENT_VERSION, FILE_SIGNATURE, FILE_TRAILER, HEADER_SIZE,
    MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, allow_version, is_supported,
};
pub(crate) mod attr_meta;
pub(crate) mod error;
pub use error::{IonError, IonResult};
