#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use std::path::Path;

use crate::{
    ion::{
        IonResult,
        encoder::{ion_writer::IonWriter as LowWriter, scan_stream::ScanStream, utilities::WriteBytes},
    },
    mzml::structs::{Chromatogram, MzML, Spectrum},
};

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use crate::ion::encoder::utilities::FileWriter;

use super::options::WriteOptions;

enum Sink<'out> {
    Borrowed(&'out mut dyn WriteBytes),
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    Owned(FileWriter),
}

impl WriteBytes for Sink<'_> {
    fn write(&mut self, bytes: &[u8]) -> IonResult<()> {
        match self {
            Sink::Borrowed(output) => output.write(bytes),
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            Sink::Owned(file) => file.write(bytes),
        }
    }

    fn patch(&mut self, at: u64, bytes: &[u8]) -> IonResult<()> {
        match self {
            Sink::Borrowed(output) => output.patch(at, bytes),
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            Sink::Owned(file) => file.patch(at, bytes),
        }
    }

    fn position(&mut self) -> IonResult<u64> {
        match self {
            Sink::Borrowed(output) => output.position(),
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            Sink::Owned(file) => file.position(),
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
    sink: Sink<'out>,
    low: LowWriter,
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
        let mut sink = Sink::Owned(file);
        let low = LowWriter::begin(&mut sink, metadata, options)?;
        Ok(Self {
            sink,
            low,
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
        let mut sink = Sink::Borrowed(output);
        let low = LowWriter::begin(&mut sink, metadata, options)?;
        Ok(Self {
            sink,
            low,
            metadata: strip_lists(metadata),
        })
    }

    pub fn write_spectrum(&mut self, spectrum: &Spectrum) -> IonResult<()> {
        self.low.push_spectrum(&mut self.sink, spectrum)
    }

    pub fn write_chromatogram(&mut self, chromatogram: &Chromatogram) -> IonResult<()> {
        self.low.push_chromatogram(&mut self.sink, chromatogram)
    }

    pub fn write_stream(&mut self, scans: &mut dyn ScanStream) -> IonResult<()> {
        self.low.write_stream(&mut self.sink, scans)
    }

    pub fn finish(mut self) -> IonResult<()> {
        self.low.finish(&mut self.sink, &self.metadata)?;

        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        if let Sink::Owned(file) = &mut self.sink {
            file.flush()?;
        }

        Ok(())
    }
}
