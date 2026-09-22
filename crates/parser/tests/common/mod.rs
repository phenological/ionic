#![allow(dead_code)]

#[macro_use]
mod macros;

pub mod assertions;
pub mod binary_ext;
pub mod helpers;
pub mod test_files;

use std::{collections::BTreeSet, fs, path::PathBuf, sync::OnceLock};

#[allow(unused_imports)]
pub(crate) use binary_ext::BinaryDataExt;
use ionic::{
    IonReader, IonResult, ReadOptions, WriteOptions,
    mzml::{
        parse_mzml::{parse_indexed_mzml, parse_mzml},
        structs::*,
    },
};

pub(crate) fn repo_root() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root path must be resolvable")
    })
}

pub(crate) fn read_test_file(rel: &str) -> Vec<u8> {
    let full = repo_root().join(rel);
    fs::read(&full).unwrap_or_else(|e| panic!("cannot read test_file {}: {e}", full.display()))
}

pub(crate) fn parse_test_file(rel: &str) -> MzML {
    let bytes = read_test_file(rel);
    parse_mzml(&bytes).unwrap_or_else(|e| panic!("parse_mzml failed for {rel}: {e}"))
}

pub(crate) fn parse_xml(xml: &str) -> MzML {
    parse_mzml(xml.as_bytes()).unwrap_or_else(|e| panic!("parse_mzml(xml) failed: {e}"))
}

pub(crate) fn parse_indexed(rel: &str) -> IndexedmzML {
    let bytes = read_test_file(rel);
    parse_indexed_mzml(&bytes)
        .unwrap_or_else(|e| panic!("parse_indexed_mzml failed for {rel}: {e}"))
}

pub(crate) fn write_mzml_to_ion(
    mzml: &MzML,
    options: WriteOptions,
    output: &mut Vec<u8>,
) -> IonResult<()> {
    let mut writer = ionic::IonWriter::to(output, mzml, &options)?;
    if let Some(list) = &mzml.run.spectrum_list {
        for spectrum in &list.spectra {
            writer.write_spectrum(spectrum)?;
        }
    }
    if let Some(list) = &mzml.run.chromatogram_list {
        for chromatogram in &list.chromatograms {
            writer.write_chromatogram(chromatogram)?;
        }
    }
    writer.finish()
}

pub(crate) fn encode_to_ion(mzml: &MzML, compression_level: u8, force_f32: bool) -> Vec<u8> {
    let mut out = Vec::new();
    write_mzml_to_ion(
        mzml,
        WriteOptions {
            compression_level,
            force_f32,
            ..Default::default()
        },
        &mut out,
    )
    .expect("encode should succeed");
    out
}

pub(crate) fn decode_ion(bytes: &[u8]) -> IonResult<MzML> {
    let mut decoder = IonReader::from_bytes(bytes, &ReadOptions::default())?;
    decoder.to_mzml()
}

pub(crate) fn spectra(mzml: &MzML) -> &[Spectrum] {
    mzml.run
        .spectrum_list
        .as_ref()
        .map(|list| list.spectra.as_slice())
        .unwrap_or(&[])
}

pub(crate) fn chromatograms(mzml: &MzML) -> &[Chromatogram] {
    mzml.run
        .chromatogram_list
        .as_ref()
        .map(|list| list.chromatograms.as_slice())
        .unwrap_or(&[])
}

pub(crate) fn spectrum_arrays(s: &Spectrum) -> &[BinaryDataArray] {
    s.binary_data_array_list
        .as_ref()
        .map(|b| b.binary_data_arrays.as_slice())
        .unwrap_or(&[])
}

pub(crate) fn chromatogram_arrays(c: &Chromatogram) -> &[BinaryDataArray] {
    c.binary_data_array_list
        .as_ref()
        .map(|b| b.binary_data_arrays.as_slice())
        .unwrap_or(&[])
}

pub(crate) fn spectrum_by_id<'a>(mzml: &'a MzML, id: &str) -> &'a Spectrum {
    spectra(mzml)
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("spectrum not found: {id}"))
}

pub(crate) fn chromatogram_by_id<'a>(mzml: &'a MzML, id: &str) -> &'a Chromatogram {
    chromatograms(mzml)
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("chromatogram not found: {id}"))
}

pub(crate) fn find_array_by_accession<'a>(
    arrays: &'a [BinaryDataArray],
    accession: &str,
) -> &'a BinaryDataArray {
    arrays
        .iter()
        .find(|a| cv_has_accession(&a.cv_params, accession))
        .unwrap_or_else(|| panic!("binaryDataArray with accession {accession} not found"))
}

