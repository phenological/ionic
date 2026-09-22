use crate::{
    ion::{
        attr_meta::{
            ACC_ATTR_COUNT, ACC_ATTR_DATA_PROCESSING_REF, ACC_ATTR_DEFAULT_ARRAY_LENGTH,
            ACC_ATTR_DEFAULT_DATA_PROCESSING_REF, ACC_ATTR_EXTERNAL_SPECTRUM_ID, ACC_ATTR_ID,
            ACC_ATTR_INDEX, ACC_ATTR_NATIVE_ID, ACC_ATTR_REF, ACC_ATTR_SOURCE_FILE_REF,
            ACC_ATTR_SPECTRUM_REF,
        },
        decoder::decode::Metadatum,
        utilities::{
            children_lookup::{ChildrenLookup, MetadataPolicy, OwnerRows},
            common::{get_attr_text, get_attr_u32},
            parse_binary_data_array_list::parse_binary_data_array_list,
            parse_cv_and_user_params,
        },
    },
    mzml::{
        schema::TagId,
        structs::{
            Activation, Chromatogram, ChromatogramList, IsolationWindow, Precursor, Product,
            ReferenceableParamGroupRef, SelectedIon, SelectedIonList,
        },
    },
};

#[inline]
pub(crate) fn parse_chromatogram_list<P: MetadataPolicy>(
    metadata: &[&Metadatum],
    children_lookup: &ChildrenLookup,
    policy: &P,
    index_base: u32,
) -> Option<ChromatogramList> {
    let mut owner_rows = OwnerRows::with_capacity(metadata.len());
    for &entry in metadata {
        owner_rows.insert(entry.id, entry);
    }

    let (mut list, chromatogram_ids) = parse_chromatogram_list_header(&owner_rows, children_lookup)?;

    let mut param_buffer: Vec<&Metadatum> = Vec::new();

    list.chromatograms = chromatogram_ids
        .iter()
        .enumerate()
        .map(|(index, &chromatogram_id)| {
            parse_chromatogram(
                &owner_rows,
                children_lookup,
                chromatogram_id,
                index_base + index as u32,
                policy,
                &mut param_buffer,
            )
        })
        .collect();

    Some(list)
}

pub(crate) fn parse_chromatogram_list_header<'l>(
    owner_rows: &OwnerRows,
    children_lookup: &'l ChildrenLookup,
) -> Option<(ChromatogramList, &'l [u32])> {
    let list_id = children_lookup
        .all_ids(TagId::ChromatogramList)
        .first()
        .copied()?;

    let chromatogram_ids = children_lookup.ids_for(list_id, TagId::Chromatogram);
    if chromatogram_ids.is_empty() {
        return None;
    }

    let list_rows = owner_rows.get(list_id);
    let list = ChromatogramList {
        count: get_attr_u32(list_rows, ACC_ATTR_COUNT)
            .map(|v| v as usize)
            .or(Some(chromatogram_ids.len())),
        default_data_processing_ref: get_attr_text(
            list_rows,
            ACC_ATTR_DEFAULT_DATA_PROCESSING_REF,
        ),
        chromatograms: Vec::new(),
    };
    Some((list, chromatogram_ids))
}

#[inline]
pub(crate) fn parse_chromatogram<'a, P: MetadataPolicy>(
    owner_rows: &'a OwnerRows<'a>,
    children_lookup: &ChildrenLookup,
    chromatogram_id: u32,
    fallback_index: u32,
    policy: &P,
    param_buffer: &mut Vec<&'a Metadatum>,
) -> Chromatogram {
    let rows = owner_rows.get(chromatogram_id);

    param_buffer.clear();
    children_lookup.get_param_rows_into(owner_rows, chromatogram_id, policy, param_buffer);
    let (cv_params, user_params) = parse_cv_and_user_params(param_buffer);

    let binary_data_array_list =
        parse_binary_data_array_list(owner_rows, children_lookup, chromatogram_id);

    Chromatogram {
        id: get_attr_text(rows, ACC_ATTR_ID).unwrap_or_default(),
        native_id: get_attr_text(rows, ACC_ATTR_NATIVE_ID),
        index: get_attr_u32(rows, ACC_ATTR_INDEX).or(Some(fallback_index)),
        default_array_length: get_attr_u32(rows, ACC_ATTR_DEFAULT_ARRAY_LENGTH).map(|v| v as usize),
        data_processing_ref: get_attr_text(rows, ACC_ATTR_DATA_PROCESSING_REF),
        referenceable_param_group_refs: children_lookup
            .ids_for(chromatogram_id, TagId::ReferenceableParamGroupRef)
            .iter()
            .filter_map(|&ref_id| {
                get_attr_text(owner_rows.get(ref_id), ACC_ATTR_REF)
                    .map(|r| ReferenceableParamGroupRef { r#ref: r })
            })
            .collect(),
        cv_params,
        user_params,
        precursor: parse_precursor(
            owner_rows,
            children_lookup,
            chromatogram_id,
            policy,
            param_buffer,
        ),
        product: parse_product(
            owner_rows,
            children_lookup,
            chromatogram_id,
            policy,
            param_buffer,
        ),
        binary_data_array_list,
    }
}

#[inline]
fn parse_precursor<'a, P: MetadataPolicy>(
    owner_rows: &'a OwnerRows<'a>,
    children_lookup: &ChildrenLookup,
    parent_id: u32,
    policy: &P,
    param_buffer: &mut Vec<&'a Metadatum>,
) -> Option<Precursor> {
    let precursor_id = children_lookup
        .ids_for(parent_id, TagId::Precursor)
        .first()
        .copied()?;

    let rows = owner_rows.get(precursor_id);

    Some(Precursor {
        spectrum_ref: get_attr_text(rows, ACC_ATTR_SPECTRUM_REF),
        source_file_ref: get_attr_text(rows, ACC_ATTR_SOURCE_FILE_REF),
        external_spectrum_id: get_attr_text(rows, ACC_ATTR_EXTERNAL_SPECTRUM_ID),
        isolation_window: parse_isolation_window(
            owner_rows,
            children_lookup,
            precursor_id,
            policy,
            param_buffer,
        ),
        selected_ion_list: parse_selected_ion_list(
            owner_rows,
            children_lookup,
            precursor_id,
            policy,
            param_buffer,
        ),
        activation: parse_activation(
            owner_rows,
            children_lookup,
            precursor_id,
            policy,
            param_buffer,
        ),
    })
}

#[inline]
fn parse_product<'a, P: MetadataPolicy>(
    owner_rows: &'a OwnerRows<'a>,
    children_lookup: &ChildrenLookup,
    parent_id: u32,
    policy: &P,
    param_buffer: &mut Vec<&'a Metadatum>,
) -> Option<Product> {
    let product_id = children_lookup
        .ids_for(parent_id, TagId::Product)
        .first()
        .copied()?;

    let rows = owner_rows.get(product_id);

    param_buffer.clear();
    children_lookup.get_param_rows_into(owner_rows, product_id, policy, param_buffer);
    let (cv_params, user_params) = parse_cv_and_user_params(param_buffer);

    Some(Product {
        spectrum_ref: get_attr_text(rows, ACC_ATTR_SPECTRUM_REF),
        source_file_ref: get_attr_text(rows, ACC_ATTR_SOURCE_FILE_REF),
        external_spectrum_id: get_attr_text(rows, ACC_ATTR_EXTERNAL_SPECTRUM_ID),
        isolation_window: parse_isolation_window(
            owner_rows,
            children_lookup,
            product_id,
            policy,
            param_buffer,
        ),
        cv_params,
        user_params,
    })
}

