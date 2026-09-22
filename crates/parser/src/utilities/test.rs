use core::str::FromStr;
use std::{fs, path::PathBuf, sync::OnceLock};

use crate::{
    ion::{ReadOptions, decoder::decode::IonReader},
    mzml::{
        parse_mzml::parse_mzml,
        structs::{
            Chromatogram, ChromatogramList, CvParam, MzML, PrecursorList, Run, ScanList, Software,
            SoftwareParam, Spectrum, SpectrumDescription,
        },
    },
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum CvRefMode {
    #[allow(dead_code)]
    Strict,
    #[allow(dead_code)]
    AllowMissingMs,
}

#[allow(dead_code)]
pub(crate) fn mzml(cache: &'static OnceLock<MzML>, path: &str) -> &'static MzML {
    cache.get_or_init(|| {
        let bytes = load_mzml_bytes(path);
        parse_mzml(&bytes).unwrap_or_else(|e| panic!("parse_mzml failed: {e}"))
    })
}

#[allow(dead_code)]
pub(crate) fn parse_ion_as_mzml(cache: &'static OnceLock<MzML>, path: &str) -> &'static MzML {
    cache.get_or_init(|| {
        let bytes: &'static [u8] = Box::leak(load_mzml_bytes(path).into_boxed_slice());

        let mut decoder = IonReader::from_bytes(bytes, &ReadOptions::default())
            .unwrap_or_else(|e| panic!("IonReader::from_bytes failed: {e}"));

        decoder
            .to_mzml()
            .unwrap_or_else(|e| panic!("to_mzml failed: {e}"))
    })
}

pub(crate) fn load_mzml_bytes(path: &str) -> Vec<u8> {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read(&full).unwrap_or_else(|e| panic!("cannot read {:?}: {}", full, e))
}

#[allow(dead_code)]
pub(crate) fn spectrum_by_id<'a>(mzml: &'a MzML, id: &str) -> &'a Spectrum {
    let spectrum_list = mzml
        .run
        .spectrum_list
        .as_ref()
        .expect("spectrumList parsed");
    spectrum_list
        .spectra
        .iter()
        .find(|spectrum| spectrum.id == id)
        .unwrap_or_else(|| panic!("spectrum {id} not found"))
}

#[allow(dead_code)]
pub(crate) fn spectrum_by_index(mzml: &MzML, index: usize) -> &Spectrum {
    let spectrum_list = mzml
        .run
        .spectrum_list
        .as_ref()
        .expect("spectrumList parsed");
    spectrum_list
        .spectra
        .get(index)
        .unwrap_or_else(|| panic!("spectrum index {index} not found"))
}

#[allow(dead_code)]
pub(crate) fn spectrum_description(spectrum: &Spectrum) -> &SpectrumDescription {
    spectrum
        .spectrum_description
        .as_ref()
        .expect("spectrumDescription parsed")
}

#[allow(dead_code)]
pub(crate) fn spectrum_scan_list(spectrum: &Spectrum) -> &ScanList {
    if let Some(spectrum_description) = spectrum.spectrum_description.as_ref()
        && let Some(spectrum_list) = spectrum_description.scan_list.as_ref()
    {
        return spectrum_list;
    }
    spectrum.scan_list.as_ref().expect("scanList parsed")
}

#[allow(dead_code)]
pub(crate) fn spectrum_precursor_list(spectrum: &Spectrum) -> Option<&PrecursorList> {
    if let Some(spectrum_description) = spectrum.spectrum_description.as_ref()
        && spectrum_description.precursor_list.is_some()
    {
        return spectrum_description.precursor_list.as_ref();
    }
    spectrum.precursor_list.as_ref()
}

#[allow(dead_code)]
pub(crate) fn chromatogram_list(run: &Run) -> &ChromatogramList {
    run.chromatogram_list
        .as_ref()
        .expect("chromatogramList parsed")
}

#[allow(dead_code)]
pub(crate) fn chromatogram<'a>(
    chromatogram_list: &'a ChromatogramList,
    id: &str,
) -> &'a Chromatogram {
    chromatogram_list
        .chromatograms
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("chromatogram {id} not found"))
}

#[allow(dead_code)]
pub(crate) fn cv_by_name<'a>(cv_params: &'a [CvParam], name: &str) -> Option<&'a CvParam> {
    cv_params.iter().find(|cv| cv.name == name)
}

#[allow(dead_code)]
pub(crate) fn assert_cv_absent(cv_params: &[CvParam], name: &str) {
    let got = cv_by_name(cv_params, name);
    assert!(
        got.is_none(),
        "expected cvParam {name:?} to be absent, got: {got:?}"
    );
}

