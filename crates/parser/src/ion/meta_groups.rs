use crate::ion::{ByteRange, IonError, IonResult};

pub(crate) const METADATA_GROUP_SIZE: u32 = 8192;
pub(crate) const META_GROUP_ENTRY_SIZE: usize = 32;
pub(crate) const META_GROUP_HEADER_SIZE: usize = 12;

pub(crate) struct MetaGroupEntry {
    pub(crate) payload_offset: u64,
    pub(crate) payload_size: u64,
    pub(crate) uncompressed_size: u64,
    pub(crate) checksum: u32,
}

impl MetaGroupEntry {
    pub(crate) fn write_into(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.payload_offset.to_le_bytes());
        out.extend_from_slice(&self.payload_size.to_le_bytes());
        out.extend_from_slice(&self.uncompressed_size.to_le_bytes());
        out.extend_from_slice(&self.checksum.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
    }

    pub(crate) fn read_from(bytes: &[u8]) -> Self {
        Self {
            payload_offset: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            payload_size: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            uncompressed_size: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            checksum: u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        }
    }
}

pub(crate) struct MetaTotals {
    pub(crate) rows: u64,
    pub(crate) numeric: u64,
    pub(crate) string: u64,
    pub(crate) uncompressed: u64,
}

pub(crate) struct MetaSection {
    pub(crate) range: ByteRange,
    pub(crate) group_count: u64,
    pub(crate) group_size: u32,
    pub(crate) item_count: u64,
    pub(crate) totals: MetaTotals,
}

pub(crate) fn meta_directory_range(section: ByteRange, group_count: u64) -> IonResult<ByteRange> {
    let length = group_count
        .checked_mul(META_GROUP_ENTRY_SIZE as u64)
        .ok_or_else(|| IonError::from("metadata groups: directory size overflows"))?;
    let offset = section
        .length
        .checked_sub(length)
        .ok_or_else(|| IonError::from("metadata groups: section smaller than directory"))?;
    Ok(ByteRange {
        offset: section.offset + offset,
        length,
    })
}

pub(crate) fn group_count_for(item_count: u64, group_size: u32) -> u64 {
    let group_size = group_size as u64;
    if group_size == 0 || item_count == 0 {
        return 0;
    }
    item_count.div_ceil(group_size)
}

pub(crate) fn group_of_item(item_index: u64, group_size: u32) -> u64 {
    item_index / group_size as u64
}

pub(crate) fn item_range_of_group(
    group_index: u64,
    group_size: u32,
    item_count: u64,
) -> (u64, u64) {
    let group_size = group_size as u64;
    let start = (group_index * group_size).min(item_count);
    let end = ((group_index + 1) * group_size).min(item_count);
    (start, end)
}

pub(crate) fn write_group_header(
    out: &mut Vec<u8>,
    meta_count: u32,
    numeric_count: u32,
    string_count: u32,
) {
    out.extend_from_slice(&meta_count.to_le_bytes());
    out.extend_from_slice(&numeric_count.to_le_bytes());
    out.extend_from_slice(&string_count.to_le_bytes());
}

pub(crate) fn read_group_header(bytes: &[u8]) -> IonResult<(u32, u32, u32)> {
    if bytes.len() < META_GROUP_HEADER_SIZE {
        return Err(IonError::from(
            "metadata group: payload smaller than group header",
        ));
    }
    let meta_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let numeric_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let string_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    Ok((meta_count, numeric_count, string_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_count_rounds_up() {
        assert_eq!(group_count_for(0, 8192), 0);
        assert_eq!(group_count_for(1, 8192), 1);
        assert_eq!(group_count_for(8192, 8192), 1);
        assert_eq!(group_count_for(8193, 8192), 2);
        assert_eq!(group_count_for(20000, 8192), 3);
    }

    #[test]
    fn item_range_is_clamped_to_item_count() {
        assert_eq!(item_range_of_group(0, 8192, 20000), (0, 8192));
        assert_eq!(item_range_of_group(1, 8192, 20000), (8192, 16384));
        assert_eq!(item_range_of_group(2, 8192, 20000), (16384, 20000));
    }

    #[test]
    fn item_lands_in_its_group() {
        assert_eq!(group_of_item(0, 8192), 0);
        assert_eq!(group_of_item(8191, 8192), 0);
        assert_eq!(group_of_item(8192, 8192), 1);
        assert_eq!(group_of_item(20000, 8192), 2);
    }

    #[test]
    fn entry_round_trips_through_bytes() {
        let entry = MetaGroupEntry {
            payload_offset: 0x0102030405060708,
            payload_size: 42,
            uncompressed_size: 1000,
            checksum: 0xABCDEF01,
        };
        let mut bytes = Vec::new();
        entry.write_into(&mut bytes);
        assert_eq!(bytes.len(), META_GROUP_ENTRY_SIZE);
        let read = MetaGroupEntry::read_from(&bytes);
        assert_eq!(read.payload_offset, entry.payload_offset);
        assert_eq!(read.payload_size, entry.payload_size);
        assert_eq!(read.uncompressed_size, entry.uncompressed_size);
        assert_eq!(read.checksum, entry.checksum);
    }

    #[test]
    fn group_header_round_trips() {
        let mut bytes = Vec::new();
        write_group_header(&mut bytes, 7, 3, 2);
        assert_eq!(bytes.len(), META_GROUP_HEADER_SIZE);
        assert_eq!(read_group_header(&bytes).unwrap(), (7, 3, 2));
    }
}
