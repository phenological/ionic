#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use cosmoz::{CompressOptions, Compressor};

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use crate::ion::encoder::encode::get_supported_compression_level;
use crate::ion::{IonError, IonResult};

pub trait WriteBytes {
    fn write(&mut self, bytes: &[u8]) -> IonResult<()>;
    fn patch(&mut self, at: u64, bytes: &[u8]) -> IonResult<()>;
    fn position(&mut self) -> IonResult<u64>;
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub struct FileWriter {
    writer: BufWriter<File>,
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl FileWriter {
    #[allow(dead_code)]
    pub fn open(path: &str) -> IonResult<Self> {
        Self::open_path(Path::new(path))
    }

    pub fn open_path(path: &Path) -> IonResult<Self> {
        let file = File::create(path).map_err(|err| {
            IonError::from(format!(
                "cannot create output file '{}': {err}",
                path.display()
            ))
        })?;
        Ok(Self {
            writer: BufWriter::with_capacity(8 * 1024 * 1024, file),
        })
    }

    pub fn flush(&mut self) -> IonResult<()> {
        use std::io::Write as StdWrite;
        self.writer
            .flush()
            .map_err(|err| IonError::from(format!("flush error: {err}")))
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl WriteBytes for FileWriter {
    fn write(&mut self, bytes: &[u8]) -> IonResult<()> {
        std::io::Write::write_all(&mut self.writer, bytes)
            .map_err(|err| IonError::from(format!("write error: {err}")))
    }

    fn patch(&mut self, at: u64, bytes: &[u8]) -> IonResult<()> {
        use std::io::Write as StdWrite;
        self.writer
            .flush()
            .map_err(|err| IonError::from(format!("flush error: {err}")))?;
        let resume_position = self
            .writer
            .stream_position()
            .map_err(|err| IonError::from(format!("position error: {err}")))?;
        self.writer
            .seek(SeekFrom::Start(at))
            .map_err(|err| IonError::from(format!("seek error: {err}")))?;
        std::io::Write::write_all(&mut self.writer, bytes)
            .map_err(|err| IonError::from(format!("patch write error: {err}")))?;
        self.writer
            .seek(SeekFrom::Start(resume_position))
            .map_err(|err| IonError::from(format!("seek-resume error: {err}")))?;
        Ok(())
    }

    fn position(&mut self) -> IonResult<u64> {
        use std::io::Write as StdWrite;
        self.writer
            .flush()
            .map_err(|err| IonError::from(format!("flush error: {err}")))?;
        self.writer
            .stream_position()
            .map_err(|err| IonError::from(format!("position error: {err}")))
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
static NEXT_SECTION_ID: AtomicU64 = AtomicU64::new(0);

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub(crate) struct SpilledSection {
    packer: Option<Compressor<BufWriter<File>>>,
    path: PathBuf,
    len: u64,
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl Drop for SpilledSection {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub(crate) struct RawSpill {
    file: File,
    path: PathBuf,
    len: u64,
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl Drop for RawSpill {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl RawSpill {
    pub(crate) fn create() -> IonResult<Self> {
        let path = std::env::temp_dir().join(format!(
            ".ionic-blocks.{}.{}",
            std::process::id(),
            NEXT_SECTION_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|err| IonError::from(format!("cannot create '{}': {err}", path.display())))?;
        Ok(Self { file, path, len: 0 })
    }

    pub(crate) fn append(&mut self, bytes: &[u8]) -> IonResult<u64> {
        let start = self.len;
        std::io::Write::write_all(&mut self.file, bytes).map_err(|err| {
            IonError::from(format!(
                "cannot stage blocks in '{}': {err}",
                self.path.display()
            ))
        })?;
        self.len += bytes.len() as u64;
        Ok(start)
    }

    pub(crate) fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> IonResult<()> {
        self.file.seek(SeekFrom::Start(offset)).map_err(|err| {
            IonError::from(format!(
                "cannot seek staged blocks in '{}': {err}",
                self.path.display()
            ))
        })?;
        self.file.read_exact(buffer).map_err(|err| {
            IonError::from(format!(
                "cannot read staged blocks from '{}': {err}",
                self.path.display()
            ))
        })
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionStorage {
    Memory,
    Disk,
}

pub(crate) enum SectionChunk {
    Memory(Vec<u8>),
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    Spilled(SpilledSection),
}

impl SectionChunk {
    pub(crate) fn memory(capacity: usize) -> Self {
        SectionChunk::Memory(Vec::with_capacity(capacity))
    }

    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    pub(crate) fn spilled(level: u8) -> IonResult<Self> {
        let path = std::env::temp_dir().join(format!(
            ".ionic-section.{}.{}",
            std::process::id(),
            NEXT_SECTION_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|err| IonError::from(format!("cannot create '{}': {err}", path.display())))?;
        let packer = Compressor::write_to(
            BufWriter::with_capacity(1 << 20, file),
            &CompressOptions {
                level: get_supported_compression_level(level),
                ..Default::default()
            },
        )
        .map_err(|err| IonError::from(format!("zstd start error: {err}")))?;
        Ok(SectionChunk::Spilled(SpilledSection {
            packer: Some(packer),
            path,
            len: 0,
        }))
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> IonResult<()> {
        match self {
            SectionChunk::Memory(buffer) => buffer.extend_from_slice(bytes),
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            SectionChunk::Spilled(section) => {
                let packer = section
                    .packer
                    .as_mut()
                    .ok_or_else(|| IonError::from("section is already closed"))?;
                std::io::Write::write_all(packer, bytes)
                    .map_err(|err| IonError::from(format!("section write error: {err}")))?;
                section.len += bytes.len() as u64;
            }
        }
        Ok(())
    }

    pub(crate) fn len(&self) -> u64 {
        match self {
            SectionChunk::Memory(buffer) => buffer.len() as u64,
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            SectionChunk::Spilled(section) => section.len,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn is_spilled(&self) -> bool {
        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        return matches!(self, SectionChunk::Spilled(_));
        #[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
        return false;
    }

    #[cfg(test)]
    pub(crate) fn as_slice(&self) -> &[u8] {
        match self {
            SectionChunk::Memory(buffer) => buffer,
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            SectionChunk::Spilled(_) => &[],
        }
    }

    pub(crate) fn into_vec(self) -> IonResult<Vec<u8>> {
        match self {
            SectionChunk::Memory(buffer) => Ok(buffer),
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            SectionChunk::Spilled(_) => Err(IonError::from(
                "a spilled section cannot be read into memory",
            )),
        }
    }

    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    pub(crate) fn copy_into(self, output: &mut dyn WriteBytes) -> IonResult<(u64, u64, u32)> {
        let SectionChunk::Spilled(mut section) = self else {
            return Err(IonError::from("copy_into needs a spilled section"));
        };
        if section.len == 0 {
            let start = pad_to_alignment(output)?;
            return Ok((start, 0, crc32fast::hash(&[])));
        }
        let mut file = section
            .packer
            .take()
            .ok_or_else(|| IonError::from("section is already closed"))?
            .finish()
            .map_err(|err| IonError::from(format!("zstd finish error: {err}")))?
            .into_inner()
            .map_err(|err| IonError::from(format!("section flush error: {err}")))?;
        file.seek(SeekFrom::Start(0))
            .map_err(|err| IonError::from(format!("section seek error: {err}")))?;

        let start = pad_to_alignment(output)?;
        let mut hasher = crc32fast::Hasher::new();
        let mut stored = 0u64;
        let mut buffer = vec![0u8; 1 << 20];
        loop {
            let filled = file
                .read(&mut buffer)
                .map_err(|err| IonError::from(format!("section read error: {err}")))?;
            if filled == 0 {
                break;
            }
            hasher.update(&buffer[..filled]);
            output.write(&buffer[..filled])?;
            stored += filled as u64;
        }
        Ok((start, stored, hasher.finalize()))
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
fn pad_to_alignment(output: &mut dyn WriteBytes) -> IonResult<u64> {
    static PAD: [u8; 7] = [0u8; 7];
    let position = output.position()?;
    let aligned = (position + 7) & !7;
    if aligned > position {
        output.write(&PAD[..(aligned - position) as usize])?;
    }
    Ok(aligned)
}

#[cfg(all(test, not(all(target_arch = "wasm32", not(target_os = "wasi")))))]
mod tests {
    use cosmoz::{DecompressOptions, Decoder};

    use super::*;

    fn row(seed: u64) -> [u8; 64] {
        let mut record = [0u8; 64];
        for (slot, part) in record.chunks_mut(8).enumerate() {
            part.copy_from_slice(&(seed + slot as u64).to_le_bytes());
        }
        record
    }

    #[test]
    fn raw_spill_roundtrips_appended_blocks() {
        let mut spill = RawSpill::create().unwrap();
        let staged_path = spill.path().to_path_buf();
        assert!(staged_path.exists());

        let blocks: Vec<Vec<u8>> = (0..8u8)
            .map(|seed| vec![seed; 100 + seed as usize * 37])
            .collect();
        let mut offsets = Vec::new();
        for block in &blocks {
            offsets.push(spill.append(block).unwrap());
        }

        let mut running = 0u64;
        for (block, offset) in blocks.iter().zip(&offsets) {
            assert_eq!(*offset, running);
            running += block.len() as u64;
        }

        for (block, offset) in blocks.iter().zip(&offsets).rev() {
            let mut read_back = vec![0u8; block.len()];
            spill.read_at(*offset, &mut read_back).unwrap();
            assert_eq!(&read_back, block);
        }

        drop(spill);
        assert!(
            !staged_path.exists(),
            "the temp file must be deleted on drop"
        );
    }

    #[test]
    fn a_memory_section_never_touches_disk() {
        let mut chunk = SectionChunk::memory(0);
        chunk.write(&row(1)).unwrap();
        assert!(!chunk.is_spilled());
        assert_eq!(chunk.len(), 64);
    }

    #[test]
    fn a_disk_section_keeps_every_byte() {
        let mut chunk = SectionChunk::spilled(3).unwrap();
        let rows = 64 * 1024;
        let mut written = Vec::with_capacity(rows * 64);
        for seed in 0..rows as u64 {
            let record = row(seed);
            chunk.write(&record).unwrap();
            written.extend_from_slice(&record);
        }

        assert!(chunk.is_spilled());
        assert_eq!(chunk.len(), written.len() as u64);

        let mut output: Vec<u8> = Vec::new();
        let (offset, stored, crc32) = chunk.copy_into(&mut output).unwrap();
        assert_eq!(offset, 0);
        assert_eq!(stored, output.len() as u64);
        assert!(stored < written.len() as u64);
        assert_eq!(crc32, crc32fast::hash(&output));

        let mut restored = vec![0u8; written.len()];
        let mut decoder = Decoder::new(&DecompressOptions::default());
        let restored_length = decoder.decompress_into(&output, &mut restored).unwrap();
        assert_eq!(restored_length, written.len());
        assert_eq!(restored, written);
    }

    #[test]
    fn two_sections_never_share_a_file() {
        let first = SectionChunk::spilled(3).unwrap();
        let second = SectionChunk::spilled(3).unwrap();
        let (SectionChunk::Spilled(first), SectionChunk::Spilled(second)) = (first, second) else {
            panic!("both must be spilled");
        };
        assert_ne!(first.path, second.path);
    }
}

impl WriteBytes for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) -> IonResult<()> {
        self.extend_from_slice(bytes);
        Ok(())
    }

    fn patch(&mut self, at: u64, bytes: &[u8]) -> IonResult<()> {
        let start = at as usize;
        let end = start + bytes.len();
        self.get_mut(start..end)
            .ok_or_else(|| IonError::from(format!("patch: range {start}..{end} out of bounds")))?
            .copy_from_slice(bytes);
        Ok(())
    }

    fn position(&mut self) -> IonResult<u64> {
        Ok(self.len() as u64)
    }
}
