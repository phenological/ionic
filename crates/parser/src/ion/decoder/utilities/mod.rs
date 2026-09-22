pub mod byte_source;
pub(crate) use crate::ion::header::{Header, check_section_layout};
pub(crate) mod common;
pub(crate) mod meta_column_layout;
pub(crate) mod meta_group_reader;
pub(crate) mod parse_metadata;
pub(crate) use meta_group_reader::MetaGroupReader;
pub(crate) mod parse_binary_data_array_list;
pub(crate) use parse_binary_data_array_list::parse_binary_data_array_list;
pub(crate) mod parse_cv_and_user_params;
pub(crate) use parse_cv_and_user_params::parse_cv_and_user_params;

pub(crate) mod parse_scan_list;
pub(crate) use parse_scan_list::parse_scan_list;
pub(crate) mod parse_precursor_list;
pub(crate) use parse_precursor_list::parse_precursor_list;
pub(crate) mod parse_product_list;
pub(crate) use parse_product_list::parse_product_list;
pub(crate) mod parse_spectrum_list;
pub(crate) use parse_spectrum_list::{
    parse_spectrum, parse_spectrum_list, parse_spectrum_list_header,
};
pub(crate) mod parse_chromatogram_list;
pub(crate) use parse_chromatogram_list::{
    parse_chromatogram, parse_chromatogram_list, parse_chromatogram_list_header,
};
pub(crate) mod assign_attributes;
#[cfg(test)]
pub(crate) use assign_attributes::assign_attributes;
pub(crate) use assign_attributes::{EmitAttributes, assign_attributes_into};
pub(crate) mod parse_file_description;
pub(crate) use parse_file_description::parse_file_description;
pub(crate) mod parse_referenceable_param_group_list;
pub(crate) use parse_referenceable_param_group_list::parse_referenceable_param_group_list;
pub(crate) mod parse_global_metadata;
pub(crate) mod parse_sample_list;
pub(crate) use parse_sample_list::parse_sample_list;
pub(crate) mod parse_instrument_list;
pub(crate) use parse_instrument_list::parse_instrument_list;
pub(crate) mod parse_software_list;
pub(crate) use parse_software_list::parse_software_list;
pub(crate) mod parse_data_processing_list;
pub(crate) use parse_data_processing_list::parse_data_processing_list;
pub(crate) mod parse_scan_settings_list;
pub(crate) use parse_scan_settings_list::parse_scan_settings_list;
pub(crate) mod parse_cv_list;
pub(crate) use parse_cv_list::parse_cv_list;
pub(crate) mod block_reader;
pub(crate) mod children_lookup;
pub(crate) mod cv_table;
pub(crate) mod decompression_limit;
pub(crate) mod spectrum_source;
pub(crate) mod window_directory;

#[cfg(test)]
mod tests;
