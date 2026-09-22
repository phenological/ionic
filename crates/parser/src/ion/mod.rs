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
pub(crate) mod version_generated;
pub(crate) use decoder::utilities;
pub use decoder::decode::{DataXY, Select, Window, header_ranges, merge_ranges};
pub use decoder::utilities::{
    byte_source::{CallbackSource, ReadBytes},
    decompression_limit::DecompressionLimit,
};
pub use header::{HEADER_FORMAT_VERSION_OFFSET, set_version, version_of};
pub(crate) mod encoder;
pub use encoder::encode::DEFAULT_MZ_WINDOW;
pub use encoder::scan_stream::ScanStream;
pub use encoder::utilities::{SectionStorage, WriteBytes};
pub use format::{
    CODEC_NONE, CODEC_ZSTD, CURRENT_VERSION, FILE_SIGNATURE, FILE_TRAILER, HEADER_SIZE,
    MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, allow_version, is_supported,
};
pub(crate) mod attr_meta;
pub(crate) mod error;
pub use error::{IonError, IonResult};
