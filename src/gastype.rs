use egui::Color32;
use std::fmt;
use std::str::FromStr;

#[derive(Debug)]
pub struct ParseGasError(String);

impl fmt::Display for ParseGasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ParseGasError {}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum GasType {
    #[default]
    CO2,
    CH4,
    H2O,
    N2O,
}

impl fmt::Display for GasType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GasType::CO2 => write!(f, "CO2"),
            GasType::CH4 => write!(f, "CH4"),
            GasType::H2O => write!(f, "H2O"),
            GasType::N2O => write!(f, "N2O"),
        }
    }
}

impl FromStr for GasType {
    type Err = ParseGasError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "ch4" => Ok(GasType::CH4),
            "co2" => Ok(GasType::CO2),
            "h2o" => Ok(GasType::H2O),
            "n2o" => Ok(GasType::N2O),
            other => Err(ParseGasError(format!("Invalid gas: {other}"))),
        }
    }
}
impl GasType {
    pub fn column_name(&self) -> &'static str {
        match self {
            GasType::CH4 => "CH4",
            GasType::CO2 => "CO2",
            GasType::H2O => "H2O",
            GasType::N2O => "N2O",
        }
    }
    pub fn as_int(&self) -> usize {
        match self {
            GasType::CO2 => 0,
            GasType::CH4 => 1,
            GasType::H2O => 2,
            GasType::N2O => 3,
        }
    }
    pub fn from_int(i: usize) -> Option<GasType> {
        match i {
            0 => Some(GasType::CO2),
            1 => Some(GasType::CH4),
            2 => Some(GasType::H2O),
            3 => Some(GasType::N2O),
            _ => None,
        }
    }

    pub fn flux_col(&self) -> String {
        format!("{}_flux", self.column_name().to_lowercase())
    }
    pub fn r2_col(&self) -> String {
        format!("{}_r2", self.column_name().to_lowercase())
    }
    pub fn measurement_r2_col(&self) -> String {
        format!("{}_measurement_r2", self.column_name().to_lowercase())
    }
    pub fn intercept_col(&self) -> String {
        format!("{}_intercept", self.column_name().to_lowercase())
    }
    pub fn slope_col(&self) -> String {
        format!("{}_slope", self.column_name().to_lowercase())
    }
    pub fn calc_range_start_col(&self) -> String {
        format!("{}_calc_range_start", self.column_name().to_lowercase())
    }
    pub fn calc_range_end_col(&self) -> String {
        format!("{}_calc_range_end", self.column_name().to_lowercase())
    }
    pub fn t0_concentration_col(&self) -> String {
        format!("{}_t0_concentration", self.column_name().to_lowercase())
    }
    pub fn color(&self) -> Color32 {
        match self {
            GasType::CH4 => Color32::GREEN,
            GasType::CO2 => Color32::ORANGE,
            GasType::H2O => Color32::CYAN,
            GasType::N2O => Color32::LIGHT_RED,
        }
    }
    pub fn mol_mass(&self) -> f64 {
        match self {
            GasType::CH4 => 16.0,
            GasType::CO2 => 44.0,
            GasType::H2O => 18.0,
            GasType::N2O => 44.0,
        }
    }
    pub fn conv_factor(&self) -> f64 {
        //
        match self {
            GasType::CH4 => 1000.0,
            GasType::CO2 => 1.0,
            GasType::H2O => 1.0,
            GasType::N2O => 1000.0,
        }
    }
    pub fn unit(&self) -> &str {
        match self {
            GasType::CH4 => "ppb",
            GasType::CO2 => "ppm",
            GasType::H2O => "ppm",
            GasType::N2O => "ppb",
        }
    }
}