pub(crate) fn cv_has_accession(cv_params: &[CvParam], accession: &str) -> bool {
    cv_params
        .iter()
        .any(|p| p.accession.as_deref() == Some(accession))
}

pub(crate) fn cv_param_by_accession<'a>(
    cv_params: &'a [CvParam],
    accession: &str,
) -> Option<&'a CvParam> {
    cv_params
        .iter()
        .find(|p| p.accession.as_deref() == Some(accession))
}

pub(crate) fn cv_value_f64(cv_params: &[CvParam], accession: &str) -> Option<f64> {
    cv_param_by_accession(cv_params, accession)
        .and_then(|p| p.value.as_deref())
        .and_then(|v| v.parse::<f64>().ok())
}

pub(crate) fn bda_role(bda: &BinaryDataArray) -> &'static str {
    if cv_has_accession(&bda.cv_params, "MS:1000514") {
        return "mz";
    }
    if cv_has_accession(&bda.cv_params, "MS:1000515") {
        return "intensity";
    }
    if cv_has_accession(&bda.cv_params, "MS:1000595") {
        return "time";
    }
    if cv_has_accession(&bda.cv_params, "MS:1000786") {
        return "non_standard";
    }
    "other"
}

pub(crate) fn scan_list_of_spectrum(s: &Spectrum) -> Option<&ScanList> {
    if let Some(sd) = s.spectrum_description.as_ref()
        && sd.scan_list.is_some()
    {
        return sd.scan_list.as_ref();
    }
    s.scan_list.as_ref()
}

pub(crate) fn scan_list_of_spectrum_mut(s: &mut Spectrum) -> Option<&mut ScanList> {
    if let Some(sd) = s.spectrum_description.as_mut()
        && sd.scan_list.is_some()
    {
        return sd.scan_list.as_mut();
    }
    s.scan_list.as_mut()
}

pub(crate) fn precursor_list_of_spectrum(s: &Spectrum) -> Option<&PrecursorList> {
    if let Some(sd) = s.spectrum_description.as_ref()
        && sd.precursor_list.is_some()
    {
        return sd.precursor_list.as_ref();
    }
    s.precursor_list.as_ref()
}

pub(crate) fn product_list_of_spectrum(s: &Spectrum) -> Option<&ProductList> {
    if let Some(sd) = s.spectrum_description.as_ref()
        && sd.product_list.is_some()
    {
        return sd.product_list.as_ref();
    }
    s.product_list.as_ref()
}

pub(crate) fn first_scan(s: &Spectrum) -> &Scan {
    scan_list_of_spectrum(s)
        .and_then(|sl| sl.scans.first())
        .expect("first scan must exist")
}

pub(crate) fn first_scan_mut(s: &mut Spectrum) -> &mut Scan {
    scan_list_of_spectrum_mut(s)
        .and_then(|sl| sl.scans.first_mut())
        .expect("first scan must exist")
}

pub(crate) fn spectrum_scan_count(s: &Spectrum) -> usize {
    if let Some(sd) = s.spectrum_description.as_ref() {
        return sd.scan_list.as_ref().map(|sl| sl.scans.len()).unwrap_or(0);
    }
    s.scan_list.as_ref().map(|sl| sl.scans.len()).unwrap_or(0)
}

pub(crate) fn spectrum_precursor_count(s: &Spectrum) -> usize {
    if let Some(sd) = s.spectrum_description.as_ref() {
        return sd
            .precursor_list
            .as_ref()
            .map(|pl| pl.precursors.len())
            .unwrap_or(0);
    }
    s.precursor_list
        .as_ref()
        .map(|pl| pl.precursors.len())
        .unwrap_or(0)
}

pub(crate) fn spectrum_product_count(s: &Spectrum) -> usize {
    if let Some(sd) = s.spectrum_description.as_ref() {
        return sd
            .product_list
            .as_ref()
            .map(|pl| pl.products.len())
            .unwrap_or(0);
    }
    s.product_list
        .as_ref()
        .map(|pl| pl.products.len())
        .unwrap_or(0)
}

pub(crate) fn parse_scan_number_from_id(id: &str) -> Option<u32> {
    id.split_whitespace()
        .find_map(|tok| tok.strip_prefix("scan="))
        .and_then(|v| v.parse::<u32>().ok())
}

pub(crate) fn scan_start_time_seconds(s: &Spectrum) -> Option<f64> {
    let scan = scan_list_of_spectrum(s)?.scans.first()?;
    let p = cv_param_by_accession(&scan.cv_params, "MS:1000016")?;
    let value = p.value.as_deref()?.parse::<f64>().ok()?;
    match p.unit_accession.as_deref() {
        Some("UO:0000031") => Some(value * 60.0),
        _ => Some(value),
    }
}

