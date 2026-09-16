use std::collections::HashMap;

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use rayon::prelude::*;
#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use zstd::{bulk::Compressor as ZstdCompressor, zstd_safe::compress_bound};

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use crate::ion::encoder::utilities::output::RawSpill;
use crate::ion::{
    IonError, IonResult, byte_transpose::shuffle_with_tail, encoder::utilities::output::WriteBytes,
    packing::PackingId,
};

pub(crate) const BLOCK_DIRECTORY_ENTRY_SIZE: usize = 32;
const DEFAULT_MAX_PENDING_BYTES: usize = 64 * 1024 * 1024;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stride {
    OneByte = 1,
    TwoBytes = 2,
    FourBytes = 4,
    EightBytes = 8,
}

impl Stride {
    #[inline]
    pub(crate) fn from_size(element_size: usize) -> Self {
        match element_size {
            2 => Self::TwoBytes,
            4 => Self::FourBytes,
            8 => Self::EightBytes,
            _ => Self::OneByte,
        }
    }

    #[inline]
    pub(crate) fn as_usize(self) -> usize {
        self as usize
    }
}

pub(crate) trait BlockCompressor {
    fn compress(&mut self, input: &[u8], output: &mut Vec<u8>) -> IonResult<usize>;
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    fn fork(&self) -> IonResult<Self>
    where
        Self: Sized;
    fn shuffle_bytes_into(&self, input: &[u8], output: &mut [u8], element_stride: usize);
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub(crate) struct DefaultCompressor {
    level: i32,
    inner: ZstdCompressor<'static>,
}

#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
pub(crate) struct DefaultCompressor;

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl DefaultCompressor {
    pub(crate) fn new(compression_level: i32) -> IonResult<Self> {
        Ok(Self {
            level: compression_level,
            inner: ZstdCompressor::new(compression_level)
                .map_err(|err| IonError::from(err.to_string()))?,
        })
    }
}

#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
impl DefaultCompressor {
    pub(crate) fn new(_compression_level: i32) -> IonResult<Self> {
        Ok(Self)
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
impl BlockCompressor for DefaultCompressor {
    fn compress(&mut self, input: &[u8], output: &mut Vec<u8>) -> IonResult<usize> {
        output.clear();
        output.reserve(compress_bound(input.len()));
        self.inner
            .compress_to_buffer(input, output)
            .map_err(|err| IonError::from(err.to_string()))
    }

    fn fork(&self) -> IonResult<Self> {
        Self::new(self.level)
    }

    fn shuffle_bytes_into(&self, input: &[u8], output: &mut [u8], element_stride: usize) {
        shuffle_with_tail(input, output, element_stride);
    }
}

#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
impl BlockCompressor for DefaultCompressor {
    fn compress(&mut self, input: &[u8], output: &mut Vec<u8>) -> IonResult<usize> {
        let options = osmo::CompressOptions::zstd();
        output.clear();
        output.resize(osmo::get_max_compressed_size(input.len(), &options), 0);
        let mut workspace = osmo::EncodeWorkspace::new_boxed();
        let written = osmo::compress(input, output, &options, &mut workspace)
            .map_err(|err| IonError::from(format!("zstd encode failed: {err:?}")))?;
        output.truncate(written);
        Ok(written)
    }

    fn shuffle_bytes_into(&self, input: &[u8], output: &mut [u8], element_stride: usize) {
        shuffle_with_tail(input, output, element_stride);
    }
}

pub(crate) enum CompressionMode<C: BlockCompressor> {
    Raw,
    Compressed(C),
}

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct BlockDirEntry {
    pub(crate) payload_offset: u64,
    pub(crate) payload_size: u64,
    pub(crate) uncompressed_len_bytes: u64,
    pub(crate) checksum: u32,
}

fn block_id_from_count(count: usize) -> IonResult<u32> {
    u32::try_from(count)
        .map_err(|_| IonError::from("container: block count exceeds the u32 block id limit"))
}

impl BlockDirEntry {
    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.payload_offset.to_le_bytes());
        buffer.extend_from_slice(&self.payload_size.to_le_bytes());
        buffer.extend_from_slice(&self.uncompressed_len_bytes.to_le_bytes());
        buffer.extend_from_slice(&self.checksum.to_le_bytes());
        buffer.extend_from_slice(&0u32.to_le_bytes());
    }
}

struct BlockDirectory {
    entries: Vec<BlockDirEntry>,
}

impl BlockDirectory {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn reserve_next_block_id(&mut self) -> IonResult<u32> {
        let count = self.entries.len();
        let block_id = block_id_from_count(count)?;
        self.entries.push(BlockDirEntry::default());
        Ok(block_id)
    }

    fn seal_block(&mut self, block_id: u32, entry: BlockDirEntry) -> IonResult<()> {
        let slot = self
            .entries
            .get_mut(block_id as usize)
            .ok_or_else(|| IonError::from(format!("seal_block: unknown block_id={block_id}")))?;
        slot.clone_from(&entry);
        Ok(())
    }

