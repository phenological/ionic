use crate::{
    ion::{IonError, IonResult, encoder::ion_writer::write_mzml_to_ion, format::FILE_SIGNATURE},
    mzml::{bin_to_mzml, parse_mzml},
};

use super::{
    options::{ConvertKind, ConvertOptions},
    reader::IonReader,
};

fn resolve_kind(wanted: ConvertKind, from_extension: Option<ConvertKind>, is_ion: bool) -> ConvertKind {
    if wanted != ConvertKind::Auto {
        return wanted;
    }
    if let Some(kind) = from_extension {
        return kind;
    }
    if is_ion {
        ConvertKind::IonToMzml
    } else {
        ConvertKind::MzmlToIon
    }
}

pub fn convert(input: &[u8], options: &ConvertOptions) -> IonResult<Vec<u8>> {
    let is_ion = input.starts_with(&FILE_SIGNATURE);
    if resolve_kind(options.kind, None, is_ion) == ConvertKind::IonToMzml {
        let mut reader = IonReader::from_bytes(input, &options.read)?;
        let mzml = reader.to_mzml()?;
        return Ok(bin_to_mzml(&mzml)?);
    }
    if is_ion {
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

    use crate::mzml::{MzmlReader, structs::MzML};
    use super::writer::IonWriter;

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

    let kind = resolve_kind(options.kind, kind_from_extension(input), is_ion);

    if kind == ConvertKind::IonToMzml {
        let mut reader = IonReader::open(input, &options.read)?;
        let mzml = reader.to_mzml()?;
        let xml = bin_to_mzml(&mzml)?;
        std::fs::write(output, &xml)?;
        return Ok(());
    }

    if is_ion {
        return Err(IonError::from("input is already an ion file"));
    }

    IonWriter::create(output, &MzML::default(), &options.write)?
        .write_stream(&mut MzmlReader::open(input)?)?;
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
