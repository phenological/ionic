use crate::{
    accessions as acc_const,
    ion::attr_meta::parse_accession_tail,
    mzml::structs::{CvParam, MzML, NumericArray, Spectrum},
};

#[inline]
fn acc(s: Option<&str>) -> u32 {
    parse_accession_tail(s).raw()
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Second,
    Minute,
    Millisecond,
    Other,
}

impl TimeUnit {
    pub(crate) fn code(self) -> u8 {
        match self {
            TimeUnit::Other => 0,
            TimeUnit::Second => 1,
            TimeUnit::Minute => 2,
            TimeUnit::Millisecond => 3,
        }
    }

    pub(crate) fn from_code(code: u8) -> Self {
        match code {
            1 => TimeUnit::Second,
            2 => TimeUnit::Minute,
            3 => TimeUnit::Millisecond,
            _ => TimeUnit::Other,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct ScanSummary {
    pub rt: f64,
    pub rt_unit: TimeUnit,
    pub ms_level: u8,
    pub polarity: u8,
    pub selected_ion_mz: f64,
    pub base_peak_mz: f64,
    pub base_peak_int: f64,
    pub total_ion_current: f64,
    pub position_x: u32,
    pub position_y: u32,
    pub position_z: u32,
}

#[cfg(test)]
pub trait ScanSource {
    fn for_each_summary(&mut self, callback: &mut dyn FnMut(usize, ScanSummary));
    fn load_scan(&mut self, index: usize, mz: &mut Vec<f64>, intensity: &mut Vec<f64>) -> bool;

    fn for_each_in_range<F>(&mut self, rt_min: f64, rt_max: f64, ms_level: u8, mut callback: F)
    where
        Self: Sized,
        F: FnMut(&ScanSummary, &[f64], &[f64]),
    {
        let mut matching: Vec<(usize, ScanSummary)> = Vec::new();
        self.for_each_summary(&mut |index, summary| {
            if summary.rt >= rt_min
                && summary.rt <= rt_max
                && (ms_level == 0 || summary.ms_level == ms_level)
            {
                matching.push((index, summary));
            }
        });
        let mut mz = Vec::new();
        let mut intensity = Vec::new();
        for (index, summary) in &matching {
            if self.load_scan(*index, &mut mz, &mut intensity) {
                callback(summary, &mz, &intensity);
            }
        }
    }
}

#[cfg(test)]
impl ScanSource for MzML {
    fn for_each_summary(&mut self, callback: &mut dyn FnMut(usize, ScanSummary)) {
        let Some(list) = self.run.spectrum_list.as_ref() else {
            return;
        };
        summary_from_spectra(&list.spectra, callback);
    }

    fn load_scan(&mut self, index: usize, mz: &mut Vec<f64>, intensity: &mut Vec<f64>) -> bool {
        let spectra = self
            .run
            .spectrum_list
            .as_ref()
            .map(|l| l.spectra.as_slice())
            .unwrap_or_default();
        load_scan_from_spectra(spectra, index, mz, intensity)
    }

    fn for_each_in_range<F>(&mut self, rt_min: f64, rt_max: f64, ms_level: u8, mut callback: F)
    where
        Self: Sized,
        F: FnMut(&ScanSummary, &[f64], &[f64]),
    {
        let spectra = self
            .run
            .spectrum_list
            .as_ref()
            .map(|l| l.spectra.as_slice())
            .unwrap_or_default();
        let mut mz = Vec::new();
        let mut intensity = Vec::new();
        for (index, spectrum) in spectra.iter().enumerate() {
            let summary = summary_from_spectrum(spectrum);
            if summary.rt < rt_min
                || summary.rt > rt_max
                || (ms_level != 0 && summary.ms_level != ms_level)
            {
                continue;
            }
            if load_scan_from_spectra(spectra, index, &mut mz, &mut intensity) {
                callback(&summary, &mz, &intensity);
            }
        }
    }
}

pub(crate) fn summary_from_spectrum(spectrum: &Spectrum) -> ScanSummary {
    let mut rt = f64::NAN;
    let mut rt_unit = TimeUnit::Other;
    let mut ms_level = spectrum
        .ms_level
        .and_then(|level| u8::try_from(level).ok())
        .unwrap_or(0);
    let mut polarity = 0u8;
    let mut base_peak_mz = f64::NAN;
    let mut base_peak_int = f64::NAN;
    let mut total_ion_current = f64::NAN;

    for param in &spectrum.cv_params {
        match acc(param.accession.as_deref()) {
            acc_const::MS_LEVEL => {
                if let Some(value) = param.value.as_deref().and_then(|v| v.parse().ok()) {
                    ms_level = value;
                }
            }
            acc_const::BASE_PEAK_MZ => base_peak_mz = parse_f64(param.value.as_deref()),
            acc_const::BASE_PEAK_INT => base_peak_int = parse_f64(param.value.as_deref()),
            acc_const::TOTAL_ION_CURRENT => total_ion_current = parse_f64(param.value.as_deref()),
            acc_const::POSITIVE_SCAN => polarity = 1,
            acc_const::NEGATIVE_SCAN => polarity = 2,
            _ => {}
        }
    }

    let scan_list = spectrum.scan_list.as_ref().or_else(|| {
        spectrum
            .spectrum_description
            .as_ref()
            .and_then(|d| d.scan_list.as_ref())
    });

    'find_rt: {
        let Some(scan_list) = scan_list else {
            break 'find_rt;
        };
        for scan in &scan_list.scans {
            if let Some((value, unit)) = rt_from_params(&scan.cv_params) {
                rt = value;
                rt_unit = unit;
                break 'find_rt;
            }
        }
        if let Some((value, unit)) = rt_from_params(&scan_list.cv_params) {
            rt = value;
            rt_unit = unit;
        }
    }

    let mut position_x = 0u32;
    let mut position_y = 0u32;
    let mut position_z = 0u32;
    if let Some(scan_list) = scan_list {
        for scan in &scan_list.scans {
            for param in &scan.cv_params {
                match acc(param.accession.as_deref()) {
                    acc_const::POSITION_X => position_x = parse_u32(param.value.as_deref()),
                    acc_const::POSITION_Y => position_y = parse_u32(param.value.as_deref()),
                    acc_const::POSITION_Z => position_z = parse_u32(param.value.as_deref()),
                    _ => {}
                }
            }
        }
    }

    let selected_ion_mz = spectrum
        .precursor_list
        .as_ref()
        .and_then(|precursor_list| precursor_list.precursors.first())
        .and_then(|precursor| precursor.selected_ion_list.as_ref())
        .and_then(|selected_ion_list| selected_ion_list.selected_ions.first())
        .map(|selected_ion| {
            selected_ion
                .cv_params
                .iter()
                .find(|param| acc(param.accession.as_deref()) == acc_const::SELECTED_ION_MZ)
                .and_then(|param| param.value.as_deref()?.parse().ok())
                .unwrap_or(f64::NAN)
        })
        .unwrap_or(f64::NAN);

    ScanSummary {
        rt,
        rt_unit,
        ms_level,
        polarity,
        base_peak_mz,
        base_peak_int,
        total_ion_current,
        selected_ion_mz,
        position_x,
        position_y,
        position_z,
    }
}

#[cfg(test)]
pub(crate) fn summary_from_spectra(
    spectra: &[Spectrum],
    callback: &mut dyn FnMut(usize, ScanSummary),
) {
    for (index, spectrum) in spectra.iter().enumerate() {
        callback(index, summary_from_spectrum(spectrum));
    }
}

#[inline]
fn rt_from_params(params: &[CvParam]) -> Option<(f64, TimeUnit)> {
    for param in params {
        if acc(param.accession.as_deref()) == acc_const::SCAN_START_TIME {
            let value: f64 = param.value.as_deref()?.parse().ok()?;
            if !value.is_finite() {
                return None;
            }
            let unit = time_unit_from(param.unit_accession.as_deref(), param.unit_name.as_deref());
            return Some((value, unit));
        }
    }
    None
}

fn time_unit_from(unit_accession: Option<&str>, unit_name: Option<&str>) -> TimeUnit {
    match acc(unit_accession) {
        acc_const::UNIT_MINUTE => TimeUnit::Minute,
        acc_const::UNIT_SECOND => TimeUnit::Second,
        acc_const::UNIT_MS => TimeUnit::Millisecond,
        _ => match unit_name {
            Some("minute" | "minutes") => TimeUnit::Minute,
            Some("second" | "seconds") => TimeUnit::Second,
            Some("millisecond" | "milliseconds") => TimeUnit::Millisecond,
            _ => TimeUnit::Other,
        },
    }
}

#[inline]
fn parse_f64(s: Option<&str>) -> f64 {
    s.and_then(|v| v.parse().ok()).unwrap_or(f64::NAN)
}

#[inline]
fn parse_u32(s: Option<&str>) -> u32 {
    s.and_then(|v| v.parse().ok()).unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn binary_pair(spectrum: &Spectrum) -> Option<(&NumericArray, &NumericArray)> {
    let list = spectrum.binary_data_array_list.as_ref()?;
    let mut mz = None;
    let mut intensity = None;
    for array in &list.binary_data_arrays {
        if mz.is_some() && intensity.is_some() {
            break;
        }
        let mut is_mz = false;
        let mut is_intensity = false;
        for param in &array.cv_params {
            match acc(param.accession.as_deref()) {
                acc_const::MZ_ARRAY => is_mz = true,
                acc_const::INTENSITY_ARRAY => is_intensity = true,
                _ => {}
            }
            if is_mz && is_intensity {
                break;
            }
        }
        if is_mz {
            mz = array.binary.as_ref();
        }
        if is_intensity {
            intensity = array.binary.as_ref();
        }
    }
    Some((mz?, intensity?))
}

#[cfg(test)]
pub(crate) fn load_scan_from_spectra(
    spectra: &[Spectrum],
    index: usize,
    mz: &mut Vec<f64>,
    intensity: &mut Vec<f64>,
) -> bool {
    let Some(spectrum) = spectra.get(index) else {
        return false;
    };
    let Some((mz_data, int_data)) = binary_pair(spectrum) else {
        return false;
    };
    let len = mz_data.len().min(int_data.len());
    if len == 0 {
        return false;
    }
    mz.clear();
    mz.reserve(len);
    extend_from_binary(mz_data, mz, len);
    intensity.clear();
    intensity.reserve(len);
    extend_from_binary(int_data, intensity, len);
    true
}

#[cfg(test)]
fn extend_from_binary(data: &NumericArray, out: &mut Vec<f64>, max: usize) {
    match data {
        NumericArray::F64(v) => out.extend_from_slice(&v[..max]),
        NumericArray::F32(v) => out.extend(v[..max].iter().map(|&x| x as f64)),
        NumericArray::I16(v) => out.extend(v[..max].iter().map(|&x| x as f64)),
        NumericArray::I32(v) => out.extend(v[..max].iter().map(|&x| x as f64)),
        NumericArray::I64(v) => out.extend(v[..max].iter().map(|&x| x as f64)),
        NumericArray::F16(v) => out.extend(v[..max].iter().copied().map(f16_bits_to_f64)),
    }
}

pub(crate) fn f16_bits_to_f64(bits: u16) -> f64 {
    let sign = ((bits & 0x8000) as u32) << 16;
    let exp = (bits & 0x7C00) >> 10;
    let mant = (bits & 0x03FF) as u32;
    let bits32 = match exp {
        0 if mant == 0 => sign,
        0 => {
            let mut mantissa = mant;
            let mut exponent_shift = 0u32;
            while mantissa & 0x400 == 0 {
                mantissa <<= 1;
                exponent_shift += 1;
            }
            sign | ((127 - 14 - exponent_shift) << 23) | ((mantissa & 0x03FF) << 13)
        }
        31 => sign | 0x7F80_0000 | (mant << 13),
        exponent => sign | ((exponent as u32 + 112) << 23) | (mant << 13),
    };
    f32::from_bits(bits32) as f64
}