#[inline]
fn parse_isolation_window<'a, P: MetadataPolicy>(
    owner_rows: &'a OwnerRows<'a>,
    children_lookup: &ChildrenLookup,
    parent_id: u32,
    policy: &P,
    param_buffer: &mut Vec<&'a Metadatum>,
) -> Option<IsolationWindow> {
    let isolation_window_id = children_lookup
        .ids_for(parent_id, TagId::IsolationWindow)
        .first()
        .copied()?;

    param_buffer.clear();
    children_lookup.get_param_rows_into(owner_rows, isolation_window_id, policy, param_buffer);
    let (cv_params, user_params) = parse_cv_and_user_params(param_buffer);

    Some(IsolationWindow {
        referenceable_param_group_refs: children_lookup
            .ids_for(isolation_window_id, TagId::ReferenceableParamGroupRef)
            .iter()
            .filter_map(|&ref_id| {
                get_attr_text(owner_rows.get(ref_id), ACC_ATTR_REF)
                    .map(|r| ReferenceableParamGroupRef { r#ref: r })
            })
            .collect(),
        cv_params,
        user_params,
    })
}

#[inline]
fn parse_activation<'a, P: MetadataPolicy>(
    owner_rows: &'a OwnerRows<'a>,
    children_lookup: &ChildrenLookup,
    parent_id: u32,
    policy: &P,
    param_buffer: &mut Vec<&'a Metadatum>,
) -> Option<Activation> {
    let activation_id = children_lookup
        .ids_for(parent_id, TagId::Activation)
        .first()
        .copied()?;

    param_buffer.clear();
    children_lookup.get_param_rows_into(owner_rows, activation_id, policy, param_buffer);
    let (cv_params, user_params) = parse_cv_and_user_params(param_buffer);

    Some(Activation {
        referenceable_param_group_refs: children_lookup
            .ids_for(activation_id, TagId::ReferenceableParamGroupRef)
            .iter()
            .filter_map(|&ref_id| {
                get_attr_text(owner_rows.get(ref_id), ACC_ATTR_REF)
                    .map(|r| ReferenceableParamGroupRef { r#ref: r })
            })
            .collect(),
        cv_params,
        user_params,
    })
}

#[inline]
fn parse_selected_ion_list<'a, P: MetadataPolicy>(
    owner_rows: &'a OwnerRows<'a>,
    children_lookup: &ChildrenLookup,
    precursor_id: u32,
    policy: &P,
    param_buffer: &mut Vec<&'a Metadatum>,
) -> Option<SelectedIonList> {
    let selected_ion_list_id = children_lookup
        .ids_for(precursor_id, TagId::SelectedIonList)
        .first()
        .copied()?;

    let selected_ions = children_lookup
        .ids_for(selected_ion_list_id, TagId::SelectedIon)
        .iter()
        .map(|&selected_ion_id| {
            param_buffer.clear();
            children_lookup.get_param_rows_into(owner_rows, selected_ion_id, policy, param_buffer);
            let (cv_params, user_params) = parse_cv_and_user_params(param_buffer);

            SelectedIon {
                referenceable_param_group_refs: children_lookup
                    .ids_for(selected_ion_id, TagId::ReferenceableParamGroupRef)
                    .iter()
                    .filter_map(|&ref_id| {
                        get_attr_text(owner_rows.get(ref_id), ACC_ATTR_REF)
                            .map(|r| ReferenceableParamGroupRef { r#ref: r })
                    })
                    .collect(),
                cv_params,
                user_params,
            }
        })
        .collect::<Vec<_>>();

    Some(SelectedIonList {
        count: Some(selected_ions.len()),
        selected_ions,
    })
}
