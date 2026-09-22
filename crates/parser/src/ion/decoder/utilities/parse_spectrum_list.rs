use crate::{
    accessions::ACC_MS_LEVEL,
    ion::{
        attr_meta::{
            ACC_ATTR_COUNT, ACC_ATTR_DATA_PROCESSING_REF, ACC_ATTR_DEFAULT_ARRAY_LENGTH,
            ACC_ATTR_DEFAULT_DATA_PROCESSING_REF, ACC_ATTR_ID, ACC_ATTR_INDEX, ACC_ATTR_MS_LEVEL,
            ACC_ATTR_NATIVE_ID, ACC_ATTR_REF, ACC_ATTR_SCAN_NUMBER, ACC_ATTR_SOURCE_FILE_REF,
            ACC_ATTR_SPOT_ID,
        },
        decoder::decode::Metadatum,
        utilities::{
            children_lookup::{ChildrenLookup, MetadataPolicy, OwnerRows},
            common::{get_attr_text, get_attr_u32},
            parse_binary_data_array_list, parse_cv_and_user_params, parse_precursor_list,
            parse_product_list, parse_scan_list,
        },
    },
    mzml::{
        schema::TagId,
        structs::{
            CvParam, ReferenceableParamGroupRef, Spectrum, SpectrumDescription, SpectrumList,
        },
    },
};

#[inline]
pub(crate) fn parse_spectrum_list<P: MetadataPolicy>(
    metadata: &[&Metadatum],
    children_lookup: &ChildrenLookup,
    policy: &P,
    index_base: u32,
) -> Option<SpectrumList> {
    let mut owner_rows = OwnerRows::with_capacity(metadata.len());
    for &entry in metadata {
        owner_rows.insert(entry.id, entry);
    }

    let (mut list, spectrum_ids) = parse_spectrum_list_header(&owner_rows, children_lookup)?;

    let mut param_buffer: Vec<&Metadatum> = Vec::new();

    list.spectra = spectrum_ids
        .iter()
        .enumerate()
        .map(|(index, &spectrum_id)| {
            parse_spectrum(
                &owner_rows,
                children_lookup,
                spectrum_id,
                index_base + index as u32,
                policy,
                &mut param_buffer,
            )
        })
        .collect();

    Some(list)
}

pub(crate) fn parse_spectrum_list_header<'l>(
    owner_rows: &OwnerRows,
    children_lookup: &'l ChildrenLookup,
) -> Option<(SpectrumList, &'l [u32])> {
    let list_id = children_lookup
        .all_ids(TagId::SpectrumList)
        .first()
        .copied()?;

    let spectrum_ids = children_lookup.ids_for(list_id, TagId::Spectrum);
    if spectrum_ids.is_empty() {
        return None;
    }

    let list_rows = owner_rows.get(list_id);
    let list = SpectrumList {
        count: get_attr_u32(list_rows, ACC_ATTR_COUNT)
            .map(|v| v as usize)
            .or(Some(spectrum_ids.len())),
        default_data_processing_ref: get_attr_text(
            list_rows,
            ACC_ATTR_DEFAULT_DATA_PROCESSING_REF,
        ),
        spectra: Vec::new(),
    };
    Some((list, spectrum_ids))
}