    fn block_count(&self) -> u32 {
        self.entries.len() as u32
    }

    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        for entry in &self.entries {
            entry.write_to_buffer(buffer);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BlockGroup {
    array_type: u32,
    element_size: usize,
    window_index: u32,
}

impl BlockGroup {
    fn new(array_type: u32, element_size: usize, window_index: u32) -> Self {
        Self {
            array_type,
            element_size: element_size.max(1),
            window_index,
        }
    }

    fn stride(self) -> Stride {
        Stride::from_size(self.element_size)
    }
}

struct ActiveBlock {
    block_id: u32,
    stride: Stride,
    accumulated_data: Vec<u8>,
    compress_sequentially: bool,
}

struct OpenBlocks {
    by_group: HashMap<BlockGroup, ActiveBlock>,
}

impl OpenBlocks {
    fn new() -> Self {
        Self {
            by_group: HashMap::new(),
        }
    }

    fn get_mut(&mut self, group: BlockGroup) -> Option<&mut ActiveBlock> {
        self.by_group.get_mut(&group)
    }

    fn insert(&mut self, group: BlockGroup, block: ActiveBlock) {
        self.by_group.insert(group, block);
    }

    fn take(&mut self, group: BlockGroup) -> Option<ActiveBlock> {
        self.by_group.remove(&group)
    }

    fn is_open(&self, group: BlockGroup) -> bool {
        self.by_group.contains_key(&group)
    }

    fn byte_len(&self, group: BlockGroup) -> usize {
        self.by_group
            .get(&group)
            .map_or(0, |block| block.accumulated_data.len())
    }

    fn open_groups_in_id_order(&self) -> Vec<BlockGroup> {
        let mut groups: Vec<(u32, BlockGroup)> = self
            .by_group
            .iter()
            .map(|(group, block)| (block.block_id, *group))
            .collect();
        groups.sort_by_key(|(block_id, _)| *block_id);
        groups.into_iter().map(|(_, group)| group).collect()
    }
}

struct BlockStore {
    open_blocks: OpenBlocks,
    directory: BlockDirectory,
    max_block_size: usize,
}

impl BlockStore {
    fn new(max_block_size: usize) -> Self {
        Self {
            open_blocks: OpenBlocks::new(),
            directory: BlockDirectory::new(),
            max_block_size,
        }
    }

    fn would_overflow(&self, group: BlockGroup, additional_bytes: usize) -> bool {
        let current = self.open_blocks.byte_len(group);
        self.open_blocks.is_open(group)
            && current > 0
            && current + additional_bytes > self.max_block_size
    }

    fn ensure_open_block(&mut self, group: BlockGroup, capacity_hint: usize) -> IonResult<()> {
        if !self.open_blocks.is_open(group) {
            let block_id = self.directory.reserve_next_block_id()?;
            self.open_blocks.insert(
                group,
                ActiveBlock {
                    block_id,
                    stride: group.stride(),
                    accumulated_data: Vec::with_capacity(capacity_hint),
                    compress_sequentially: false,
                },
            );
        }
        Ok(())
    }

    fn append_to_block<W>(
        &mut self,
        group: BlockGroup,
        item_byte_size: usize,
        write_action: W,
    ) -> IonResult<(u32, u64)>
    where
        W: FnOnce(&mut Vec<u8>) -> IonResult<()>,
    {
        let active = self
            .open_blocks
            .get_mut(group)
            .expect("append_to_block: no open block for group");

        let block_id = active.block_id;
        let element_offset = (active.accumulated_data.len() / active.stride.as_usize()) as u64;
        active.accumulated_data.reserve(item_byte_size);
        write_action(&mut active.accumulated_data)?;
        Ok((block_id, element_offset))
    }

    fn open_isolated_block(
        &mut self,
        group: BlockGroup,
        capacity: usize,
        compress_sequentially: bool,
    ) -> IonResult<u32> {
        let block_id = self.directory.reserve_next_block_id()?;
        self.open_blocks.insert(
            group,
            ActiveBlock {
                block_id,
                stride: group.stride(),
                accumulated_data: Vec::with_capacity(capacity),
                compress_sequentially,
            },
        );
        Ok(block_id)
    }

    fn take_open_block(&mut self, group: BlockGroup) -> Option<ActiveBlock> {
        self.open_blocks.take(group)
    }

    fn seal(&mut self, block_id: u32, entry: BlockDirEntry) -> IonResult<()> {
        self.directory.seal_block(block_id, entry)
    }

    fn block_count(&self) -> u32 {
        self.directory.block_count()
    }

    fn write_directory(&self, buffer: &mut Vec<u8>) {
        self.directory.write_to_buffer(buffer);
    }
}

struct PendingBlock {
    block_id: u32,
    window_index: u32,
    stride: Stride,
    data: Vec<u8>,
    compress_sequentially: bool,
}

struct ReadyBlock {
    block_id: u32,
    window_index: u32,
    bytes: Vec<u8>,
    raw_len: u64,
    checksum: u32,
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
struct StagedKey {
    window_index: u32,
    block_id: u32,
    staged_offset: u64,
    staged_len: u64,
    raw_len: u64,
    checksum: u32,
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
struct Staging {
    spill: RawSpill,
    keys: Vec<StagedKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContainerSummary {
    pub(crate) block_count: u32,
    pub(crate) total_bytes: u64,
    pub(crate) directory_crc32: u32,
}

pub(crate) struct BlockWriter<'output, C: BlockCompressor> {
    output: &'output mut dyn WriteBytes,
    block_packing_id: PackingId,
    store: BlockStore,
    pending: Vec<PendingBlock>,
    pending_bytes: usize,
    payload_bytes: u64,
    max_pending_bytes: usize,
    compressor: CompressionMode<C>,
    par_min_blocks: usize,
    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    staging: Option<Staging>,
}

impl<'output, C: BlockCompressor + Send + Sync> BlockWriter<'output, C> {
    pub(crate) fn new(
        output: &'output mut dyn WriteBytes,
        max_block_uncompressed_size: usize,
        compressor: CompressionMode<C>,
        block_packing_id: PackingId,
    ) -> Self {
        Self {
            output,
            block_packing_id,
            store: BlockStore::new(max_block_uncompressed_size),
            pending: Vec::new(),
            pending_bytes: 0,
            payload_bytes: 0,
            max_pending_bytes: DEFAULT_MAX_PENDING_BYTES,
            compressor,
            par_min_blocks: 4,
            #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
            staging: None,
        }
    }

    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    pub(crate) fn window_major(mut self) -> IonResult<Self> {
        self.staging = Some(Staging {
            spill: RawSpill::create()?,
            keys: Vec::new(),
        });
        Ok(self)
    }

    pub(crate) fn force_sequential(mut self) -> Self {
        self.par_min_blocks = usize::MAX;
        self
    }

    pub(crate) fn add_item_to_box<WriteAction>(
        &mut self,
        array_type: u32,
        item_byte_size: usize,
        element_size: usize,
        write_action: WriteAction,
    ) -> IonResult<(u32, u64)>
    where
        WriteAction: FnOnce(&mut Vec<u8>) -> IonResult<()>,
    {
        self.add_item_to_window(array_type, 0, item_byte_size, element_size, write_action)
    }

    pub(crate) fn add_item_to_window<WriteAction>(
        &mut self,
        array_type: u32,
        window_index: u32,
        item_byte_size: usize,
        element_size: usize,
        write_action: WriteAction,
    ) -> IonResult<(u32, u64)>
    where
        WriteAction: FnOnce(&mut Vec<u8>) -> IonResult<()>,
    {
        let group = BlockGroup::new(array_type, element_size, window_index);
        if item_byte_size > self.store.max_block_size {
            self.add_isolated_block(group, item_byte_size, true, write_action)
        } else {
            self.add_normal_item(group, item_byte_size, write_action)
        }
    }

    fn add_isolated_block<WriteAction>(
        &mut self,
        group: BlockGroup,
        item_byte_size: usize,
        compress_sequentially: bool,
        write_action: WriteAction,
    ) -> IonResult<(u32, u64)>
    where
        WriteAction: FnOnce(&mut Vec<u8>) -> IonResult<()>,
    {
        self.seal_open_block_for_group(group)?;

        let block_id =
            self.store
                .open_isolated_block(group, item_byte_size, compress_sequentially)?;

        write_action(
            &mut self
                .store
                .open_blocks
                .get_mut(group)
                .expect("isolated block was just inserted")
                .accumulated_data,
        )?;

        self.seal_open_block_for_group(group)?;
        Ok((block_id, 0))
    }

    fn add_normal_item<WriteAction>(
        &mut self,
        group: BlockGroup,
        item_byte_size: usize,
        write_action: WriteAction,
    ) -> IonResult<(u32, u64)>
    where
        WriteAction: FnOnce(&mut Vec<u8>) -> IonResult<()>,
    {
        if self.store.would_overflow(group, item_byte_size) {
            self.seal_open_block_for_group(group)?;
        }

        self.store.ensure_open_block(group, item_byte_size)?;
        let (block_id, element_offset) =
            self.store
                .append_to_block(group, item_byte_size, write_action)?;

        Ok((block_id, element_offset))
    }

    fn seal_open_block_for_group(&mut self, group: BlockGroup) -> IonResult<()> {
        let Some(active_block) = self.store.take_open_block(group) else {
            return Ok(());
        };
        if active_block.accumulated_data.is_empty() {
            return Ok(());
        }
        let data_len = active_block.accumulated_data.len();
        self.pending.push(PendingBlock {
            block_id: active_block.block_id,
            window_index: group.window_index,
            stride: active_block.stride,
            data: active_block.accumulated_data,
            compress_sequentially: active_block.compress_sequentially,
        });
        self.pending_bytes += data_len;
        if self.pending_bytes >= self.max_pending_bytes {
            self.flush_pending()?;
        }
        Ok(())
    }

    fn make_block(
        block_packing_id: PackingId,
        block: PendingBlock,
        mode: &mut CompressionMode<C>,
    ) -> IonResult<ReadyBlock> {
        let raw_len = block.data.len() as u64;
        match mode {
            CompressionMode::Raw => Ok(ReadyBlock {
                block_id: block.block_id,
                window_index: block.window_index,
                checksum: crc32fast::hash(&block.data),
                raw_len,
                bytes: block.data,
            }),
            CompressionMode::Compressed(compressor) => {
                let data = if block_packing_id == PackingId::ByteShuffle
                    && block.stride != Stride::OneByte
                {
                    let mut shuffled = vec![0u8; block.data.len()];
                    compressor.shuffle_bytes_into(
                        &block.data,
                        &mut shuffled,
                        block.stride.as_usize(),
                    );
                    shuffled
                } else {
                    block.data
                };
                let mut bytes = Vec::new();
                compressor.compress(&data, &mut bytes)?;
                Ok(ReadyBlock {
                    block_id: block.block_id,
                    window_index: block.window_index,
                    checksum: crc32fast::hash(&bytes),
                    raw_len,
                    bytes,
                })
            }
        }
    }

    fn finish_seq(&mut self, blocks: Vec<PendingBlock>) -> IonResult<Vec<ReadyBlock>> {
        let mut out = Vec::with_capacity(blocks.len());
        for block in blocks {
            out.push(Self::make_block(
                self.block_packing_id,
                block,
                &mut self.compressor,
            )?);
        }
        Ok(out)
    }

    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    fn finish_par(&self, blocks: Vec<PendingBlock>) -> IonResult<Vec<ReadyBlock>> {
        match &self.compressor {
            CompressionMode::Raw => blocks
                .into_par_iter()
                .map(|block| {
                    Ok(ReadyBlock {
                        block_id: block.block_id,
                        window_index: block.window_index,
                        checksum: crc32fast::hash(&block.data),
                        raw_len: block.data.len() as u64,
                        bytes: block.data,
                    })
                })
                .collect(),
            CompressionMode::Compressed(compressor) => {
                let block_packing_id = self.block_packing_id;
                blocks
                    .into_par_iter()
                    .map_init(
                        || compressor.fork().map(CompressionMode::Compressed),
                        |worker, block| {
                            let mode = worker.as_mut().map_err(|error| error.clone())?;
                            Self::make_block(block_packing_id, block, mode)
                        },
                    )
                    .collect()
            }
        }
    }

    #[cfg(test)]
    fn set_par_min_blocks(&mut self, value: usize) {
        self.par_min_blocks = value;
    }

    #[cfg(test)]
    fn set_max_pending_bytes(&mut self, value: usize) {
        self.max_pending_bytes = value;
    }

    fn flush_pending(&mut self) -> IonResult<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let batch = std::mem::take(&mut self.pending);
        self.pending_bytes = 0;
        let (sequential, shared): (Vec<_>, Vec<_>) =
            batch.into_iter().partition(|b| b.compress_sequentially);

        let sequential_ready = self.finish_seq(sequential)?;

        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        let shared_ready = {
            let par_min_blocks = self.par_min_blocks;
            if shared.len() < par_min_blocks {
                self.finish_seq(shared)?
            } else {
                self.finish_par(shared)?
            }
        };
        #[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
        let shared_ready = self.finish_seq(shared)?;

        let mut ready = sequential_ready;
        ready.extend(shared_ready);
        ready.sort_unstable_by_key(|block| (block.window_index, block.block_id));

        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        if let Some(staging) = self.staging.as_mut() {
            for block in ready {
                let staged_offset = staging.spill.append(&block.bytes)?;
                staging.keys.push(StagedKey {
                    window_index: block.window_index,
                    block_id: block.block_id,
                    staged_offset,
                    staged_len: block.bytes.len() as u64,
                    raw_len: block.raw_len,
                    checksum: block.checksum,
                });
            }
            return Ok(());
        }

        for block in ready {
            self.output.write(&block.bytes)?;
            self.store.seal(
                block.block_id,
                BlockDirEntry {
                    payload_offset: self.payload_bytes,
                    payload_size: block.bytes.len() as u64,
                    uncompressed_len_bytes: block.raw_len,
                    checksum: block.checksum,
                    ..Default::default()
                },
            )?;
            self.payload_bytes += block.bytes.len() as u64;
        }
        Ok(())
    }

    #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
    fn write_staged_blocks_in_window_order(&mut self) -> IonResult<()> {
        let Some(mut staging) = self.staging.take() else {
            return Ok(());
        };
        staging
            .keys
            .sort_unstable_by_key(|key| (key.window_index, key.block_id));

        let mut buffer = Vec::new();
        for key in staging.keys {
            buffer.resize(key.staged_len as usize, 0);
            staging.spill.read_at(key.staged_offset, &mut buffer)?;
            self.output.write(&buffer)?;
            self.store.seal(
                key.block_id,
                BlockDirEntry {
                    payload_offset: self.payload_bytes,
                    payload_size: key.staged_len,
                    uncompressed_len_bytes: key.raw_len,
                    checksum: key.checksum,
                },
            )?;
            self.payload_bytes += key.staged_len;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self) -> IonResult<ContainerSummary> {
        for group in self.store.open_blocks.open_groups_in_id_order() {
            self.seal_open_block_for_group(group)?;
        }
        self.flush_pending()?;
        #[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
        self.write_staged_blocks_in_window_order()?;

        let block_count = self.store.block_count();
        let mut directory_bytes =
            Vec::with_capacity(block_count as usize * BLOCK_DIRECTORY_ENTRY_SIZE);
        self.store.write_directory(&mut directory_bytes);
        let directory_crc32 = crc32fast::hash(&directory_bytes);
        self.output.write(&directory_bytes)?;

        let total_bytes = self.payload_bytes + directory_bytes.len() as u64;
        Ok(ContainerSummary {
            block_count,
            total_bytes,
            directory_crc32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_group_keeps_array_type_and_element_size() {
        let mz = BlockGroup::new(1000514, 8, 0);
        let intensity = BlockGroup::new(1000515, 4, 0);
        assert_ne!(mz, intensity);
        assert_eq!(mz.stride(), Stride::EightBytes);
        assert_eq!(intensity.stride(), Stride::FourBytes);
    }

    #[test]
    fn block_directory_allocate_increments() {
        let mut directory = BlockDirectory::new();
        assert_eq!(directory.reserve_next_block_id().unwrap(), 0);
        assert_eq!(directory.reserve_next_block_id().unwrap(), 1);
        assert_eq!(directory.reserve_next_block_id().unwrap(), 2);
        assert_eq!(directory.block_count(), 3);
    }

    #[test]
    fn block_directory_seal_fills_placeholder() {
        let mut directory = BlockDirectory::new();
        let block_id = directory.reserve_next_block_id().unwrap();
        directory
            .seal_block(
                block_id,
                BlockDirEntry {
                    payload_offset: 10,
                    payload_size: 20,
                    uncompressed_len_bytes: 40,
                    checksum: 0,
                },
            )
            .unwrap();
        assert_eq!(directory.entries[block_id as usize].payload_size, 20);
    }

    #[test]
    fn block_directory_seal_unknown_id_errors() {
        let mut directory = BlockDirectory::new();
        let result = directory.seal_block(99, BlockDirEntry::default());
        assert!(result.is_err());
    }

    #[test]
    fn block_dir_entry_serialises_to_correct_size() {
        let entry = BlockDirEntry {
            payload_offset: 1,
            payload_size: 2,
            uncompressed_len_bytes: 3,
            checksum: 0,
        };
        let mut buffer = Vec::new();
        entry.write_to_buffer(&mut buffer);
        assert_eq!(buffer.len(), BLOCK_DIRECTORY_ENTRY_SIZE);
    }

    #[test]
    fn block_dir_entry_bytes_are_little_endian() {
        let entry = BlockDirEntry {
            payload_offset: 0x0102030405060708,
            payload_size: 0,
            uncompressed_len_bytes: 0,
            checksum: 0,
        };
        let mut buffer = Vec::new();
        entry.write_to_buffer(&mut buffer);
        assert_eq!(
            &buffer[0..8],
            &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );
    }

    #[test]
    fn open_blocks_insert_take_roundtrip() {
        let group = BlockGroup::new(1000514, 4, 0);
        let other = BlockGroup::new(1000515, 4, 0);
        let mut open = OpenBlocks::new();
        assert!(!open.is_open(group));
        open.insert(
            group,
            ActiveBlock {
                block_id: 7,
                stride: group.stride(),
                accumulated_data: vec![1, 2, 3, 4],
                compress_sequentially: false,
            },
        );
        assert!(open.is_open(group));
        assert!(!open.is_open(other));
        let taken_block = open.take(group).unwrap();
        assert_eq!(taken_block.block_id, 7);
        assert!(!open.is_open(group));
    }

    #[test]
    fn open_blocks_byte_len() {
        let group = BlockGroup::new(1000514, 4, 0);
        let mut open = OpenBlocks::new();
        assert_eq!(open.byte_len(group), 0);
        open.insert(
            group,
            ActiveBlock {
                block_id: 0,
                stride: group.stride(),
                accumulated_data: vec![0u8; 12],
                compress_sequentially: false,
            },
        );
        assert_eq!(open.byte_len(group), 12);
    }

    #[test]
    fn block_store_ensure_open_block_creates_new() {
        let group = BlockGroup::new(1000514, 4, 0);
        let mut store = BlockStore::new(1024);
        assert_eq!(store.block_count(), 0);
        store.ensure_open_block(group, 16).unwrap();
        assert_eq!(store.block_count(), 1);
        assert!(store.open_blocks.is_open(group));
    }

    #[test]
    fn block_store_ensure_open_block_is_idempotent() {
        let group = BlockGroup::new(1000514, 4, 0);
        let mut store = BlockStore::new(1024);
        store.ensure_open_block(group, 16).unwrap();
        store.ensure_open_block(group, 16).unwrap();
        assert_eq!(store.block_count(), 1);
    }

    #[test]
    fn block_store_would_overflow_empty_block_returns_false() {
        let group = BlockGroup::new(1000514, 4, 0);
        let store = BlockStore::new(16);
        assert!(!store.would_overflow(group, 20));
    }

    #[test]
    fn block_store_would_overflow_detects_threshold() {
        let group = BlockGroup::new(1000514, 4, 0);
        let mut store = BlockStore::new(16);
        store.ensure_open_block(group, 12).unwrap();
        store
            .append_to_block(group, 12, |buf| {
                buf.extend_from_slice(&[0u8; 12]);
                Ok(())
            })
            .unwrap();
        assert!(store.would_overflow(group, 8));
        assert!(!store.would_overflow(group, 4));
    }

    #[test]
    fn block_store_append_returns_correct_element_offsets() {
        let group = BlockGroup::new(1000514, 8, 0);
        let mut store = BlockStore::new(1024);
        store.ensure_open_block(group, 24).unwrap();
        let (_, off0) = store
            .append_to_block(group, 8, |b| {
                b.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        let (_, off1) = store
            .append_to_block(group, 8, |b| {
                b.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        let (_, off2) = store
            .append_to_block(group, 8, |b| {
                b.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        assert_eq!(off0, 0);
        assert_eq!(off1, 1);
        assert_eq!(off2, 2);
    }

    #[test]
    fn block_store_open_isolated_block_replaces_open_block() {
        let group = BlockGroup::new(1000514, 4, 0);
        let mut store = BlockStore::new(1024);
        store.ensure_open_block(group, 8).unwrap();
        let first_id = store.open_blocks.get_mut(group).unwrap().block_id;
        let isolated_id = store.open_isolated_block(group, 256, true).unwrap();
        assert_ne!(first_id, isolated_id);
        assert_eq!(
            store.open_blocks.get_mut(group).unwrap().block_id,
            isolated_id
        );
    }

    #[test]
    fn block_store_seal_and_directory_roundtrip() {
        let mut store = BlockStore::new(1024);
        let bid = store.directory.reserve_next_block_id().unwrap();
        store
            .seal(
                bid,
                BlockDirEntry {
                    payload_offset: 100,
                    payload_size: 50,
                    uncompressed_len_bytes: 200,
                    checksum: 0,
                },
            )
            .unwrap();
        let mut buf = Vec::new();
        store.write_directory(&mut buf);
        assert_eq!(buf.len(), BLOCK_DIRECTORY_ENTRY_SIZE);
    }

    #[test]
    fn block_store_different_groups_independent() {
        let mz = BlockGroup::new(1000514, 8, 0);
        let intensity = BlockGroup::new(1000515, 4, 0);
        let absent = BlockGroup::new(1000516, 2, 0);
        let mut store = BlockStore::new(1024);
        store.ensure_open_block(mz, 8).unwrap();
        store.ensure_open_block(intensity, 8).unwrap();
        assert_eq!(store.block_count(), 2);
        assert!(store.open_blocks.is_open(mz));
        assert!(store.open_blocks.is_open(intensity));
        assert!(!store.open_blocks.is_open(absent));
    }

    struct VecOutput(Vec<u8>);

    impl WriteBytes for VecOutput {
        fn write(&mut self, bytes: &[u8]) -> IonResult<()> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
        fn patch(&mut self, at: u64, bytes: &[u8]) -> IonResult<()> {
            let start = at as usize;
            self.0[start..start + bytes.len()].copy_from_slice(bytes);
            Ok(())
        }
        fn position(&mut self) -> IonResult<u64> {
            Ok(self.0.len() as u64)
        }
    }

    struct PassthroughCompressor;

    impl BlockCompressor for PassthroughCompressor {
        fn compress(&mut self, input: &[u8], output: &mut Vec<u8>) -> IonResult<usize> {
            output.clear();
            output.extend_from_slice(input);
            Ok(input.len())
        }
        fn fork(&self) -> IonResult<Self> {
            Ok(Self)
        }
        fn shuffle_bytes_into(&self, input: &[u8], output: &mut [u8], element_stride: usize) {
            shuffle_with_tail(input, output, element_stride);
        }
    }

    struct CountingCompressor {
        forks: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl BlockCompressor for CountingCompressor {
        fn compress(&mut self, input: &[u8], output: &mut Vec<u8>) -> IonResult<usize> {
            output.clear();
            output.extend_from_slice(input);
            Ok(input.len())
        }
        fn fork(&self) -> IonResult<Self> {
            self.forks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Self {
                forks: self.forks.clone(),
            })
        }
        fn shuffle_bytes_into(&self, input: &[u8], output: &mut [u8], element_stride: usize) {
            shuffle_with_tail(input, output, element_stride);
        }
    }

    #[test]
    fn parallel_flush_usually_reuses_compressor_across_blocks_26() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let forks = Arc::new(AtomicUsize::new(0));
        let block_count = std::cmp::max(256, rayon::current_num_threads() * 8);

        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            8,
            CompressionMode::Compressed(CountingCompressor {
                forks: forks.clone(),
            }),
            PackingId::ByteShuffle,
        );
        builder.set_par_min_blocks(0);
        builder.set_max_pending_bytes(usize::MAX);

        for i in 0..block_count {
            builder
                .add_item_to_box(1, 8, 4, |buf| {
                    buf.extend_from_slice(&[(i % 256) as u8; 8]);
                    Ok(())
                })
                .unwrap();
        }

        builder.finish().unwrap();

        let fork_count = forks.load(Ordering::SeqCst);
        assert!(
            fork_count < block_count,
            "map_init usually reuses a forked compressor across several blocks, so forks ({}) should stay below the block count ({}); an occasional flip under maximal rayon splitting is expected, not a regression",
            fork_count,
            block_count
        );
    }

    #[test]
    fn block_writer_raw_single_item() {
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            64 * 1024 * 1024,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let item_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let (block_id, element_offset) = builder
            .add_item_to_box(1, item_data.len(), 8, |buf| {
                buf.extend_from_slice(&item_data);
                Ok(())
            })
            .unwrap();
        assert_eq!(block_id, 0);
        assert_eq!(element_offset, 0);
        let ContainerSummary {
            block_count,
            total_bytes,
            ..
        } = builder.finish().unwrap();
        assert_eq!(block_count, 1);
        assert!(total_bytes > 0);
        assert!(output.0.starts_with(&item_data));
    }

    #[test]
    fn same_stride_different_array_types_get_different_blocks() {
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            64 * 1024 * 1024,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let mz_array_type = 1000514;
        let intensity_array_type = 1000515;
        let (mz_block, _) = builder
            .add_item_to_box(mz_array_type, 8, 4, |buf| {
                buf.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        let (intensity_block, _) = builder
            .add_item_to_box(intensity_array_type, 8, 4, |buf| {
                buf.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        assert_ne!(
            mz_block, intensity_block,
            "different array types must never share a block"
        );
        assert_eq!(builder.finish().unwrap().block_count, 2);
    }

    #[test]
    fn same_type_different_windows_get_different_blocks() {
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            64 * 1024 * 1024,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let array_type = 1000514;
        let (window_0_block, _) = builder
            .add_item_to_window(array_type, 0, 8, 4, |buf| {
                buf.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        let (window_1_block, _) = builder
            .add_item_to_window(array_type, 1, 8, 4, |buf| {
                buf.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        assert_ne!(
            window_0_block, window_1_block,
            "different windows must never share a block"
        );
        let block_count = builder.finish().unwrap().block_count as usize;
        assert_eq!(block_count, 2);

        let directory_start = output.0.len() - block_count * BLOCK_DIRECTORY_ENTRY_SIZE;
        let reserved_tail = |block_id: u32| -> u32 {
            let at = directory_start + block_id as usize * BLOCK_DIRECTORY_ENTRY_SIZE + 28;
            u32::from_le_bytes(output.0[at..at + 4].try_into().unwrap())
        };
        assert_eq!(
            reserved_tail(window_0_block),
            0,
            "block directory tail must be reserved zero"
        );
        assert_eq!(
            reserved_tail(window_1_block),
            0,
            "block directory tail must be reserved zero"
        );
    }

    #[test]
    fn oversized_item_uses_sequential_path() {
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            16,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        builder
            .add_item_to_box(1, 64, 8, |buf| {
                buf.extend_from_slice(&[2u8; 64]);
                Ok(())
            })
            .unwrap();
        assert_eq!(builder.pending.len(), 1);
        assert!(
            builder.pending[0].compress_sequentially,
            "oversized blocks must stay on the sequential path"
        );
    }

    #[test]
    fn block_writer_element_offsets_are_correct() {
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            64 * 1024 * 1024,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let (_, first_offset) = builder
            .add_item_to_box(1, 8, 8, |buf| {
                buf.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        let (_, second_offset) = builder
            .add_item_to_box(1, 8, 8, |buf| {
                buf.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        let (_, third_offset) = builder
            .add_item_to_box(1, 8, 8, |buf| {
                buf.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        assert_eq!(first_offset, 0);
        assert_eq!(second_offset, 1);
        assert_eq!(third_offset, 2);
    }

    #[test]
    fn block_writer_different_strides_get_different_blocks() {
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            64 * 1024 * 1024,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let (four_byte_block_id, _) = builder
            .add_item_to_box(1, 4, 4, |buf| {
                buf.extend_from_slice(&[0u8; 4]);
                Ok(())
            })
            .unwrap();
        let (eight_byte_block_id, _) = builder
            .add_item_to_box(1, 8, 8, |buf| {
                buf.extend_from_slice(&[0u8; 8]);
                Ok(())
            })
            .unwrap();
        assert_ne!(four_byte_block_id, eight_byte_block_id);
    }

    #[test]
    fn block_writer_block_splits_when_full() {
        let max_block_size = 16usize;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            max_block_size,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let (first_block_id, _) = builder
            .add_item_to_box(1, 12, 4, |buf| {
                buf.extend_from_slice(&[0u8; 12]);
                Ok(())
            })
            .unwrap();
        let (second_block_id, _) = builder
            .add_item_to_box(1, 12, 4, |buf| {
                buf.extend_from_slice(&[0u8; 12]);
                Ok(())
            })
            .unwrap();
        assert_ne!(
            first_block_id, second_block_id,
            "overflow should have triggered a new block"
        );
        let total_block_count = builder.finish().unwrap().block_count;
        assert_eq!(total_block_count, 2);
    }

    #[test]
    fn block_writer_finish_writes_directory() {
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            64 * 1024 * 1024,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        builder
            .add_item_to_box(1, 8, 8, |buf| {
                buf.extend_from_slice(&[0xAAu8; 8]);
                Ok(())
            })
            .unwrap();
        let ContainerSummary {
            block_count,
            total_bytes,
            ..
        } = builder.finish().unwrap();
        assert_eq!(block_count, 1);
        let expected_directory_size = BLOCK_DIRECTORY_ENTRY_SIZE as u64;
        assert_eq!(total_bytes, 8 + expected_directory_size);
    }

    #[test]
    fn block_writer_empty_produces_no_blocks() {
        let mut output = VecOutput(Vec::new());
        let builder = BlockWriter::new(
            &mut output,
            64 * 1024 * 1024,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let ContainerSummary {
            block_count,
            total_bytes,
            ..
        } = builder.finish().unwrap();
        assert_eq!(block_count, 0);
        assert_eq!(total_bytes, 0);
        assert!(output.0.is_empty());
    }

    fn blocks_in_payload_order(
        raw: &[u8],
        block_count: usize,
        window_of: &std::collections::HashMap<u32, u32>,
    ) -> Vec<(u32, u32)> {
        let directory_start = raw.len() - block_count * BLOCK_DIRECTORY_ENTRY_SIZE;
        let mut by_offset: Vec<(u64, u32)> = (0..block_count as u32)
            .map(|block_id| {
                let at = directory_start + block_id as usize * BLOCK_DIRECTORY_ENTRY_SIZE;
                let offset = u64::from_le_bytes(raw[at..at + 8].try_into().unwrap());
                (offset, block_id)
            })
            .collect();
        by_offset.sort_unstable();
        by_offset
            .into_iter()
            .map(|(_, block_id)| (window_of[&block_id], block_id))
            .collect()
    }

    #[test]
    fn flush_orders_blocks_by_window_then_id() {
        let rounds = 8u32;
        let window_count = 3u32;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            8,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        builder.set_max_pending_bytes(usize::MAX);

        let mut window_of = std::collections::HashMap::new();
        for round in 0..rounds {
            for window in 0..window_count {
                let (block_id, _) = builder
                    .add_item_to_window(1000514, window, 8, 8, |buf| {
                        buf.extend_from_slice(&[round as u8; 8]);
                        Ok(())
                    })
                    .unwrap();
                window_of.insert(block_id, window);
            }
        }
        let block_count = builder.finish().unwrap().block_count as usize;
        let payload_order = blocks_in_payload_order(&output.0, block_count, &window_of);

        let mut sorted = payload_order.clone();
        sorted.sort_unstable();
        assert_eq!(
            payload_order, sorted,
            "one batch must be laid out in window then block id order"
        );

        let block_ids: Vec<u32> = payload_order.iter().map(|(_, id)| *id).collect();
        let mut ascending_ids = block_ids.clone();
        ascending_ids.sort_unstable();
        assert_ne!(
            block_ids, ascending_ids,
            "the window sort must actually reorder blocks away from seal order"
        );
    }

    #[test]
    fn multi_batch_flush_orders_each_batch() {
        let rounds = 8u32;
        let window_count = 3u32;
        let blocks_per_batch = 6usize;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            8,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        builder.set_max_pending_bytes(blocks_per_batch * 8);

        let mut window_of = std::collections::HashMap::new();
        for round in 0..rounds {
            for window in 0..window_count {
                let (block_id, _) = builder
                    .add_item_to_window(1000514, window, 8, 8, |buf| {
                        buf.extend_from_slice(&[round as u8; 8]);
                        Ok(())
                    })
                    .unwrap();
                window_of.insert(block_id, window);
            }
        }
        let block_count = builder.finish().unwrap().block_count as usize;
        let payload_order = blocks_in_payload_order(&output.0, block_count, &window_of);

        let batches: Vec<&[(u32, u32)]> = payload_order.chunks(blocks_per_batch).collect();
        assert!(batches.len() > 1, "the run must span several flushes");

        for batch in batches {
            let mut sorted = batch.to_vec();
            sorted.sort_unstable();
            assert_eq!(
                batch,
                sorted.as_slice(),
                "every batch must be laid out in window then block id order"
            );

            let mut seen_windows: Vec<u32> = batch.iter().map(|(window, _)| *window).collect();
            seen_windows.dedup();
            let unique_windows = seen_windows.len();
            seen_windows.sort_unstable();
            seen_windows.dedup();
            assert_eq!(
                unique_windows,
                seen_windows.len(),
                "each window must occupy one contiguous run inside its batch"
            );
        }
    }

    #[test]
    fn window_major_roundtrip_matches_streaming() {
        let rounds = 8u32;
        let window_count = 3u32;

        let encode = |window_major: bool| -> (Vec<u8>, usize, std::collections::HashMap<u32, u32>) {
            let mut output = VecOutput(Vec::new());
            let builder = BlockWriter::new(
                &mut output,
                8,
                CompressionMode::<PassthroughCompressor>::Raw,
                PackingId::Raw,
            );
            let mut builder = if window_major {
                builder.window_major().unwrap()
            } else {
                builder
            };
            builder.set_max_pending_bytes(48);

            let mut window_of = std::collections::HashMap::new();
            for round in 0..rounds {
                for window in 0..window_count {
                    let content = [(round * window_count + window) as u8; 8];
                    let (block_id, _) = builder
                        .add_item_to_window(1000514, window, 8, 8, |buf| {
                            buf.extend_from_slice(&content);
                            Ok(())
                        })
                        .unwrap();
                    window_of.insert(block_id, window);
                }
            }
            let block_count = builder.finish().unwrap().block_count as usize;
            (output.0, block_count, window_of)
        };

        let (streaming_raw, streaming_count, streaming_windows) = encode(false);
        let (window_major_raw, window_major_count, window_major_windows) = encode(true);
        assert_eq!(streaming_count, window_major_count);

        let block_bytes = |raw: &[u8], block_count: usize, block_id: u32| -> Vec<u8> {
            let directory_start = raw.len() - block_count * BLOCK_DIRECTORY_ENTRY_SIZE;
            let at = directory_start + block_id as usize * BLOCK_DIRECTORY_ENTRY_SIZE;
            let offset = u64::from_le_bytes(raw[at..at + 8].try_into().unwrap()) as usize;
            let size = u64::from_le_bytes(raw[at + 8..at + 16].try_into().unwrap()) as usize;
            raw[offset..offset + size].to_vec()
        };

        for block_id in 0..streaming_count as u32 {
            assert_eq!(
                block_bytes(&streaming_raw, streaming_count, block_id),
                block_bytes(&window_major_raw, window_major_count, block_id),
                "block {block_id} must carry the same bytes under both layouts"
            );
        }

        let streaming_order =
            blocks_in_payload_order(&streaming_raw, streaming_count, &streaming_windows);
        let window_major_order =
            blocks_in_payload_order(&window_major_raw, window_major_count, &window_major_windows);
        assert_ne!(
            streaming_order, window_major_order,
            "window major must physically reorder blocks the batch sort leaves scattered"
        );

        let mut window_runs: Vec<u32> = window_major_order
            .iter()
            .map(|(window, _)| *window)
            .collect();
        window_runs.dedup();
        assert_eq!(window_runs.len(), window_count as usize);
    }

    #[test]
    fn window_major_assembles_contiguous_window_runs() {
        let rounds = 8u32;
        let window_count = 3u32;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            8,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        )
        .window_major()
        .unwrap();
        builder.set_max_pending_bytes(48);

        let mut window_of = std::collections::HashMap::new();
        let mut content_of = std::collections::HashMap::new();
        for round in 0..rounds {
            for window in 0..window_count {
                let content = [(round * window_count + window) as u8; 8];
                let (block_id, _) = builder
                    .add_item_to_window(1000514, window, 8, 8, |buf| {
                        buf.extend_from_slice(&content);
                        Ok(())
                    })
                    .unwrap();
                window_of.insert(block_id, window);
                content_of.insert(block_id, content);
            }
        }
        let block_count = builder.finish().unwrap().block_count as usize;
        assert_eq!(block_count, (rounds * window_count) as usize);

        let raw = output.0;
        let payload_order = blocks_in_payload_order(&raw, block_count, &window_of);

        let mut sorted = payload_order.clone();
        sorted.sort_unstable();
        assert_eq!(
            payload_order, sorted,
            "the whole container must be laid out in window then block id order"
        );

        let mut window_runs: Vec<u32> = payload_order.iter().map(|(window, _)| *window).collect();
        window_runs.dedup();
        assert_eq!(
            window_runs.len(),
            window_count as usize,
            "each window must be one contiguous run across the whole container"
        );

        let directory_start = raw.len() - block_count * BLOCK_DIRECTORY_ENTRY_SIZE;
        for block_id in 0..block_count as u32 {
            let at = directory_start + block_id as usize * BLOCK_DIRECTORY_ENTRY_SIZE;
            let offset = u64::from_le_bytes(raw[at..at + 8].try_into().unwrap()) as usize;
            let size = u64::from_le_bytes(raw[at + 8..at + 16].try_into().unwrap()) as usize;
            assert_eq!(
                &raw[offset..offset + size],
                &content_of[&block_id],
                "block {block_id} must keep its own bytes after staging"
            );
        }
    }

    #[test]
    fn oversized_item_gets_dedicated_block() {
        let max_block_size = 16usize;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            max_block_size,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let (block_id, element_offset) = builder
            .add_item_to_box(1, 64, 8, |buf| {
                buf.extend_from_slice(&[0xABu8; 64]);
                Ok(())
            })
            .unwrap();
        assert_eq!(element_offset, 0);
        let block_count = builder.finish().unwrap().block_count;
        assert_eq!(block_count, 1);
        assert_eq!(block_id, 0);
        assert!(output.0.starts_with(&[0xABu8; 64]));
    }

    #[test]
    fn oversized_item_between_normal_items_produces_three_blocks() {
        let max_block_size = 16usize;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            max_block_size,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let (bid_before, _) = builder
            .add_item_to_box(1, 8, 8, |buf| {
                buf.extend_from_slice(&[0x01u8; 8]);
                Ok(())
            })
            .unwrap();
        let (bid_oversized, off_oversized) = builder
            .add_item_to_box(1, 64, 8, |buf| {
                buf.extend_from_slice(&[0x02u8; 64]);
                Ok(())
            })
            .unwrap();
        let (bid_after, _) = builder
            .add_item_to_box(1, 8, 8, |buf| {
                buf.extend_from_slice(&[0x03u8; 8]);
                Ok(())
            })
            .unwrap();
        assert_eq!(off_oversized, 0);
        assert_ne!(bid_before, bid_oversized);
        assert_ne!(bid_oversized, bid_after);
        let block_count = builder.finish().unwrap().block_count;
        assert_eq!(block_count, 3);
    }

    #[test]
    fn two_consecutive_oversized_items_get_separate_dedicated_blocks() {
        let max_block_size = 16usize;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            max_block_size,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        let (bid_a, off_a) = builder
            .add_item_to_box(1, 64, 8, |buf| {
                buf.extend_from_slice(&[0x11u8; 64]);
                Ok(())
            })
            .unwrap();
        let (bid_b, off_b) = builder
            .add_item_to_box(1, 64, 8, |buf| {
                buf.extend_from_slice(&[0x22u8; 64]);
                Ok(())
            })
            .unwrap();
        assert_eq!(off_a, 0);
        assert_eq!(off_b, 0);
        assert_ne!(bid_a, bid_b);
        let block_count = builder.finish().unwrap().block_count;
        assert_eq!(block_count, 2);
    }

    #[test]
    fn oversized_item_directory_entry_records_actual_uncompressed_size() {
        let max_block_size = 16usize;
        let item_size = 128usize;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            max_block_size,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        builder
            .add_item_to_box(1, item_size, 8, |buf| {
                buf.extend_from_slice(&[0xFFu8; 128]);
                Ok(())
            })
            .unwrap();
        let block_count = builder.finish().unwrap().block_count;
        assert_eq!(block_count, 1);
        let directory_start = output.0.len() - BLOCK_DIRECTORY_ENTRY_SIZE;
        let uncomp_bytes = u64::from_le_bytes(
            output.0[directory_start + 16..directory_start + 24]
                .try_into()
                .unwrap(),
        );
        assert_eq!(uncomp_bytes, item_size as u64);
    }

    #[test]
    fn block_writer_seq_and_par_match() {
        let mut seq_out = VecOutput(Vec::new());
        let mut seq = BlockWriter::new(
            &mut seq_out,
            8,
            CompressionMode::Compressed(PassthroughCompressor),
            PackingId::ByteShuffle,
        );
        seq.set_par_min_blocks(usize::MAX);

        let mut par_out = VecOutput(Vec::new());
        let mut par = BlockWriter::new(
            &mut par_out,
            8,
            CompressionMode::Compressed(PassthroughCompressor),
            PackingId::ByteShuffle,
        );
        par.set_par_min_blocks(0);

        for builder in [&mut seq, &mut par] {
            builder
                .add_item_to_box(1, 8, 4, |buf| {
                    buf.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
                    Ok(())
                })
                .unwrap();
            builder
                .add_item_to_box(1, 8, 4, |buf| {
                    buf.extend_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);
                    Ok(())
                })
                .unwrap();
            builder
                .add_item_to_box(1, 8, 4, |buf| {
                    buf.extend_from_slice(&[17, 18, 19, 20, 21, 22, 23, 24]);
                    Ok(())
                })
                .unwrap();
            builder
                .add_item_to_box(1, 8, 4, |buf| {
                    buf.extend_from_slice(&[25, 26, 27, 28, 29, 30, 31, 32]);
                    Ok(())
                })
                .unwrap();
        }

        let seq_res = seq.finish().unwrap();
        let par_res = par.finish().unwrap();

        assert_eq!(seq_res, par_res);
        assert_eq!(seq_out.0, par_out.0);
    }

    #[test]
    fn batched_flush_keeps_directory_offsets_correct() {
        let item_count = 7usize;
        let mut output = VecOutput(Vec::new());
        let mut builder = BlockWriter::new(
            &mut output,
            8,
            CompressionMode::<PassthroughCompressor>::Raw,
            PackingId::Raw,
        );
        builder.set_max_pending_bytes(16);

        for i in 0..item_count {
            builder
                .add_item_to_box(1, 8, 8, |buf| {
                    buf.extend_from_slice(&[i as u8; 8]);
                    Ok(())
                })
                .unwrap();
        }
        let block_count = builder.finish().unwrap().block_count;
        assert_eq!(block_count as usize, item_count);

        let raw = output.0;
        let directory_start = raw.len() - item_count * BLOCK_DIRECTORY_ENTRY_SIZE;
        for i in 0..item_count {
            let at = directory_start + i * BLOCK_DIRECTORY_ENTRY_SIZE;
            let offset = u64::from_le_bytes(raw[at..at + 8].try_into().unwrap()) as usize;
            let size = u64::from_le_bytes(raw[at + 8..at + 16].try_into().unwrap()) as usize;
            assert_eq!(&raw[offset..offset + size], &[i as u8; 8]);
        }
    }

    #[test]
    fn block_id_fits_under_limit() {
        assert_eq!(block_id_from_count(0).unwrap(), 0);
        assert_eq!(block_id_from_count(1).unwrap(), 1);
        assert_eq!(block_id_from_count(u32::MAX as usize).unwrap(), u32::MAX);
    }

    #[test]
    fn block_id_over_limit_errors() {
        assert!(block_id_from_count(u32::MAX as usize + 1).is_err());
    }
}
