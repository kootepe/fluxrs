use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConcentrationUnit {
    Ppm, // parts per million
    Ppb, // parts per billion
}

impl ConcentrationUnit {
    /// How to convert from this instrument unit to "ppm".
    /// Example: if instrument is ppb, divide by 1000 to get ppm.
    pub fn to_ppm_factor(self) -> f64 {
        match self {
            ConcentrationUnit::Ppm => 1.0,
            ConcentrationUnit::Ppb => 1.0 / 1000.0,
        }
    }

    /// Returns a string label like "ppm" or "ppb"
    pub fn as_str(&self) -> &'static str {
        match self {
            ConcentrationUnit::Ppm => "ppm",
            ConcentrationUnit::Ppb => "ppb",
        }
    }
}

impl fmt::Display for ConcentrationUnit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConcentrationUnit::Ppb => write!(f, "ppm"),
            ConcentrationUnit::Ppm => write!(f, "ppb"),
        }
    }
}