#[inline]
pub(crate) fn parse_spectrum<'a, P: MetadataPolicy>(
    owner_rows: &'a OwnerRows<'a>,
    children_lookup: &ChildrenLookup,
    spectrum_id: u32,
    fallback_index: u32,
    policy: &P,
    param_buffer: &mut Vec<&'a Metadatum>,
) -> Spectrum {
    let rows = owner_rows.get(spectrum_id);

    param_buffer.clear();
    children_lookup.get_param_rows_into(owner_rows, spectrum_id, policy, param_buffer);
    let (mut cv_params, mut user_params) = parse_cv_and_user_params(param_buffer);

    let spectrum_description = parse_description(
        owner_rows,
        children_lookup,
        spectrum_id,
        policy,
        param_buffer,
    );

    if let Some(description) = &spectrum_description {
        cv_params.retain(|p| {
            !description
                .cv_params
                .iter()
                .any(|dp| p.accession == dp.accession)
        });
        user_params.retain(|p| !description.user_params.iter().any(|dp| p.name == dp.name));
    }

    let ms_level = get_attr_u32(rows, ACC_ATTR_MS_LEVEL)
        .or_else(|| get_ms_level(&cv_params))
        .or_else(|| {
            spectrum_description
                .as_ref()
                .and_then(|description| get_ms_level(&description.cv_params))
        });

    let binary_data_array_list =
        parse_binary_data_array_list(owner_rows, children_lookup, spectrum_id);

    Spectrum {
        id: get_attr_text(rows, ACC_ATTR_ID).unwrap_or_default(),
        index: get_attr_u32(rows, ACC_ATTR_INDEX).or(Some(fallback_index)),
        scan_number: get_attr_u32(rows, ACC_ATTR_SCAN_NUMBER),
        default_array_length: get_attr_u32(rows, ACC_ATTR_DEFAULT_ARRAY_LENGTH).map(|v| v as usize),
        native_id: get_attr_text(rows, ACC_ATTR_NATIVE_ID),
        data_processing_ref: get_attr_text(rows, ACC_ATTR_DATA_PROCESSING_REF),
        source_file_ref: get_attr_text(rows, ACC_ATTR_SOURCE_FILE_REF),
        spot_id: get_attr_text(rows, ACC_ATTR_SPOT_ID),
        ms_level,
        referenceable_param_group_refs: children_lookup
            .ids_for(spectrum_id, TagId::ReferenceableParamGroupRef)
            .iter()
            .filter_map(|&ref_id| {
                get_attr_text(owner_rows.get(ref_id), ACC_ATTR_REF)
                    .map(|r| ReferenceableParamGroupRef { r#ref: r })
            })
            .collect(),
        cv_params,
        user_params,
        spectrum_description,
        scan_list: parse_scan_list(owner_rows, children_lookup, spectrum_id),
        precursor_list: parse_precursor_list(owner_rows, children_lookup, spectrum_id),
        product_list: parse_product_list(owner_rows, children_lookup, spectrum_id),
        binary_data_array_list,
    }
}

fn get_ms_level(cv_params: &[CvParam]) -> Option<u32> {
    cv_params
        .iter()
        .find(|param| param.accession.as_deref() == Some(ACC_MS_LEVEL))
        .and_then(|param| param.value.as_deref())
        .and_then(|value| value.parse::<u32>().ok())
}

#[inline]
fn parse_description<'a, P: MetadataPolicy>(
    owner_rows: &'a OwnerRows<'a>,
    children_lookup: &ChildrenLookup,
    spectrum_id: u32,
    policy: &P,
    param_buffer: &mut Vec<&'a Metadatum>,
) -> Option<SpectrumDescription> {
    let description_id = children_lookup
        .ids_for(spectrum_id, TagId::SpectrumDescription)
        .first()
        .copied()?;

    param_buffer.clear();
    children_lookup.get_param_rows_into(owner_rows, description_id, policy, param_buffer);
    let (cv_params, user_params) = parse_cv_and_user_params(param_buffer);

    Some(SpectrumDescription {
        referenceable_param_group_refs: children_lookup
            .ids_for(description_id, TagId::ReferenceableParamGroupRef)
            .iter()
            .filter_map(|&ref_id| {
                get_attr_text(owner_rows.get(ref_id), ACC_ATTR_REF)
                    .map(|r| ReferenceableParamGroupRef { r#ref: r })
            })
            .collect(),
        cv_params,
        user_params,
        scan_list: parse_scan_list(owner_rows, children_lookup, description_id),
        precursor_list: parse_precursor_list(owner_rows, children_lookup, description_id),
        product_list: parse_product_list(owner_rows, children_lookup, description_id),
    })
}