pub(crate) fn assert_cv_ref(policy: CvRefMode, got: Option<&str>, expected: &str, ctx: &str) {
    match (policy, got) {
        (_, Some(v)) if v == expected => {}
        (CvRefMode::AllowMissingMs, Some("")) if expected == "MS" => {}
        (CvRefMode::AllowMissingMs, None) if expected == "MS" => {}
        _ => panic!("wrong cv_ref for {ctx}: got {got:?}, expected {expected:?}"),
    }
}

pub(crate) enum ExpectedCvValue<'a> {
    None,
    Str(&'a str),
    F64(f64),
    F32(f32),
}

impl<'a> From<&'a str> for ExpectedCvValue<'a> {
    #[inline]
    fn from(v: &'a str) -> Self {
        Self::Str(v)
    }
}

impl<'a> From<f64> for ExpectedCvValue<'a> {
    #[inline]
    fn from(v: f64) -> Self {
        Self::F64(v)
    }
}

impl<'a> From<f32> for ExpectedCvValue<'a> {
    #[inline]
    fn from(v: f32) -> Self {
        Self::F32(v)
    }
}

impl<'a> From<Option<&'a str>> for ExpectedCvValue<'a> {
    #[inline]
    fn from(v: Option<&'a str>) -> Self {
        v.map(Self::Str).unwrap_or(Self::None)
    }
}

#[inline]
fn parse_required<T: FromStr>(s: &str, what: &str, name: &str) -> T
where
    T::Err: core::fmt::Debug,
{
    s.parse::<T>()
        .unwrap_or_else(|e| panic!("wrong {what} for {name}: {s:?} ({e:?})"))
}

#[allow(dead_code)]
pub(crate) fn assert_cv<'a>(
    policy: CvRefMode,
    cv_params: &[CvParam],
    name: &str,
    accession: &str,
    cv_ref: &str,
    value: impl Into<ExpectedCvValue<'a>>,
    unit_name: Option<&str>,
) {
    let cv = cv_params
        .iter()
        .find(|cv| cv.name == name)
        .unwrap_or_else(|| panic!("cvParam with name {name} not found"));

    assert_eq!(
        cv.accession.as_deref(),
        Some(accession),
        "wrong accession for {name}"
    );

    assert_cv_ref(policy, cv.cv_ref.as_deref(), cv_ref, name);

    let got_value = cv.value.as_deref();

    macro_rules! assert_num {
        ($t:ty, $expected:expr, $eps:expr) => {{
            let s = got_value.unwrap_or_else(|| panic!("expected value for {name}, got None"));
            let got: $t = parse_required::<$t>(s, "value", name);
            let expected: $t = $expected;
            let eps: $t = $eps;
            assert!(
                (got - expected).abs() <= eps,
                "wrong value for {name}: got {got} expected {expected} (eps {eps})"
            );
        }};
    }

    match value.into() {
        ExpectedCvValue::Str(v) => {
            if v.is_empty() {
                assert!(
                    got_value.unwrap_or("").is_empty(),
                    "wrong value for {name}: {:?}",
                    cv.value
                );
            } else {
                assert_eq!(got_value, Some(v), "wrong value for {name}");
            }
        }
        ExpectedCvValue::None => assert!(
            got_value.is_none(),
            "expected no value for {name}, got {:?}",
            cv.value
        ),
        ExpectedCvValue::F64(expected) => assert_num!(f64, expected, 1e-12_f64),
        ExpectedCvValue::F32(expected) => assert_num!(f32, expected, 1e-6_f32),
    }

    match unit_name {
        Some(u) => assert_eq!(
            cv.unit_name.as_deref(),
            Some(u),
            "wrong unit_name for {name}"
        ),
        None => assert!(
            cv.unit_name.is_none(),
            "expected no unit_name for {name}, got {:?}",
            cv.unit_name
        ),
    }
}

#[allow(dead_code)]
pub(crate) fn assert_software(
    policy: CvRefMode,
    sw: &Software,
    cv_ref: &str,
    accession: &str,
    name: &str,
    version: Option<&str>,
) {
    if let Some(p) = sw.software_param.first() {
        assert_software_param(policy, p, cv_ref, accession, name, version);
        return;
    }

    assert_cv(policy, &sw.cv_param, name, accession, cv_ref, None, None);
    assert_eq!(sw.version.as_deref(), version, "wrong version for {name}");
}

#[allow(dead_code)]
pub(crate) fn assert_software_param(
    policy: CvRefMode,
    p: &SoftwareParam,
    cv_ref: &str,
    accession: &str,
    name: &str,
    version: Option<&str>,
) {
    assert_cv_ref(policy, p.cv_ref.as_deref(), cv_ref, name);
    assert_eq!(
        p.accession.as_str(),
        accession,
        "wrong accession for {name}"
    );
    assert_eq!(p.name.as_str(), name, "wrong name for {name}");
    assert_eq!(p.version.as_deref(), version, "wrong version for {name}");
}