pub(crate) fn scan_start_time_raw(s: &Spectrum) -> Option<f64> {
    let scan = scan_list_of_spectrum(s)?.scans.first()?;
    let p = cv_param_by_accession(&scan.cv_params, "MS:1000016")?;
    p.value.as_deref()?.parse::<f64>().ok()
}

pub(crate) fn scan_start_time_unit_code(s: &Spectrum) -> u8 {
    let Some(scan) = scan_list_of_spectrum(s).and_then(|list| list.scans.first()) else {
        return 0;
    };
    let Some(p) = cv_param_by_accession(&scan.cv_params, "MS:1000016") else {
        return 0;
    };
    match p.unit_accession.as_deref() {
        Some("UO:0000031") | Some("MS:1000038") => 2,
        Some("UO:0000010") | Some("MS:1000039") => 1,
        Some("UO:0000028") => 3,
        _ => match p.unit_name.as_deref() {
            Some("minute" | "minutes") => 2,
            Some("second" | "seconds") => 1,
            Some("millisecond" | "milliseconds") => 3,
            _ => 0,
        },
    }
}

pub(crate) fn id_name_value_pairs(id: &str) -> Vec<(&str, &str)> {
    id.split_whitespace()
        .filter_map(|tok| tok.split_once('='))
        .collect()
}

pub(crate) fn find_name_value_indices(mzml: &MzML, key: &str, value: &str) -> Vec<usize> {
    spectra(mzml)
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let has_pair = id_name_value_pairs(&s.id)
                .into_iter()
                .any(|(k, v)| k == key && v == value);
            if has_pair { Some(i) } else { None }
        })
        .collect()
}

pub(crate) fn find_spot_id_indices(mzml: &MzML, spot_id: &str) -> Vec<usize> {
    spectra(mzml)
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match s.spot_id.as_deref() {
            Some(id) if id == spot_id => Some(i),
            _ => None,
        })
        .collect()
}

pub(crate) fn first_array_values_by_accession(s: &Spectrum, accession: &str) -> Vec<f64> {
    use binary_ext::BinaryDataExt;
    let bda = find_array_by_accession(spectrum_arrays(s), accession);
    let bin = bda.binary.as_ref().expect("binary payload present");
    bin.to_f64_vec()
}

pub(crate) fn first_chrom_array_values_by_accession(c: &Chromatogram, accession: &str) -> Vec<f64> {
    use binary_ext::BinaryDataExt;
    let bda = find_array_by_accession(chromatogram_arrays(c), accession);
    let bin = bda.binary.as_ref().expect("binary payload present");
    bin.to_f64_vec()
}

pub(crate) fn set_of_ids<T, F>(items: &[T], mut f: F) -> BTreeSet<String>
where
    F: FnMut(&T) -> Option<&str>,
{
    let mut out = BTreeSet::new();
    for item in items {
        if let Some(id) = f(item) {
            out.insert(id.to_string());
        }
    }
    out
}

pub(crate) fn top_level_software_ids(m: &MzML) -> BTreeSet<String> {
    m.software_list
        .as_ref()
        .map(|sl| set_of_ids(&sl.software, |s| Some(s.id.as_str())))
        .unwrap_or_default()
}

pub(crate) fn top_level_dp_ids(m: &MzML) -> BTreeSet<String> {
    m.data_processing_list
        .as_ref()
        .map(|dpl| set_of_ids(&dpl.data_processing, |dp| Some(dp.id.as_str())))
        .unwrap_or_default()
}

pub(crate) fn top_level_source_file_ids(m: &MzML) -> BTreeSet<String> {
    m.file_description
        .as_ref()
        .map(|fd| set_of_ids(&fd.source_file_list.source_file, |sf| Some(sf.id.as_str())))
        .unwrap_or_default()
}

pub(crate) fn top_level_instrument_ids(m: &MzML) -> BTreeSet<String> {
    m.instrument_list
        .as_ref()
        .map(|il| set_of_ids(&il.instrument, |ic| Some(ic.id.as_str())))
        .unwrap_or_default()
}

pub(crate) fn top_level_sample_ids(m: &MzML) -> BTreeSet<String> {
    m.sample_list
        .as_ref()
        .map(|sl| set_of_ids(&sl.samples, |s| Some(s.id.as_str())))
        .unwrap_or_default()
}

pub const FNV64_OFFSET: u64 = 0xcbf29ce484222325;
pub const FNV64_PRIME: u64 = 0x00000100000001B3;

pub(crate) fn fnv64_update(mut state: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        state ^= *b as u64;
        state = state.wrapping_mul(FNV64_PRIME);
    }
    state
}

pub(crate) fn fnv64_bytes(bytes: &[u8]) -> u64 {
    fnv64_update(FNV64_OFFSET, bytes)
}

