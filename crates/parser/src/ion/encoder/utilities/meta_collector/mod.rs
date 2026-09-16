pub(crate) mod grouper;
pub(crate) mod schema;

#[cfg(test)]
mod tests;

pub(crate) use grouper::{GroupedSection, MetaGrouper, serialize_global_meta_with_counts};
pub(crate) use schema::MzmlListItem;
#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
use zstd::bulk::compress as zstd_compress;

use crate::{
    accessions::{INTENSITY_ARRAY, MZ_ARRAY, TIME_ARRAY},
    ion::{
        IonResult,
        attr_meta::{
            AccessionTail, CV_CODE_UNKNOWN, CV_REF_ATTR, attr_cv_param, cv_ref_code_from_str,
            parse_accession_tail,
        },
        decoder::decode::MetadatumValue,
        utilities::assign_attributes_into,
    },
    mzml::{
        schema::TagId,
        structs::{BinaryDataArray, CvParam, MzML, ReferenceableParamGroupRef, UserParam},
    },
};

const USER_PARAM_NAME_VALUE_SEPARATOR: char = '\0';
pub(crate) const LOCAL_LIST_NODE_ID: u32 = 1;
pub(crate) const FIRST_LOCAL_ITEM_NODE_ID: u32 = 2;

pub(crate) struct MetaCollector {
    context: TraversalContext,
}

impl MetaCollector {
    pub(crate) fn new() -> Self {
        Self {
            context: TraversalContext::new(),
        }
    }

    pub(crate) fn collect_global_meta(&mut self, mzml: &MzML) -> (PackedMeta, GlobalCounts) {
        schema::pack_global_meta(mzml, &mut self.context)
    }

