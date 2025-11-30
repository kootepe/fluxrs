use crate::data_formats::chamberdata::Chamber;
use crate::flux::fluxmodel::FluxModel;
use crate::gaschannel::GasChannel;
use crate::gastype::GasType;
use std::fmt;
use std::str::FromStr;

#[derive(Debug)]
pub struct ParseFluxUnitError(String);

impl fmt::Display for ParseFluxUnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseFluxUnitError {}

#[derive(Default, Clone, Copy, Hash, PartialEq, Eq)]
pub enum FluxUnit {
    #[default]
    UmolM2S,
    UmolM2H,
    MmolM2S,
    MmolM2H,
    MgM2S,
    MgM2H,

    NmolM2S,
    NmolM2H,
}

impl std::fmt::Display for FluxUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FluxUnit::UmolM2S => write!(f, "µmol/m2/s"),
            FluxUnit::UmolM2H => write!(f, "µmol/m2/h"),
            FluxUnit::MmolM2S => write!(f, "mmol/m2/s"),
            FluxUnit::MmolM2H => write!(f, "mmol/m2/h"),
            FluxUnit::MgM2S => write!(f, "mg/m2/s"),
            FluxUnit::MgM2H => write!(f, "mg/m2/h"),
            FluxUnit::NmolM2S => write!(f, "nmol/m2/s"),
            FluxUnit::NmolM2H => write!(f, "nmol/m2/h"),
        }
    }
}

impl FromStr for FluxUnit {
    type Err = ParseFluxUnitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "µmol/m2/s" => Ok(FluxUnit::UmolM2S),
            "µmol/m2/h" => Ok(FluxUnit::UmolM2H),
            "mmol/m2/s" => Ok(FluxUnit::MmolM2S),
            "mmol/m2/h" => Ok(FluxUnit::MmolM2H),
            "mg/m2/s" => Ok(FluxUnit::MgM2S),
            "mg/m2/h" => Ok(FluxUnit::MgM2H),
            "nmol/m2/s" => Ok(FluxUnit::NmolM2S),
            "nmol/m2/h" => Ok(FluxUnit::NmolM2H),

            other => Err(ParseFluxUnitError(format!("Invalid unit: {other}"))),
        }
    }
}

impl FluxUnit {
    pub fn all() -> &'static [FluxUnit] {
        use FluxUnit::*;
        &[UmolM2S, UmolM2H, MmolM2S, MmolM2H, MgM2S, MgM2H, NmolM2S, NmolM2H]
    }

    pub fn from_umol_m2_s(&self, value_umol_m2_s: f64, gas: GasType) -> f64 {
        match self {
            // nmol/m²/s = µmol * 1000
            FluxUnit::NmolM2S => value_umol_m2_s * 1000.0,

            // nmol/m²/h = µmol * 1000 * 3600
            FluxUnit::NmolM2H => value_umol_m2_s * 1000.0 * 3600.0,

            // base
            FluxUnit::UmolM2S => value_umol_m2_s,

            // µmol → hours
            FluxUnit::UmolM2H => value_umol_m2_s * 3600.0,

            // mmol
            FluxUnit::MmolM2S => value_umol_m2_s / 1000.0,
            FluxUnit::MmolM2H => value_umol_m2_s / 1000.0 * 3600.0,

            // mg
            FluxUnit::MgM2S => value_umol_m2_s * gas.mol_mass() / 1000.0,
            FluxUnit::MgM2H => value_umol_m2_s * gas.mol_mass() / 1000.0 * 3600.0,
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            FluxUnit::UmolM2S => "umol_m2_s",
            FluxUnit::UmolM2H => "umol_m2_h",
            FluxUnit::MmolM2S => "mmol_m2_s",
            FluxUnit::MmolM2H => "mmol_m2_h",
            FluxUnit::MgM2S => "mg_m2_s",
            FluxUnit::MgM2H => "mg_m2_h",
            FluxUnit::NmolM2S => "nmol_m2_s",
            FluxUnit::NmolM2H => "nmol_m2_h",
        }
    }
}
