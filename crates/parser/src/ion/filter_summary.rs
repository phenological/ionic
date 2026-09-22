use serde::{Deserialize, Serialize};

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
pub struct SpectrumSummary {
    pub rt: f64,
    pub rt_unit: u8,
    pub base_peak_mz: f64,
    pub selected_ion_mz: f64,
    pub base_peak_int: f64,
    pub total_ion_current: f64,
    pub ms_level: u8,
    pub polarity: u8,
    pub position_x: u32,
    pub position_y: u32,
    pub position_z: u32,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
pub struct ChromatogramSummary {
    pub lowest_mz: f64,
    pub highest_mz: f64,
    pub lowest_wavelength: f64,
    pub highest_wavelength: f64,
    pub lowest_ion_mobility: f64,
    pub highest_ion_mobility: f64,
    pub polarity: u8,
}
