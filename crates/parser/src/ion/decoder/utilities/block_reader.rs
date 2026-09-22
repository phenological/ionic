use std::{cell::RefCell, sync::Arc};

use cosmoz::{DecompressOptions, Decoder};

use crate::ion::{
    ByteRange, IonError, IonResult,
    decoder::utilities::byte_source::{ReadBytes, SourceBytes},
    encoder::utilities::block_writer::{BLOCK_DIRECTORY_ENTRY_SIZE, BlockDirEntry, Stride},
    format::CODEC_NONE,
    packing::PackingId,
    utilities::{
        common::{read_u32_le_at, read_u64_le_at},
        decompression_limit::DecompressionLimit,
    },
};

const LRU_NONE: usize = usize::MAX;

thread_local! {
    static BLOCK_DECODE_SCRATCH: RefCell<(Decoder, Vec<u8>)> =
        RefCell::new((Decoder::new(&DecompressOptions::default()), Vec::new()));
    static UNSHUFFLE_SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn unshuffle_into_owned(
    processor: &dyn BlockProcessor,
    source: &[u8],
    stride: Stride,
    uncompressed_len: usize,
) -> Vec<u8> {
    UNSHUFFLE_SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        if scratch.len() < uncompressed_len {
            scratch.resize(uncompressed_len, 0);
        }
        processor.unshuffle(source, &mut scratch[..uncompressed_len], stride.as_usize());
        scratch[..uncompressed_len].to_vec()
    })
}

fn decompress_zstd_block(
    compressed: &[u8],
    target_len: usize,
    budget: DecompressionLimit,
) -> IonResult<Vec<u8>> {
    if target_len == 0 {
        return Ok(Vec::new());
    }
    budget.validate(compressed.len(), target_len)?;

    BLOCK_DECODE_SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        let (decoder, buf) = &mut *scratch;
        if buf.len() < target_len {
            buf.resize(target_len, 0);
        }
        let actual = decoder
            .decompress_into(compressed, &mut buf[..target_len])
            .map_err(|err| IonError::from(format!("zstd decode failed: {err:?}")))?;

        if actual != target_len {
            return Err(
                format!("zstd: bad decoded size (got={actual}, expected={target_len})").into(),
            );
        }

        Ok(buf[..target_len].to_vec())
    })
}

pub(crate) trait BlockProcessor {
    fn decompress(
        &self,
        compressed: &[u8],
        target_len: usize,
        budget: DecompressionLimit,
    ) -> IonResult<Vec<u8>>;
    fn unshuffle(&self, source: &[u8], target: &mut [u8], stride: usize);
    fn requires_unshuffle(&self, block_packing_id: PackingId) -> bool;
}

pub(crate) trait ContainerAccess {
    fn get_array_bytes_from_block(
        &mut self,
        block_id: u32,
        element_offset: u64,
        element_count: u64,
        element_stride: usize,
        ctx: &'static str,
    ) -> IonResult<&[u8]>;
}

#[derive(Debug)]
pub(crate) struct DefaultBlockProcessor;

impl BlockProcessor for DefaultBlockProcessor {
    #[inline]
    fn decompress(
        &self,
        compressed: &[u8],
        target_len: usize,
        budget: DecompressionLimit,
    ) -> IonResult<Vec<u8>> {
        decompress_zstd_block(compressed, target_len, budget)
    }

    #[inline]
    fn unshuffle(&self, source: &[u8], target: &mut [u8], stride: usize) {
        unshuffle_bytes(source, target, stride);
    }

    #[inline]
    fn requires_unshuffle(&self, block_packing_id: PackingId) -> bool {
        block_packing_id == PackingId::ByteShuffle
    }
}

pub(crate) struct BlockReader<P: BlockProcessor> {
    source: Arc<dyn ReadBytes>,
    container_offset: u64,
    container_len: u64,
    entries: Vec<BlockDirEntry>,
    cache: Box<[Option<SourceBytes>]>,
    lru_prev: Box<[usize]>,
    lru_next: Box<[usize]>,
    lru_head: usize,
    lru_tail: usize,
    cached_bytes: usize,
    max_cached_bytes: usize,
    stride_history: Box<[Option<Stride>]>,
    compression_codec: u8,
    block_packing_id: PackingId,
    verify_checksums: bool,
    decompression_limit: DecompressionLimit,
    processor: P,
}

