use std::fmt;

#[derive(Clone, PartialEq, Debug, Copy)]
pub enum DataType {
    Gas,
    Cycle,
    Meteo,
    Height,
    Chamber,
}
impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DataType::Gas => write!(f, "Gas Data"),
            DataType::Cycle => write!(f, "Cycle data"),
            DataType::Meteo => write!(f, "Meteo data"),
            DataType::Height => write!(f, "Height data"),
            DataType::Chamber => write!(f, "Chamber metadata"),
        }
    }
}

impl DataType {
    pub fn type_str(&self) -> &'static str {
        match self {
            DataType::Gas => "gas",
            DataType::Cycle => "cycle",
            DataType::Meteo => "meteo",
            DataType::Height => "height",
            DataType::Chamber => "chamber_meta",
        }
    }
}
