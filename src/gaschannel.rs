use crate::concentrationunit::ConcentrationUnit;
use crate::gastype::GasType;
use std::fmt;

#[derive(Debug, Clone)]
pub struct GasChannel {
    pub gas: GasType,
    pub unit: ConcentrationUnit,
    pub instrument_id: String, // optional but often useful
}

impl GasChannel {
    pub fn new(gas: GasType, unit: ConcentrationUnit, instrument_id: impl Into<String>) -> Self {
        Self { gas, unit, instrument_id: instrument_id.into() }
    }

    /// Convert a slope reported by THIS channel into ppm/s
    /// slope_raw_per_s is "what the regression saw", in the instrument's native units per second
    pub fn slope_ppm_per_s(&self, slope_raw_per_s: f64) -> f64 {
        slope_raw_per_s * self.unit.to_ppm_factor()
    }
}

impl fmt::Display for GasChannel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "gas: {}, unit: {}, id: {}", self.gas, self.unit, self.instrument_id)
    }
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub gas: GasType,
    pub concentration_col: String, // column name in instrument data for concentration
    pub unit: ConcentrationUnit,   // e.g. ppb, ppm
    pub instrument_id: String,
}