    pub(crate) fn add_item<T, L>(
        &mut self,
        item: &T,
        item_index: usize,
        list_node_id: u32,
        list_schema: Option<&L>,
        policy: ArrayPolicy,
        metadata_writer: &mut dyn MetadataWriter,
    ) -> IonResult<()>
    where
        T: MzmlListItem,
        L: crate::ion::utilities::EmitAttributes,
    {
        self.context.reset_for_item();
        let mut buffer = MetaParamBuffer::new();
        {
            let mut writer = buffer.as_writer();
            if metadata_writer.is_first_item_in_group()
                && list_node_id != 0
                && let Some(schema) = list_schema
            {
                writer.touch(T::list_tag(), list_node_id, 0);
                writer.push_schema_attrs(T::list_tag(), list_node_id, 0, schema);
            }
            let item_id = self.context.alloc();
            writer.push_schema_attrs(T::item_tag(), item_id, list_node_id, item);
            if !item.has_explicit_index() {
                writer.push_optional_u32_attr(
                    T::item_tag(),
                    item_id,
                    list_node_id,
                    crate::ion::attr_meta::ACC_ATTR_INDEX,
                    Some(item_index as u32),
                );
            }
            writer.push_ref_group_params(item_id, item.group_refs(), &mut self.context);
            writer.push_cv_and_user_params(
                item_id,
                list_node_id,
                item.cv_params(),
                item.user_params(),
            );
            item.flatten_children(&mut writer, item_id, &mut self.context, policy);
        }
        buffer.normalize_attr_cv_values();
        metadata_writer.write_metadata_item(&buffer)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArrayPolicy {
    pub(crate) x_array_accession: u32,
    pub(crate) y_array_accession: u32,
    pub(crate) force_f32: bool,
}

impl ArrayPolicy {
    pub(crate) fn is_xy_array(self, accession: u32) -> bool {
        accession == self.x_array_accession || accession == self.y_array_accession
    }
    pub(crate) fn should_force_f32(self, accession: u32) -> bool {
        self.force_f32 && self.is_xy_array(accession)
    }
}

#[derive(Debug)]
pub struct PackedMeta {
    pub index_offsets: Vec<u32>,
    pub ids: Vec<u32>,
    pub parent_indices: Vec<u32>,
    pub tag_ids: Vec<u8>,
    pub ref_codes: Vec<u8>,
    pub accession_numbers: Vec<u32>,
    pub unit_ref_codes: Vec<u8>,
    pub unit_accession_numbers: Vec<u32>,
    pub value_kinds: Vec<u8>,
    pub value_indices: Vec<u32>,
    pub numeric_values: Vec<f64>,
    pub string_offsets: Vec<u32>,
    pub string_lengths: Vec<u32>,
    pub string_bytes: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct GlobalCounts {
    pub(crate) n_file_description: u32,
    pub(crate) n_run: u32,
    pub(crate) n_ref_param_groups: u32,
    pub(crate) n_samples: u32,
    pub(crate) n_instrument_configs: u32,
    pub(crate) n_software: u32,
    pub(crate) n_data_processing: u32,
    pub(crate) n_acquisition_settings: u32,
    pub(crate) n_cvs: u32,
}

struct IdAllocator(u32);

impl IdAllocator {
    fn new() -> Self {
        Self(1)
    }
    #[inline]
    fn next(&mut self) -> u32 {
        let id = self.0;
        self.0 += 1;
        id
    }
    fn reset_to(&mut self, next: u32) {
        self.0 = next;
    }
}

pub(crate) struct TraversalContext {
    nodes: IdAllocator,
}

impl TraversalContext {
    fn new() -> Self {
        Self {
            nodes: IdAllocator::new(),
        }
    }
    #[inline]
    pub(crate) fn alloc(&mut self) -> u32 {
        self.nodes.next()
    }
    pub(crate) fn reset_for_item(&mut self) {
        self.nodes.reset_to(FIRST_LOCAL_ITEM_NODE_ID);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ValueEncoding {
    pub(crate) kind: u8,
    pub(crate) index: u32,
}

pub(crate) struct ValuePool {
    pub(crate) numeric_values: Vec<f64>,
    pub(crate) string_offsets: Vec<u32>,
    pub(crate) string_lengths: Vec<u32>,
    pub(crate) string_bytes: Vec<u8>,
    numeric_count: u32,
    string_count: u32,
}

impl ValuePool {
    pub(crate) fn new() -> Self {
        Self {
            numeric_values: Vec::new(),
            string_offsets: Vec::new(),
            string_lengths: Vec::new(),
            string_bytes: Vec::new(),
            numeric_count: 0,
            string_count: 0,
        }
    }

    pub(crate) fn encode(&mut self, value: Option<&str>) -> ValueEncoding {
        match value {
            None | Some("") => ValueEncoding { kind: 2, index: 0 },
            Some(text) => {
                let looks_numeric = text.contains('.')
                    || text.contains('e')
                    || text.contains('E')
                    || text.starts_with('-') && text[1..].contains('.');
                if looks_numeric
                    && let Ok(n) = text.parse::<f64>()
                    && n.to_string() == text
                {
                    let index = self.numeric_count;
                    self.numeric_values.push(n);
                    self.numeric_count += 1;
                    return ValueEncoding { kind: 0, index };
                }
                self.store_string(text)
            }
        }
    }

    pub(crate) fn encode_as_string(&mut self, value: Option<&str>) -> ValueEncoding {
        match value {
            None | Some("") => ValueEncoding { kind: 2, index: 0 },
            Some(text) => self.store_string(text),
        }
    }

    fn store_string(&mut self, text: &str) -> ValueEncoding {
        let index = self.string_count;
        let bytes = text.as_bytes();
        self.string_offsets.push(self.string_bytes.len() as u32);
        self.string_lengths.push(bytes.len() as u32);
        self.string_bytes.extend_from_slice(bytes);
        self.string_count += 1;
        ValueEncoding { kind: 1, index }
    }
}

pub(crate) struct MetaParamRow {
    pub(crate) cv_param: CvParam,
    pub(crate) tag_id: u8,
    pub(crate) id: u32,
    pub(crate) parent_id: u32,
}

pub(crate) struct MetaParamBuffer {
    pub(crate) rows: Vec<MetaParamRow>,
}

impl MetaParamBuffer {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::with_capacity(64),
        }
    }

    pub(crate) fn push(&mut self, tag: TagId, id: u32, parent_id: u32, cv_param: CvParam) {
        self.rows.push(MetaParamRow {
            cv_param,
            tag_id: tag as u8,
            id,
            parent_id,
        });
    }

    pub(crate) fn extend_cv_params(
        &mut self,
        tag: TagId,
        id: u32,
        parent_id: u32,
        cv_params: &[CvParam],
    ) {
        self.rows.reserve(cv_params.len());
        for cv_param in cv_params {
            self.rows.push(MetaParamRow {
                cv_param: cv_param.clone(),
                tag_id: tag as u8,
                id,
                parent_id,
            });
        }
    }

    pub(crate) fn extend_user_params(
        &mut self,
        tag: TagId,
        id: u32,
        parent_id: u32,
        user_params: &[UserParam],
    ) {
        self.rows.reserve(user_params.len());
        for user_param in user_params {
            self.rows.push(MetaParamRow {
                cv_param: encode_user_param_as_cv(user_param),
                tag_id: tag as u8,
                id,
                parent_id,
            });
        }
    }

    pub(crate) fn as_writer(&mut self) -> MetaParamWriter<'_> {
        MetaParamWriter { buffer: self }
    }

    pub(crate) fn normalize_attr_cv_values(&mut self) {
        for row in &mut self.rows {
            if row.cv_param.cv_ref.as_deref() == Some(CV_REF_ATTR) {
                let absent = row.cv_param.value.as_deref().is_none_or(str::is_empty);
                if absent && !row.cv_param.name.is_empty() {
                    row.cv_param.value = Some(std::mem::take(&mut row.cv_param.name));
                }
            }
        }
    }
}

pub(crate) struct MetaParamWriter<'b> {
    buffer: &'b mut MetaParamBuffer,
}

impl<'b> MetaParamWriter<'b> {
    pub(crate) fn push_one(&mut self, tag: TagId, id: u32, parent_id: u32, cv_param: CvParam) {
        self.buffer.push(tag, id, parent_id, cv_param);
    }
    pub(crate) fn push_many(&mut self, tag: TagId, id: u32, parent_id: u32, cv_params: &[CvParam]) {
        self.buffer.extend_cv_params(tag, id, parent_id, cv_params);
    }
    fn push_user_params(&mut self, tag: TagId, id: u32, parent_id: u32, user_params: &[UserParam]) {
        self.buffer
            .extend_user_params(tag, id, parent_id, user_params);
    }
    pub(crate) fn touch(&mut self, tag: TagId, id: u32, parent_id: u32) {
        self.push_one(tag, id, parent_id, empty_cv_param());
    }
    pub(crate) fn push_str_attr(
        &mut self,
        tag: TagId,
        id: u32,
        parent_id: u32,
        tail: AccessionTail,
        value: &str,
    ) {
        if !value.is_empty() {
            self.push_one(tag, id, parent_id, attr_cv_param(tail, value));
        }
    }
    pub(crate) fn push_optional_u32_attr(
        &mut self,
        tag: TagId,
        id: u32,
        parent_id: u32,
        tail: AccessionTail,
        value: Option<u32>,
    ) {
        if let Some(n) = value {
            self.push_one(tag, id, parent_id, attr_cv_param(tail, &n.to_string()));
        }
    }
    pub(crate) fn push_cv_and_user_params(
        &mut self,
        id: u32,
        parent_id: u32,
        cv_params: &[CvParam],
        user_params: &[UserParam],
    ) {
        self.push_many(TagId::CvParam, id, parent_id, cv_params);
        self.push_user_params(TagId::UserParam, id, parent_id, user_params);
    }