impl<P: BlockProcessor> BlockReader<P> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        source: Arc<dyn ReadBytes>,
        container_offset: u64,
        container_len: u64,
        block_count: u64,
        directory_crc32: u32,
        compression_codec: u8,
        block_packing_id: PackingId,
        verify_checksums: bool,
        ctx: &'static str,
        processor: P,
        max_cached_bytes: usize,
        decompression_limit: DecompressionLimit,
    ) -> IonResult<Self> {
        let block_count = validate_block_count(block_count, container_len, ctx)?;
        let directory =
            container_directory_range(container_offset, container_len, block_count, ctx)?;
        let directory_bytes = source.read(directory)?;

        if verify_checksums {
            verify_directory_crc(&directory_bytes, directory_crc32, ctx)?;
        }

        let entries = read_entries(&directory_bytes, block_count, ctx)?;

        Ok(Self {
            source,
            container_offset,
            container_len,
            entries,
            cache: std::iter::repeat_with(|| None)
                .take(block_count)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            lru_prev: vec![LRU_NONE; block_count].into_boxed_slice(),
            lru_next: vec![LRU_NONE; block_count].into_boxed_slice(),
            lru_head: LRU_NONE,
            lru_tail: LRU_NONE,
            cached_bytes: 0,
            max_cached_bytes,
            stride_history: vec![None; block_count].into_boxed_slice(),
            compression_codec,
            block_packing_id,
            verify_checksums,
            decompression_limit,
            processor,
        })
    }

    pub(crate) fn block_byte_range(&self, block_id: u32) -> Option<ByteRange> {
        let entry = self.entries.get(block_id as usize)?;
        let offset = self.container_offset.checked_add(entry.payload_offset)?;
        Some(ByteRange {
            offset,
            length: entry.payload_size,
        })
    }

    fn get_block_payload(
        &self,
        block_index: usize,
        ctx: &'static str,
    ) -> IonResult<(BlockDirEntry, SourceBytes)> {
        let entry = *self.entries.get(block_index).ok_or_else(|| {
            IonError::from(format!(
                "{ctx}: block index {block_index} out of range (count={})",
                self.cache.len()
            ))
        })?;

        let payload_region_len = self
            .container_len
            .saturating_sub((self.entries.len() as u64) * (BLOCK_DIRECTORY_ENTRY_SIZE as u64));

        let payload_end = entry.payload_offset.checked_add(entry.payload_size);
        if payload_end.is_none_or(|end| end > payload_region_len) {
            return Err(
                format!("{ctx}: block {block_index} payload exceeds container bounds").into(),
            );
        }

        let read_offset = self
            .container_offset
            .checked_add(entry.payload_offset)
            .ok_or_else(|| {
                IonError::from(format!(
                    "{ctx}: block {block_index} payload offset overflows"
                ))
            })?;

        let payload = self.source.read(ByteRange {
            offset: read_offset,
            length: entry.payload_size,
        })?;

        if self.verify_checksums {
            let computed = crc32fast::hash(&payload);
            if computed != entry.checksum {
                return Err(format!(
                    "{ctx}: block {block_index} checksum mismatch \
                     (stored={:#010x}, computed={:#010x})",
                    entry.checksum, computed
                )
                .into());
            }
        }

        Ok((entry, payload))
    }

    fn decode_block(
        &self,
        payload: SourceBytes,
        uncompressed_len: usize,
        stride: Stride,
    ) -> IonResult<SourceBytes> {
        self.decompression_limit
            .validate(payload.len(), uncompressed_len)?;

        let needs_unshuffle =
            self.processor.requires_unshuffle(self.block_packing_id) && stride != Stride::OneByte;

        if self.compression_codec == CODEC_NONE {
            if payload.len() != uncompressed_len {
                return Err(format!(
                    "uncompressed payload size mismatch: got {}, expected {uncompressed_len}",
                    payload.len()
                )
                .into());
            }
            if !needs_unshuffle {
                return Ok(payload);
            }
            let scratch =
                unshuffle_into_owned(&self.processor, &payload, stride, uncompressed_len);
            return Ok(SourceBytes::Owned(scratch));
        }

        let decompressed =
            self.processor
                .decompress(&payload, uncompressed_len, self.decompression_limit)?;

        if !needs_unshuffle {
            return Ok(SourceBytes::Owned(decompressed));
        }

        let scratch =
            unshuffle_into_owned(&self.processor, &decompressed, stride, uncompressed_len);
        Ok(SourceBytes::Owned(scratch))
    }

    pub(crate) fn read_block(
        &self,
        block_id: u32,
        element_stride: usize,
        ctx: &'static str,
    ) -> IonResult<SourceBytes> {
        let block_index = block_id as usize;
        let stride = Stride::from_size(element_stride);
        let (entry, payload) = self.get_block_payload(block_index, ctx)?;
        let uncompressed_len = usize::try_from(entry.uncompressed_len_bytes).map_err(|_| {
            IonError::from(format!(
                "{ctx}: uncompressed length too large for this platform"
            ))
        })?;
        self.decode_block(payload, uncompressed_len, stride)
    }

    #[inline]
    fn touch_lru(&mut self, block_index: usize) {
        if self.lru_head == block_index {
            return;
        }
        self.detach_lru(block_index);
        self.attach_lru_front(block_index);
    }

    #[inline]
    fn attach_lru_front(&mut self, block_index: usize) {
        self.lru_prev[block_index] = LRU_NONE;
        self.lru_next[block_index] = self.lru_head;
        if self.lru_head != LRU_NONE {
            self.lru_prev[self.lru_head] = block_index;
        } else {
            self.lru_tail = block_index;
        }
        self.lru_head = block_index;
    }

    #[inline]
    fn detach_lru(&mut self, block_index: usize) {
        let prev = self.lru_prev[block_index];
        let next = self.lru_next[block_index];
        if prev != LRU_NONE {
            self.lru_next[prev] = next;
        } else if self.lru_head == block_index {
            self.lru_head = next;
        }
        if next != LRU_NONE {
            self.lru_prev[next] = prev;
        } else if self.lru_tail == block_index {
            self.lru_tail = prev;
        }
        self.lru_prev[block_index] = LRU_NONE;
        self.lru_next[block_index] = LRU_NONE;
    }

    #[inline]
    fn evict_lru_tail(&mut self) {
        let block_index = self.lru_tail;
        if block_index == LRU_NONE {
            return;
        }
        self.detach_lru(block_index);
        if let Some(block) = self.cache[block_index].take() {
            self.cached_bytes = self.cached_bytes.saturating_sub(block.len());
        }
    }

    fn evict_until_room(&mut self, needed: usize) {
        if self.max_cached_bytes == 0 {
            return;
        }
        while self
            .cached_bytes
            .checked_add(needed)
            .is_none_or(|total| total > self.max_cached_bytes)
        {
            if self.lru_tail == LRU_NONE {
                break;
            }
            self.evict_lru_tail();
        }
    }

    fn ensure_block_loaded(
        &mut self,
        block_id: u32,
        element_stride: usize,
        ctx: &'static str,
    ) -> IonResult<()> {
        let block_index = block_id as usize;
        if block_index >= self.cache.len() {
            return Err(format!(
                "{ctx}: block index {block_index} out of range (count={})",
                self.cache.len()
            )
            .into());
        }

        let stride = Stride::from_size(element_stride);
        self.record_stride_or_fail(block_index, stride, ctx)?;

        if self.cache[block_index].is_some() {
            return Ok(());
        }

        let (entry, payload) = self.get_block_payload(block_index, ctx)?;
        let uncompressed_len = usize::try_from(entry.uncompressed_len_bytes).map_err(|_| {
            IonError::from(format!(
                "{ctx}: uncompressed length too large for this platform"
            ))
        })?;
        let decoded = self.decode_block(payload, uncompressed_len, stride)?;

        let block_heap = decoded.len();
        self.evict_until_room(block_heap);
        self.cached_bytes += block_heap;
        self.cache[block_index] = Some(decoded);
        self.attach_lru_front(block_index);
        Ok(())
    }

    fn record_stride_or_fail(
        &mut self,
        block_index: usize,
        stride: Stride,
        ctx: &'static str,
    ) -> IonResult<()> {
        if !self.processor.requires_unshuffle(self.block_packing_id) || stride == Stride::OneByte {
            return Ok(());
        }
        match self.stride_history[block_index] {
            None => {
                self.stride_history[block_index] = Some(stride);
                Ok(())
            }
            Some(recorded) if recorded == stride => Ok(()),
            Some(recorded) => Err(format!(
                "{ctx}: stride mismatch for block {block_index} \
                 (expected {recorded:?}, got {stride:?})"
            )
            .into()),
        }
    }
}

