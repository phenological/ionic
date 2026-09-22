pub use crate::ion::decoder::decode::ReadOptions;
pub use crate::ion::encoder::encode::WriteOptions;

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConvertKind {
    #[default]
    Auto,
    MzmlToIon,
    IonToMzml,
}

#[derive(Debug, Clone, Default)]
pub struct ConvertOptions {
    pub kind: ConvertKind,
    pub read: ReadOptions,
    pub write: WriteOptions,
}
