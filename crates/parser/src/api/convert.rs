use crate::{
    ion::{IonError, IonResult, encoder::ion_writer::write_mzml_to_ion, format::FILE_SIGNATURE},
    mzml::{bin_to_mzml, parse_mzml},
};

use super::options::{ConvertKind, ConvertOptions};

fn resolve_kind(bytes: &[u8], kind: ConvertKind) -> ConvertKind {
    if kind != ConvertKind::Auto {
        return kind;
    }
    if bytes.starts_with(&FILE_SIGNATURE) {
        ConvertKind::IonToMzml
    } else {
        ConvertKind::MzmlToIon
    }
}

pub fn convert(input: &[u8], options: &ConvertOptions) -> IonResult<Vec<u8>> {
    if resolve_kind(input, options.kind) == ConvertKind::IonToMzml {
        let mut reader = crate::ion::decoder::decode::IonReader::from_bytes(input, &options.read)?;
        let mzml = reader.to_mzml()?;
        return Ok(bin_to_mzml(&mzml)?);
    }
    if input.starts_with(&FILE_SIGNATURE) {
        return Err(IonError::from("input is already an ion file"));
    }
    let mzml = parse_mzml(input)?;
    let mut output = Vec::new();
    write_mzml_to_ion(&mzml, &options.write, &mut output)?;
    Ok(output)
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub fn convert_file(
    input: impl AsRef<std::path::Path>,
    output: impl AsRef<std::path::Path>,
    options: &ConvertOptions,
) -> IonResult<()> {
    use std::io::Read;

    use crate::{
        ion::{encoder::ion_writer::IonWriter, encoder::utilities::FileWriter},
        mzml::{MzmlReader, structs::MzML},
    };

    let input = input.as_ref();
    let output = output.as_ref();

    let start = {
        let mut start = Vec::with_capacity(FILE_SIGNATURE.len());
        std::fs::File::open(input)?
            .take(FILE_SIGNATURE.len() as u64)
            .read_to_end(&mut start)?;
        start
    };
    let is_ion = start.starts_with(&FILE_SIGNATURE);

    let kind = if options.kind != ConvertKind::Auto {
        options.kind
    } else if let Some(kind) = kind_from_extension(input) {
        kind
    } else if is_ion {
        ConvertKind::IonToMzml
    } else {
        ConvertKind::MzmlToIon
    };

    if kind == ConvertKind::IonToMzml {
        let mut reader = crate::ion::decoder::decode::IonReader::open(input, &options.read)?;
        let mzml = reader.to_mzml()?;
        let xml = bin_to_mzml(&mzml)?;
        std::fs::write(output, &xml)?;
        return Ok(());
    }

    if is_ion {
        return Err(IonError::from("input is already an ion file"));
    }

    let mut file = FileWriter::open_path(output)?;
    let mut writer = IonWriter::begin(&mut file, &MzML::default(), &options.write)?;
    writer.write_stream(&mut file, &mut MzmlReader::open(input)?)?;
    file.flush()?;
    Ok(())
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
fn kind_from_extension(path: &std::path::Path) -> Option<ConvertKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "mzml" => Some(ConvertKind::MzmlToIon),
        "ion" => Some(ConvertKind::IonToMzml),
        _ => None,
    }
}
