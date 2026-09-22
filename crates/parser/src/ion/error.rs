use std::{
    error::Error,
    fmt::{Display, Formatter},
};

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IonError {
    Msg(String),
    BadDtype { dtype: u8, kind: &'static str },
    UnsupportedPacking(u8),
    UnsupportedFormatVersion(u16),
    UnsupportedCodec(u8),
    MissingSpectrumBounds,
    BadSpectrumBoundsChecksum,
    MalformedSpectrumBounds(String),
    MissingChromatogramBounds,
    BadChromatogramBoundsChecksum,
    MalformedChromatogramBounds(String),
    OutOfRange { index: usize, count: u64 },
}

pub type IonResult<T> = Result<T, IonError>;

impl IonError {
    pub fn contains(&self, text: &str) -> bool {
        self.to_string().contains(text)
    }
}

impl Display for IonError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Msg(text) => f.write_str(text),
            Self::BadDtype { dtype, kind } => {
                write!(f, "unsupported dtype {dtype} for {kind}")
            }
            Self::UnsupportedPacking(b) => write!(f, "unsupported packing id: {b}"),
            Self::UnsupportedFormatVersion(v) => write!(f, "unsupported format version: {v}"),
            Self::UnsupportedCodec(c) => write!(f, "unsupported compression codec: {c}"),
            Self::MissingSpectrumBounds => f.write_str(
                "spectrum has no window bounds (A0); re-encode the file with the current Ionic",
            ),
            Self::BadSpectrumBoundsChecksum => f.write_str("spectrum bounds (A0) checksum failed"),
            Self::MalformedSpectrumBounds(reason) => {
                write!(f, "spectrum bounds (A0) malformed: {reason}")
            }
            Self::MissingChromatogramBounds => f.write_str(
                "chromatogram has no window bounds (A0); re-encode the file with the current Ionic",
            ),
            Self::BadChromatogramBoundsChecksum => {
                f.write_str("chromatogram bounds (A0) checksum failed")
            }
            Self::MalformedChromatogramBounds(reason) => {
                write!(f, "chromatogram bounds (A0) malformed: {reason}")
            }
            Self::OutOfRange { index, count } => {
                write!(f, "index {index} out of range (count is {count})")
            }
        }
    }
}

impl Error for IonError {}

impl From<String> for IonError {
    fn from(text: String) -> Self {
        Self::Msg(text)
    }
}

impl From<&str> for IonError {
    fn from(text: &str) -> Self {
        Self::Msg(text.to_owned())
    }
}

impl From<std::io::Error> for IonError {
    fn from(err: std::io::Error) -> Self {
        Self::Msg(err.to_string())
    }
}

impl From<crate::mzml::ParseError> for IonError {
    fn from(source: crate::mzml::ParseError) -> Self {
        Self::Msg(source.to_string())
    }
}

impl From<crate::mzml::BinToMzmlError> for IonError {
    fn from(source: crate::mzml::BinToMzmlError) -> Self {
        Self::Msg(source.to_string())
    }
}