pub(crate) fn fnv64_str(s: &str) -> u64 {
    fnv64_bytes(s.as_bytes())
}

pub(crate) fn hash_binary_payload(bin: &NumericArray) -> u64 {
    use binary_ext::BinaryDataExt;
    fnv64_bytes(&bin.to_le_bytes())
}

pub(crate) fn canonical_hash_index(mzml: &MzML) -> std::collections::BTreeMap<String, u64> {
    let mut idx = std::collections::BTreeMap::new();

    idx.insert("run/id".to_string(), fnv64_str(mzml.run.id.as_str()));
    idx.insert("count/spectra".to_string(), spectra(mzml).len() as u64);
    idx.insert(
        "count/chromatograms".to_string(),
        chromatograms(mzml).len() as u64,
    );

    for (i, sp) in spectra(mzml).iter().enumerate() {
        idx.insert(format!("spectrum/{i}/id"), fnv64_str(sp.id.as_str()));
        idx.insert(
            format!("spectrum/{i}/ms_level"),
            sp.ms_level.unwrap_or(0) as u64,
        );
        idx.insert(
            format!("spectrum/{i}/default_array_length"),
            sp.default_array_length.unwrap_or(0) as u64,
        );
        idx.insert(
            format!("spectrum/{i}/scan_count"),
            spectrum_scan_count(sp) as u64,
        );
        idx.insert(
            format!("spectrum/{i}/precursor_count"),
            spectrum_precursor_count(sp) as u64,
        );
        idx.insert(
            format!("spectrum/{i}/product_count"),
            spectrum_product_count(sp) as u64,
        );
        for (j, bda) in spectrum_arrays(sp).iter().enumerate() {
            let role = bda_role(bda);
            idx.insert(format!("spectrum/{i}/array/{j}/role"), fnv64_str(role));
            idx.insert(
                format!("spectrum/{i}/array/{j}/len"),
                bda.array_length.unwrap_or(0) as u64,
            );
            idx.insert(
                format!("spectrum/{i}/array/{j}/payload"),
                bda.binary.as_ref().map(hash_binary_payload).unwrap_or(0),
            );
        }
    }

    for (i, ch) in chromatograms(mzml).iter().enumerate() {
        idx.insert(format!("chromatogram/{i}/id"), fnv64_str(ch.id.as_str()));
        idx.insert(
            format!("chromatogram/{i}/default_array_length"),
            ch.default_array_length.unwrap_or(0) as u64,
        );
        for (j, bda) in chromatogram_arrays(ch).iter().enumerate() {
            let role = bda_role(bda);
            idx.insert(format!("chromatogram/{i}/array/{j}/role"), fnv64_str(role));
            idx.insert(
                format!("chromatogram/{i}/array/{j}/len"),
                bda.array_length.unwrap_or(0) as u64,
            );
            idx.insert(
                format!("chromatogram/{i}/array/{j}/payload"),
                bda.binary.as_ref().map(hash_binary_payload).unwrap_or(0),
            );
        }
    }

    idx
}

pub(crate) fn canonical_diff_paths(left: &MzML, right: &MzML) -> Vec<String> {
    let l = canonical_hash_index(left);
    let r = canonical_hash_index(right);

    let mut keys: BTreeSet<String> = l.keys().cloned().collect();
    keys.extend(r.keys().cloned());

    let mut out = Vec::new();
    for k in keys {
        match (l.get(&k), r.get(&k)) {
            (Some(a), Some(b)) if a != b => out.push(format!("{k}: {a:016x} != {b:016x}")),
            (Some(a), None) => out.push(format!("{k}: {a:016x} != <missing>")),
            (None, Some(b)) => out.push(format!("{k}: <missing> != {b:016x}")),
            _ => {}
        }
    }
    out
}

pub(crate) fn semantic_fingerprint(mzml: &MzML) -> String {
    let idx = canonical_hash_index(mzml);
    let mut state = FNV64_OFFSET;
    for (path, h) in idx {
        state = fnv64_update(state, path.as_bytes());
        state = fnv64_update(state, &h.to_le_bytes());
    }
    format!("{state:016x}")
}

pub(crate) fn first_spectrum_binary(mzml: &MzML) -> Option<&NumericArray> {
    mzml.run
        .spectrum_list
        .as_ref()?
        .spectra
        .first()?
        .binary_data_array_list
        .as_ref()?
        .binary_data_arrays
        .first()?
        .binary
        .as_ref()
}

pub(crate) fn roundtrip(mzml: &MzML) -> MzML {
    decode_ion(&encode_to_ion(mzml, 0, false)).expect("decode should succeed")
}