impl<P: BlockProcessor> ContainerAccess for BlockReader<P> {
    #[inline]
    fn get_array_bytes_from_block(
        &mut self,
        block_id: u32,
        element_offset: u64,
        element_count: u64,
        element_stride: usize,
        ctx: &'static str,
    ) -> IonResult<&[u8]> {
        self.ensure_block_loaded(block_id, element_stride, ctx)?;

        let block_index = block_id as usize;
        self.touch_lru(block_index);

        let block = self.cache[block_index].as_ref().unwrap();
        let start_byte = usize::try_from(element_offset)
            .ok()
            .and_then(|offset| offset.checked_mul(element_stride))
            .ok_or_else(|| {
                IonError::from(format!("{ctx}: item range overflow for block {block_id}"))
            })?;
        let end_byte = usize::try_from(element_count)
            .ok()
            .and_then(|count| count.checked_mul(element_stride))
            .and_then(|len| start_byte.checked_add(len))
            .ok_or_else(|| {
                IonError::from(format!("{ctx}: item range overflow for block {block_id}"))
            })?;

        if end_byte > block.len() {
            return Err(format!(
                "{ctx}: item range [{start_byte}..{end_byte}] out of bounds \
                 for block {block_id} (len={})",
                block.len()
            )
            .into());
        }
        Ok(&block[start_byte..end_byte])
    }
}

