use std::{fs, path::PathBuf};
#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use std::{fs::File, io::Read, path::Path};

use crate::{
    ion::{
        FILE_SIGNATURE, IonError, IonReader, IonResult, ReadOptions, WriteBytes, WriteOptions,
        write_mzml_to_ion,
    },
    mzml::{bin_to_mzml, parse_mzml},
};
#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use crate::{
    ion::{FileWriter, IonWriter},
    mzml::MzmlReader,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConvertKind {
    #[default]
    Auto,
    MzmlToIon,
    IonToMzml,
}

pub enum Input<'a> {
    Bytes(&'a [u8]),
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    Path(PathBuf),
}

#[derive(Debug, Clone, Default)]
pub struct ConvertOptions {
    pub output: Option<PathBuf>,
    pub kind: ConvertKind,
    pub read: ReadOptions,
    pub write: WriteOptions,
}

impl<'a> From<&'a [u8]> for Input<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::Bytes(bytes)
    }
}

impl<'a> From<&'a Vec<u8>> for Input<'a> {
    fn from(bytes: &'a Vec<u8>) -> Self {
        Self::Bytes(bytes)
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl From<&Path> for Input<'_> {
    fn from(path: &Path) -> Self {
        Self::Path(path.to_path_buf())
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl From<PathBuf> for Input<'_> {
    fn from(path: PathBuf) -> Self {
        Self::Path(path)
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl From<&PathBuf> for Input<'_> {
    fn from(path: &PathBuf) -> Self {
        Self::Path(path.clone())
    }
}

pub fn convert<'a>(
    input: impl Into<Input<'a>>,
    options: ConvertOptions,
) -> IonResult<Option<Vec<u8>>> {
    let input = input.into();
    if get_kind(&input, options.kind)? == ConvertKind::IonToMzml {
        return ion_to_mzml(&input, options);
    }
    mzml_to_ion(&input, options)
}

fn get_kind(input: &Input<'_>, kind: ConvertKind) -> IonResult<ConvertKind> {
    if kind != ConvertKind::Auto {
        return Ok(kind);
    }
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    if let Input::Path(path) = input
        && let Some(kind) = kind_from_extension(path)
    {
        return Ok(kind);
    }
    Ok(if input_is_ion(input)? {
        ConvertKind::IonToMzml
    } else {
        ConvertKind::MzmlToIon
    })
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
fn kind_from_extension(path: &Path) -> Option<ConvertKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "mzml" => Some(ConvertKind::MzmlToIon),
        "ion" => Some(ConvertKind::IonToMzml),
        _ => None,
    }
}

fn input_is_ion(input: &Input<'_>) -> IonResult<bool> {
    match input {
        Input::Bytes(bytes) => Ok(bytes.starts_with(&FILE_SIGNATURE)),
        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        Input::Path(path) => Ok(read_file_start(path)?.starts_with(&FILE_SIGNATURE)),
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
fn read_file_start(path: &Path) -> IonResult<Vec<u8>> {
    let file = File::open(path).map_err(|error| {
        IonError::from(format!(
            "cannot open input file '{}': {error}",
            path.display()
        ))
    })?;
    let mut start = Vec::with_capacity(FILE_SIGNATURE.len());
    file.take(FILE_SIGNATURE.len() as u64)
        .read_to_end(&mut start)?;
    Ok(start)
}

fn mzml_to_ion(input: &Input<'_>, options: ConvertOptions) -> IonResult<Option<Vec<u8>>> {
    if input_is_ion(input)? {
        return Err(IonError::from("input is already an ion file"));
    }
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    let write = options.write;
    #[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
    let write = WriteOptions {
        compression_level: 1,
        ..options.write
    };
    match options.output {
        None => {
            let mut bytes = Vec::new();
            write_ion(input, write, &mut bytes)?;
            Ok(Some(bytes))
        }
        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        Some(path) => {
            let mut file = FileWriter::open_path(&path)?;
            write_ion(input, write, &mut file)?;
            file.flush()?;
            Ok(None)
        }
        #[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
        Some(_) => Err(IonError::from(
            "file output is not available in browser wasm",
        )),
    }
}

fn write_ion(input: &Input<'_>, write: WriteOptions, output: &mut dyn WriteBytes) -> IonResult<()> {
    match input {
        Input::Bytes(bytes) => write_mzml_to_ion(&parse_mzml(bytes)?, write, output),
        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        Input::Path(path) => {
            IonWriter::create(output, write)?.write_stream(&mut MzmlReader::open(path)?)
        }
    }
}

fn ion_to_mzml(input: &Input<'_>, options: ConvertOptions) -> IonResult<Option<Vec<u8>>> {
    let mzml = open_ion(input, options.read)?.to_mzml()?;
    let xml = bin_to_mzml(&mzml)?;
    drop(mzml);
    let Some(path) = options.output else {
        return Ok(Some(xml));
    };
    fs::write(path, &xml)?;
    Ok(None)
}

fn open_ion(input: &Input<'_>, read: ReadOptions) -> IonResult<IonReader> {
    match input {
        Input::Bytes(bytes) => IonReader::open(bytes, read),
        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        Input::Path(path) => IonReader::open_file(path, read),
    }
}
