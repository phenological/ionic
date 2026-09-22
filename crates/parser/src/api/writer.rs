#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use std::path::Path;

use crate::{
    ion::{
        IonResult,
        encoder::{ion_writer::IonEncoder, scan_stream::ScanStream, utilities::WriteBytes},
    },
    mzml::structs::{Chromatogram, MzML, Spectrum},
};

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use crate::ion::encoder::utilities::FileWriter;

use super::options::WriteOptions;

enum Output<'out> {
    Borrowed(&'out mut dyn WriteBytes),
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    Owned(FileWriter),
}

impl WriteBytes for Output<'_> {
    fn write(&mut self, bytes: &[u8]) -> IonResult<()> {
        match self {
            Output::Borrowed(output) => output.write(bytes),
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            Output::Owned(file) => file.write(bytes),
        }
    }

    fn patch(&mut self, at: u64, bytes: &[u8]) -> IonResult<()> {
        match self {
            Output::Borrowed(output) => output.patch(at, bytes),
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            Output::Owned(file) => file.patch(at, bytes),
        }
    }

    fn position(&mut self) -> IonResult<u64> {
        match self {
            Output::Borrowed(output) => output.position(),
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            Output::Owned(file) => file.position(),
        }
    }
}

fn strip_lists(metadata: &MzML) -> MzML {
    let mut metadata = metadata.clone();
    metadata.run.spectrum_list = None;
    metadata.run.chromatogram_list = None;
    metadata
}

pub struct IonWriter<'out> {
    output: Output<'out>,
    encoder: IonEncoder,
    metadata: MzML,
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl IonWriter<'static> {
    pub fn create(
        path: impl AsRef<Path>,
        metadata: &MzML,
        options: &WriteOptions,
    ) -> IonResult<Self> {
        let file = FileWriter::open_path(path.as_ref())?;
        let mut output = Output::Owned(file);
        let encoder = IonEncoder::begin(&mut output, metadata, options)?;
        Ok(Self {
            output,
            encoder,
            metadata: strip_lists(metadata),
        })
    }
}

impl<'out> IonWriter<'out> {
    pub fn to(
        output: &'out mut dyn WriteBytes,
        metadata: &MzML,
        options: &WriteOptions,
    ) -> IonResult<Self> {
        let mut output = Output::Borrowed(output);
        let encoder = IonEncoder::begin(&mut output, metadata, options)?;
        Ok(Self {
            output,
            encoder,
            metadata: strip_lists(metadata),
        })
    }

    pub fn write_spectrum(&mut self, spectrum: &Spectrum) -> IonResult<()> {
        self.encoder.push_spectrum(&mut self.output, spectrum)
    }

    pub fn write_chromatogram(&mut self, chromatogram: &Chromatogram) -> IonResult<()> {
        self.encoder.push_chromatogram(&mut self.output, chromatogram)
    }

    pub fn write_stream(mut self, scans: &mut dyn ScanStream) -> IonResult<()> {
        self.encoder.write_stream(&mut self.output, scans)?;
        self.flush_file()
    }

    pub fn finish(mut self) -> IonResult<()> {
        self.encoder.finish(&mut self.output, &self.metadata)?;
        self.flush_file()
    }

    fn flush_file(&mut self) -> IonResult<()> {
        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        if let Output::Owned(file) = &mut self.output {
            file.flush()?;
        }

        Ok(())
    }
}