    pub(crate) fn push_ref_group_params(
        &mut self,
        id: u32,
        group_refs: &[ReferenceableParamGroupRef],
        context: &mut TraversalContext,
    ) {
        for gr in group_refs {
            let ref_id = context.alloc();
            self.touch(TagId::ReferenceableParamGroupRef, ref_id, id);
            self.push_str_attr(
                TagId::ReferenceableParamGroupRef,
                ref_id,
                id,
                crate::ion::attr_meta::ACC_ATTR_REF,
                &gr.r#ref,
            );
        }
    }
    pub(crate) fn push_schema_attrs<T: crate::ion::utilities::EmitAttributes>(
        &mut self,
        tag: TagId,
        id: u32,
        parent_id: u32,
        schema_value: &T,
    ) {
        let mut attrs = Vec::new();
        assign_attributes_into(schema_value, tag, id, parent_id, &mut attrs);
        for attr in attrs {
            let tail_raw = parse_accession_tail_raw(attr.accession.as_deref());
            if tail_raw == 0 {
                continue;
            }
            let text = match attr.value {
                MetadatumValue::Text(t) => t,
                MetadatumValue::Number(n) => n.to_string(),
                MetadatumValue::Empty => continue,
            };
            if text.is_empty() {
                continue;
            }
            self.push_one(
                tag,
                id,
                parent_id,
                attr_cv_param(AccessionTail::from_raw(tail_raw), &text),
            );
        }
    }
}

pub(crate) struct PackedMetaBuilder {
    index_offsets: Vec<u32>,
    ids: Vec<u32>,
    parent_indices: Vec<u32>,
    tag_ids: Vec<u8>,
    ref_codes: Vec<u8>,
    accession_numbers: Vec<u32>,
    unit_ref_codes: Vec<u8>,
    unit_accession_numbers: Vec<u32>,
    value_kinds: Vec<u8>,
    value_indices: Vec<u32>,
    value_pool: ValuePool,
    row_count: u32,
}

impl PackedMetaBuilder {
    pub(crate) fn new() -> Self {
        let mut b = Self {
            index_offsets: Vec::new(),
            ids: Vec::new(),
            parent_indices: Vec::new(),
            tag_ids: Vec::new(),
            ref_codes: Vec::new(),
            accession_numbers: Vec::new(),
            unit_ref_codes: Vec::new(),
            unit_accession_numbers: Vec::new(),
            value_kinds: Vec::new(),
            value_indices: Vec::new(),
            value_pool: ValuePool::new(),
            row_count: 0,
        };
        b.index_offsets.push(0);
        b
    }

