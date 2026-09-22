use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use crate::ion::{
    ByteRange, IonError, IonResult,
    decoder::{
        decode::{Metadatum, MetadatumValue, ReadOptions},
        utilities::{
            byte_source::ReadBytes,
            common::decompress_zstd_allow_aligned_padding,
            decompression_limit::DecompressionLimit,
            meta_column_layout::MetaColumnLayout,
            parse_metadata::{CODEC_NONE, CODEC_ZSTD, parse_metadata},
        },
    },
    meta_groups::{
        META_GROUP_ENTRY_SIZE, META_GROUP_HEADER_SIZE, MetaGroupEntry, MetaSection,
        group_count_for, group_of_item, item_range_of_group, meta_directory_range,
        read_group_header,
    },
};

pub(crate) struct MetaGroupReader {
    source: Arc<dyn ReadBytes>,
    section: MetaSection,
    directory: Vec<MetaGroupEntry>,
    payload_end: u64,
    compression_codec: u8,
    verify_checksums: bool,
    budget: DecompressionLimit,
    cache: GroupCache,
    layout: MetaColumnLayout,
}

#[derive(Default)]
struct MetaCounts {
    rows: u64,
    numeric: u64,
    string: u64,
}

impl MetaCounts {
    fn add(&mut self, rows: u32, numeric: u32, string: u32) {
        self.rows += rows as u64;
        self.numeric += numeric as u64;
        self.string += string as u64;
    }
}

struct CachedGroup {
    rows: Arc<[Metadatum]>,
    footprint: usize,
}

struct GroupCache {
    groups: HashMap<u64, CachedGroup>,
    order: VecDeque<u64>,
    used_bytes: usize,
    max_bytes: usize,
}

impl MetaGroupReader {
    pub(crate) fn open(
        source: Arc<dyn ReadBytes>,
        section: MetaSection,
        compression_codec: u8,
        options: &ReadOptions,
        layout: MetaColumnLayout,
    ) -> IonResult<Self> {
        if section.item_count > 0 && section.group_size == 0 {
            return Err(IonError::from("metadata groups: group size is zero"));
        }
        if section.group_count != group_count_for(section.item_count, section.group_size) {
            return Err(IonError::from(
                "metadata groups: group count does not match item count",
            ));
        }
        let directory_range = meta_directory_range(section.range, section.group_count)?;
        let directory: Vec<MetaGroupEntry> = source
            .read(directory_range)?
            .as_chunks::<META_GROUP_ENTRY_SIZE>()
            .0
            .iter()
            .map(|entry| MetaGroupEntry::read_from(entry))
            .collect();
        let mut uncompressed_sum: u64 = 0;
        for entry in &directory {
            uncompressed_sum = uncompressed_sum
                .checked_add(entry.uncompressed_size)
                .ok_or_else(|| IonError::from("metadata groups: uncompressed total overflows"))?;
        }
        if uncompressed_sum != section.totals.uncompressed {
            return Err(IonError::from(
                "metadata groups: uncompressed total does not match header",
            ));
        }
        let payload_end = directory_range.offset - section.range.offset;
        Ok(Self {
            source,
            section,
            directory,
            payload_end,
            compression_codec,
            verify_checksums: options.verify_checksums,
            budget: options.decompression_limit,
            cache: GroupCache {
                groups: HashMap::new(),
                order: VecDeque::new(),
                used_bytes: 0,
                max_bytes: options.max_cached_bytes,
            },
            layout,
        })
    }

    #[cfg(test)]
    pub(crate) fn read_all(&self) -> IonResult<Vec<Metadatum>> {
        Ok(self.read_all_grouped()?.into_iter().flatten().collect())
    }

    pub(crate) fn read_all_grouped(&self) -> IonResult<Vec<Vec<Metadatum>>> {
        let payloads = self.source.read(ByteRange {
            offset: self.section.range.offset,
            length: self.payload_end,
        })?;
        let mut groups = Vec::with_capacity(self.directory.len());
        let mut seen = MetaCounts::default();
        for (group_index, entry) in self.directory.iter().enumerate() {
            let range = self.payload_range(entry)?;
            let start = (range.offset - self.section.range.offset) as usize;
            let end = start + range.length as usize;
            let payload = payloads
                .get(start..end)
                .ok_or_else(|| IonError::from("metadata groups: payload out of bounds"))?;
            let bytes = self.decode_payload(entry, payload)?;
            let (meta_count, numeric_count, string_count) = read_group_header(&bytes)?;
            seen.add(meta_count, numeric_count, string_count);
            groups.push(self.parse_group(&bytes, group_index as u64)?);
        }
        self.check_totals(&seen)?;
        Ok(groups)
    }

    pub(crate) fn read_first_group(&self) -> IonResult<Vec<Metadatum>> {
        if self.directory.is_empty() {
            return Ok(Vec::new());
        }
        self.read_group(0)
    }

    fn check_totals(&self, seen: &MetaCounts) -> IonResult<()> {
        if seen.rows != self.section.totals.rows
            || seen.numeric != self.section.totals.numeric
            || seen.string != self.section.totals.string
        {
            return Err(IonError::from(
                "metadata groups: row totals do not match header",
            ));
        }
        Ok(())
    }

    pub(crate) fn read_item(&mut self, item_index: u64) -> IonResult<Vec<Metadatum>> {
        if item_index >= self.section.item_count {
            return Ok(Vec::new());
        }
        let group_index = group_of_item(item_index, self.section.group_size);
        let rows = self.get_cached_rows(group_index)?;
        let start = rows.partition_point(|row| (row.item_index as u64) < item_index);
        let end = rows.partition_point(|row| (row.item_index as u64) <= item_index);
        Ok(rows[start..end].to_vec())
    }