#[inline(always)]
fn unshuffle_bytes(source: &[u8], target: &mut [u8], stride: usize) {
    crate::ion::byte_transpose::unshuffle(source, target, stride);
}

pub(crate) fn container_directory_range(
    container_offset: u64,
    container_len: u64,
    block_count: usize,
    ctx: &'static str,
) -> IonResult<ByteRange> {
    let directory_byte_count = (block_count as u64) * (BLOCK_DIRECTORY_ENTRY_SIZE as u64);
    let directory_offset = container_offset
        .checked_add(container_len)
        .and_then(|end| end.checked_sub(directory_byte_count))
        .ok_or_else(|| IonError::from(format!("{ctx}: directory offset overflows")))?;
    Ok(ByteRange {
        offset: directory_offset,
        length: directory_byte_count,
    })
}

#[inline]
fn validate_block_count(
    block_count: u64,
    container_len: u64,
    ctx: &'static str,
) -> IonResult<usize> {
    let max_blocks = container_len / (BLOCK_DIRECTORY_ENTRY_SIZE as u64);
    if block_count > max_blocks {
        return Err(format!(
            "{ctx}: block_count {block_count} exceeds maximum \
             {max_blocks} derivable from container length {container_len}"
        )
        .into());
    }
    usize::try_from(block_count)
        .map_err(|_| IonError::from(format!("{ctx}: block count too large for this platform")))
}

fn verify_directory_crc(bytes: &[u8], expected: u32, ctx: &'static str) -> IonResult<()> {
    let found = crc32fast::hash(bytes);
    if found != expected {
        return Err(format!(
            "{ctx}: block directory checksum mismatch (stored={expected:#010x}, computed={found:#010x})"
        )
        .into());
    }
    Ok(())
}

fn read_entries(
    directory_bytes: &[u8],
    block_count: usize,
    ctx: &'static str,
) -> IonResult<Vec<BlockDirEntry>> {
    let mut read_pos = 0usize;
    let mut entries = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        let payload_offset = read_u64_le_at(directory_bytes, &mut read_pos, ctx)?;
        let payload_size = read_u64_le_at(directory_bytes, &mut read_pos, ctx)?;
        let uncompressed_len_bytes = read_u64_le_at(directory_bytes, &mut read_pos, ctx)?;
        let checksum = read_u32_le_at(directory_bytes, &mut read_pos, ctx)?;
        let _reserved = read_u32_le_at(directory_bytes, &mut read_pos, ctx)?;
        entries.push(BlockDirEntry {
            payload_offset,
            payload_size,
            uncompressed_len_bytes,
            checksum,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ion::decoder::utilities::byte_source::BytesSource;

    fn single_block_container(payload: &[u8]) -> (Arc<dyn ReadBytes>, u64, u64) {
        let mut container_bytes = payload.to_vec();
        let uncompressed_len = payload.len() as u64;
        container_bytes.extend_from_slice(&0u64.to_le_bytes());
        container_bytes.extend_from_slice(&uncompressed_len.to_le_bytes());
        container_bytes.extend_from_slice(&uncompressed_len.to_le_bytes());
        container_bytes.extend_from_slice(&0u32.to_le_bytes());
        container_bytes.extend_from_slice(&0u32.to_le_bytes());

        let container_len = container_bytes.len() as u64;
        let source: Arc<dyn ReadBytes> = Arc::new(BytesSource::new(Arc::from(
            container_bytes.into_boxed_slice(),
        )));
        (source, 0, container_len)
    }

    #[test]
    fn cached_block_reread_at_new_stride_is_rejected_5() {
        let payload: Vec<u8> = (0u8..16).collect();
        let (source, container_offset, container_len) = single_block_container(&payload);

        let mut reader = BlockReader::new(
            source,
            container_offset,
            container_len,
            1,
            0,
            CODEC_NONE,
            PackingId::ByteShuffle,
            false,
            "test",
            DefaultBlockProcessor,
            4096,
            DecompressionLimit::default(),
        )
        .unwrap();

        reader
            .ensure_block_loaded(0, 8, "test")
            .expect("first read at an 8-byte stride must load the block");

        let result = reader.ensure_block_loaded(0, 4, "test");

        assert!(
            result.is_err(),
            "rereading a cached block at a different stride must be rejected, not silently reused"
        );
    }
}