    pub(crate) fn flush_buffer(&mut self, buffer: &MetaParamBuffer) {
        for row in &buffer.rows {
            self.push_row(row.tag_id, row.id, row.parent_id, &row.cv_param);
        }
        self.end_item();
    }

    fn push_row(&mut self, tag_id: u8, id: u32, parent_id: u32, cv_param: &CvParam) {
        self.tag_ids.push(tag_id);
        self.ids.push(id);
        self.parent_indices.push(parent_id);

        let cv_ref = cv_ref_prefix_from_accession(cv_param.accession.as_deref())
            .or(cv_param.cv_ref.as_deref());
        self.ref_codes.push(cv_ref_code_from_str(cv_ref));
        self.accession_numbers
            .push(parse_accession_tail_raw(cv_param.accession.as_deref()));

        let unit_ref = cv_ref_prefix_from_accession(cv_param.unit_accession.as_deref())
            .or(cv_param.unit_cv_ref.as_deref());
        self.unit_ref_codes.push(cv_ref_code_from_str(unit_ref));
        self.unit_accession_numbers
            .push(parse_accession_tail_raw(cv_param.unit_accession.as_deref()));

        let is_attr = cv_param.cv_ref.as_deref() == Some(CV_REF_ATTR)
            || cv_ref_prefix_from_accession(cv_param.accession.as_deref()) == Some(CV_REF_ATTR);

        let enc = if is_attr {
            self.value_pool.encode_as_string(cv_param.value.as_deref())
        } else {
            self.value_pool.encode(cv_param.value.as_deref())
        };

        self.value_kinds.push(enc.kind);
        self.value_indices.push(enc.index);
        self.row_count += 1;
    }

    fn end_item(&mut self) {
        self.index_offsets.push(self.row_count);
    }