    fn get_cached_rows(&mut self, group_index: u64) -> IonResult<Arc<[Metadatum]>> {
        if let Some(group) = self.cache.groups.get(&group_index) {
            return Ok(group.rows.clone());
        }
        let rows: Arc<[Metadatum]> = Arc::from(self.read_group(group_index)?);
        let footprint = group_footprint(&rows);
        self.store_in_cache(group_index, rows.clone(), footprint);
        Ok(rows)
    }

    fn store_in_cache(&mut self, group_index: u64, rows: Arc<[Metadatum]>, footprint: usize) {
        debug_assert!(!self.cache.groups.contains_key(&group_index));
        self.cache.used_bytes += footprint;
        self.cache
            .groups
            .insert(group_index, CachedGroup { rows, footprint });
        self.cache.order.push_back(group_index);
        while self.cache.used_bytes > self.cache.max_bytes && self.cache.order.len() > 1 {
            if let Some(oldest) = self.cache.order.pop_front()
                && let Some(removed) = self.cache.groups.remove(&oldest)
            {
                self.cache.used_bytes -= removed.footprint;
            }
        }
    }

    fn read_group(&self, group_index: u64) -> IonResult<Vec<Metadatum>> {
        let entry = self
            .directory
            .get(group_index as usize)
            .ok_or_else(|| IonError::from("metadata groups: group index out of range"))?;
        let payload = self.source.read(self.payload_range(entry)?)?;
        let bytes = self.decode_payload(entry, &payload)?;
        self.parse_group(&bytes, group_index)
    }

    fn payload_range(&self, entry: &MetaGroupEntry) -> IonResult<ByteRange> {
        let end = entry
            .payload_offset
            .checked_add(entry.payload_size)
            .ok_or_else(|| IonError::from("metadata groups: payload overflows"))?;
        if end > self.payload_end {
            return Err(IonError::from(
                "metadata groups: payload overlaps the directory",
            ));
        }
        Ok(ByteRange {
            offset: self.section.range.offset + entry.payload_offset,
            length: entry.payload_size,
        })
    }

    fn decode_payload<'a>(
        &self,
        entry: &MetaGroupEntry,
        payload: &'a [u8],
    ) -> IonResult<Cow<'a, [u8]>> {
        if self.verify_checksums && crc32fast::hash(payload) != entry.checksum {
            return Err(IonError::from("metadata groups: group checksum mismatch"));
        }
        match self.compression_codec {
            CODEC_NONE => Ok(Cow::Borrowed(payload)),
            CODEC_ZSTD => {
                let size = usize::try_from(entry.uncompressed_size).map_err(|_| {
                    IonError::from("metadata groups: uncompressed size out of range")
                })?;
                let plain = decompress_zstd_allow_aligned_padding(payload, size, self.budget)?;
                Ok(Cow::Owned(plain))
            }
            other => Err(IonError::from(format!(
                "metadata groups: unsupported codec {other}"
            ))),
        }
    }

    fn parse_group(&self, bytes: &[u8], group_index: u64) -> IonResult<Vec<Metadatum>> {
        let (meta_count, numeric_count, string_count) = read_group_header(bytes)?;
        let (item_start, item_end) =
            item_range_of_group(group_index, self.section.group_size, self.section.item_count);
        let mut rows = parse_metadata(
            &bytes[META_GROUP_HEADER_SIZE..],
            item_end - item_start,
            meta_count as u64,
            numeric_count as u64,
            string_count as u64,
            CODEC_NONE,
            0,
            self.budget,
            self.layout,
        )?;
        let item_base = item_start as u32;
        for row in &mut rows {
            row.item_index += item_base;
        }
        Ok(rows)
    }
}

fn group_footprint(rows: &[Metadatum]) -> usize {
    let mut total = 0;
    for row in rows {
        total += std::mem::size_of::<Metadatum>();
        if let Some(accession) = &row.accession {
            total += accession.len();
        }
        if let Some(unit) = &row.unit_accession {
            total += unit.len();
        }
        if let MetadatumValue::Text(text) = &row.value {
            total += text.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::MetaGroupReader;
    use crate::ion::{
        ByteRange, DecompressionLimit, IonResult,
        decoder::{
            decode::ReadOptions,
            utilities::{
                byte_source::BytesSource, meta_column_layout::MetaColumnLayout,
            },
        },
        format::CODEC_NONE,
        meta_groups::{MetaSection, MetaTotals},
    };

    fn no_totals() -> MetaTotals {
        MetaTotals {
            rows: 0,
            numeric: 0,
            string: 0,
            uncompressed: 0,
        }
    }

    fn new_reader(group_count: u64, group_size: u32, item_count: u64) -> IonResult<()> {
        let section = MetaSection {
            range: ByteRange {
                offset: 0,
                length: 0,
            },
            group_count,
            group_size,
            item_count,
            totals: no_totals(),
        };
        MetaGroupReader::open(
            Arc::new(BytesSource::new(Arc::from(&[][..]))),
            section,
            CODEC_NONE,
            &ReadOptions {
                max_cached_bytes: 1024,
                verify_checksums: false,
                parallel: true,
                decompression_limit: DecompressionLimit::default(),
            },
            MetaColumnLayout::new(),
        )
        .map(|_| ())
    }

    #[test]
    fn rejects_zero_group_size_when_items_exist() {
        assert!(new_reader(0, 0, 5).is_err());
    }

    #[test]
    fn rejects_group_count_that_disagrees_with_item_count() {
        assert!(new_reader(2, 8192, 5).is_err());
    }

    #[test]
    fn allows_consistent_empty_section() {
        assert!(new_reader(0, 8192, 0).is_ok());
    }
}
