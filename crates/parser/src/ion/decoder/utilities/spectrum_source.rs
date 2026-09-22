use crate::{
    accessions as acc_const,
    ion::attr_meta::parse_accession_tail,
    mzml::structs::{CvParam, Spectrum},
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