    pub(crate) fn build(self) -> PackedMeta {
        PackedMeta {
            index_offsets: self.index_offsets,
            ids: self.ids,
            parent_indices: self.parent_indices,
            tag_ids: self.tag_ids,
            ref_codes: self.ref_codes,
            accession_numbers: self.accession_numbers,
            unit_ref_codes: self.unit_ref_codes,
            unit_accession_numbers: self.unit_accession_numbers,
            value_kinds: self.value_kinds,
            value_indices: self.value_indices,
            numeric_values: self.value_pool.numeric_values,
            string_offsets: self.value_pool.string_offsets,
            string_lengths: self.value_pool.string_lengths,
            string_bytes: self.value_pool.string_bytes,
        }
    }
}

pub(crate) fn empty_cv_param() -> CvParam {
    CvParam {
        cv_ref: None,
        accession: None,
        name: String::new(),
        value: None,
        unit_cv_ref: None,
        unit_accession: None,
        unit_name: None,
    }
}

pub(crate) fn encode_user_param_as_cv(p: &UserParam) -> CvParam {
    let value = p.value.as_deref().unwrap_or("");
    let type_ = p.r#type.as_deref().unwrap_or("");
    let encoded = format!(
        "{}{}{}{}{}",
        p.name, USER_PARAM_NAME_VALUE_SEPARATOR, value, USER_PARAM_NAME_VALUE_SEPARATOR, type_
    );
    CvParam {
        cv_ref: None,
        accession: None,
        name: String::new(),
        value: Some(encoded),
        unit_cv_ref: p.unit_cv_ref.clone(),
        unit_name: p.unit_name.clone(),
        unit_accession: p.unit_accession.clone(),
    }
}

#[inline]
pub(crate) fn parse_accession_tail_raw(accession: Option<&str>) -> u32 {
    parse_accession_tail(accession).raw()
}

pub(crate) fn cv_ref_prefix_from_accession(accession: Option<&str>) -> Option<&str> {
    accession.and_then(|s| s.split_once(':').map(|(prefix, _)| prefix))
}

fn array_type_cv_param(bda: &BinaryDataArray) -> Option<&CvParam> {
    for cv in &bda.cv_params {
        let t = parse_accession_tail_raw(cv.accession.as_deref());
        if matches!(t, MZ_ARRAY | INTENSITY_ARRAY | TIME_ARRAY) {
            return Some(cv);
        }
    }
    for cv in &bda.cv_params {
        let t = parse_accession_tail_raw(cv.accession.as_deref());
        if t != 0 && cv.name.to_ascii_lowercase().contains(" array") {
            return Some(cv);
        }
    }
    None
}

pub(crate) fn array_type_accession_from_binary_data_array(bda: &BinaryDataArray) -> u32 {
    array_type_cv_param(bda)
        .map(|cv| parse_accession_tail_raw(cv.accession.as_deref()))
        .unwrap_or(0)
}

pub(crate) fn array_type_cv_code_from_binary_data_array(bda: &BinaryDataArray) -> u8 {
    array_type_cv_param(bda)
        .map(|cv| cv_ref_code_from_str(cv.cv_ref.as_deref()))
        .unwrap_or(CV_CODE_UNKNOWN)
}

pub(crate) fn append_meta_buffer(
    buffers: &mut Vec<MetaParamBuffer>,
    fill: impl FnOnce(&mut MetaParamWriter<'_>),
) {
    let mut buffer = MetaParamBuffer::new();
    fill(&mut buffer.as_writer());
    buffer.normalize_attr_cv_values();
    buffers.push(buffer);
}

pub(crate) trait MetadataWriter {
    fn write_metadata_item(&mut self, buffer: &MetaParamBuffer) -> IonResult<()>;
    fn is_first_item_in_group(&self) -> bool {
        false
    }
}

impl MetadataWriter for PackedMetaBuilder {
    fn write_metadata_item(&mut self, buffer: &MetaParamBuffer) -> IonResult<()> {
        self.flush_buffer(buffer);
        Ok(())
    }
}

#[cfg(not(all(target_arch = "wasm32", not(target_os = "wasi"))))]
pub(crate) fn compress_bytes_if_enabled(bytes: Vec<u8>, level: u8) -> Vec<u8> {
    if level == 0 {
        bytes
    } else {
        zstd_compress(&bytes, level as i32).expect("zstd compression failed")
    }
}

#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
pub(crate) fn compress_bytes_if_enabled(bytes: Vec<u8>, level: u8) -> Vec<u8> {
    if level == 0 {
        return bytes;
    }
    let options = osmo::CompressOptions::zstd();
    let mut out = vec![0u8; osmo::get_max_compressed_size(bytes.len(), &options)];
    let mut workspace = osmo::EncodeWorkspace::new_boxed();
    let written = osmo::compress(&bytes, &mut out, &options, &mut workspace)
        .expect("zstd compression failed");
    out.truncate(written);
    out
}
