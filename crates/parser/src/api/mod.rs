mod convert;
mod options;
mod reader;
mod writer;

pub use convert::convert;
#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub use convert::convert_file;
pub use options::{ConvertKind, ConvertOptions, ReadOptions, WriteOptions};
pub use reader::{IonReader, ScanQuery};
pub use writer::IonWriter;

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use crate::{ion::IonResult, mzml::structs::MzML};

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub fn read(path: impl AsRef<std::path::Path>) -> IonResult<MzML> {
    IonReader::open(path, &ReadOptions::default())?.to_mzml()
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub fn write(
    path: impl AsRef<std::path::Path>,
    mzml: &MzML,
    options: &WriteOptions,
) -> IonResult<()> {
    use crate::ion::encoder::{ion_writer::write_mzml_to_ion, utilities::FileWriter};

    let mut file = FileWriter::open_path(path.as_ref())?;
    write_mzml_to_ion(mzml, options, &mut file)?;
    file.flush()
}
