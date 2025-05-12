use crate::gasdata::GasData;
use chrono::offset::LocalResult;
use chrono::prelude::DateTime;
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Helsinki;
use egui::Color32;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GasType {
    CH4,
    CO2,
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
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CH4" => Ok(GasType::CH4),
            "CO2" => Ok(GasType::CO2),
            "H2O" => Ok(GasType::H2O),
            "N2O" => Ok(GasType::N2O),
            "ch4" => Ok(GasType::CH4),
            "co2" => Ok(GasType::CO2),
            "h2o" => Ok(GasType::H2O),
            "n2o" => Ok(GasType::N2O),
            _ => Err(()),
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

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstrumentType {
    #[default]
    Li7810,
    Other, // Placeholder for additional instruments
}

impl fmt::Display for InstrumentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InstrumentType::Li7810 => write!(f, "LI-7810"),
            InstrumentType::Other => write!(f, "Other"),
        }
    }
}

impl InstrumentType {
    /// Convert a `&str` into an `InstrumentType`
    pub fn from_str(s: &str) -> Self {
        match s {
            "LI-7810" => InstrumentType::Li7810,
            _ => InstrumentType::Other,
        }
    }

    /// Return a list of available instruments (for UI dropdown)
    pub fn available_instruments() -> Vec<InstrumentType> {
        vec![InstrumentType::Li7810, InstrumentType::Other] // Expand this list as needed
    }
    pub fn available_gases(&self) -> Vec<GasType> {
        match self {
            InstrumentType::Li7810 => vec![GasType::CH4, GasType::CO2, GasType::H2O],
            InstrumentType::Other => vec![GasType::N2O], // Example for another instrument
        }
    }
}
pub struct Instrument {
    sep: u8,
    skiprows: i64,
    skip_after_header: i64,
    time_col: String,
    pub gas_cols: Vec<String>,
    flux_cols: Vec<String>,
    diag_col: String,
    has_header: bool,
}

pub struct Li7810_test {
    pub base: Instrument,
    pub model: String,
}

pub struct Li7810 {
    pub base: Instrument,
    pub model: String,
}

impl Instrument {
    pub fn mk_rdr<P: AsRef<Path>>(&self, filename: P) -> Result<csv::Reader<File>, Box<dyn Error>> {
        let file = File::open(filename)?;
        let rdr = csv::ReaderBuilder::new()
            .delimiter(self.sep)
            .has_headers(self.has_header)
            .flexible(true)
            .from_reader(file);
        Ok(rdr)
    }
}

impl Default for Li7810_test {
    fn default() -> Self {
        Self {
            base: Instrument {
                sep: b'\t',
                skiprows: 4,
                skip_after_header: 1,
                time_col: "SECONDS".to_string(),
                gas_cols: vec![
                    "CO2".to_string(),
                    "CH4".to_string(),
                    "H2O".to_string(),
                    "N2O".to_string(),
                ],
                flux_cols: vec!["CO2".to_string(), "CH4".to_string(), "N2O".to_string()],
                diag_col: "DIAG".to_string(),
                has_header: true,
            },
            model: "LI-7810_test".to_owned(),
        }
    }
}
impl Default for Li7810 {
    fn default() -> Self {
        Self {
            base: Instrument {
                sep: b'\t',
                skiprows: 4,
                skip_after_header: 1,
                time_col: "SECONDS".to_string(),
                gas_cols: vec!["CO2".to_string(), "CH4".to_string(), "H2O".to_string()],
                flux_cols: vec!["CO2".to_string(), "CH4".to_string()],
                diag_col: "DIAG".to_string(),
                has_header: true,
            },
            model: "LI-7810".to_owned(),
        }
    }
}

impl Li7810 {
    pub fn mk_rdr<P: AsRef<Path>>(&self, filename: P) -> Result<csv::Reader<File>, Box<dyn Error>> {
        self.base.mk_rdr(filename)
    }
    pub fn read_data_file<P: AsRef<Path>>(&self, filename: P) -> Result<GasData, Box<dyn Error>> {
        let mut rdr = self.mk_rdr(filename)?;
        let mut instrument_serial = String::new();
        // let mut skip = 1;

        // rdr.records().next();
        // for _ in 0..skip {
        //     rdr.records().next();
        // }
        if let Some(result) = rdr.records().next() {
            instrument_serial = result.unwrap()[1].to_owned();
            // instrument_serial.push(result.unwrap()[0].to_owned())
        }

        let skip = 3;
        for _ in 0..skip {
            rdr.records().next();
        }
        // let instrument_model = vec![self.model.clone()];
        let instrument_model = self.model.clone();

        let mut gas_data: HashMap<GasType, Vec<f64>> = HashMap::new();
        let mut diag: Vec<i64> = Vec::new();
        let mut datetime: Vec<DateTime<Utc>> = Vec::new();
        let mut header = csv::StringRecord::new();

        if let Some(result) = rdr.records().next() {
            header = result?;
        }

        let mut gas_indices: HashMap<GasType, usize> = HashMap::new();
        let diag_col = "DIAG";
        let secs_col = "SECONDS";
        let nsecs_col = "NANOSECONDS";

        // Find column indices dynamically
        for (i, h) in header.iter().enumerate() {
            if let Ok(gas_type) = h.parse::<GasType>() {
                gas_indices.insert(gas_type, i);
                gas_data.insert(gas_type, Vec::new()); // Initialize gas vectors
            }
        }
        let idx_diag = header.iter().position(|h| h == diag_col).unwrap_or_else(|| {
            eprintln!("Warning: Column '{}' not found, using default index.", diag_col);
            0
        });
        let idx_secs = header.iter().position(|h| h == secs_col).unwrap_or_else(|| {
            eprintln!("Warning: Column '{}' not found, using default index.", diag_col);
            0
        });
        let idx_nsecs = header.iter().position(|h| h == nsecs_col).unwrap_or_else(|| {
            eprintln!("Warning: Column '{}' not found, using default index.", diag_col);
            0
        });

        for (i, r) in rdr.records().enumerate() {
            let record = r?;
            if i == 0 || i == 1 {
                continue;
            }

            for (&gas_type, &idx) in &gas_indices {
                let value = record[idx].parse::<f64>().unwrap_or(f64::NAN);
                if let Some(gas_vector) = gas_data.get_mut(&gas_type) {
                    gas_vector.push(value);
                }
            }

            if let Ok(val) = record[idx_diag].parse::<i64>() {
                diag.push(val);
            }

            let sec = record[idx_secs].parse::<i64>()?;
            let nsec = record[idx_nsecs].parse::<i64>()?;
            let dt_utc = parse_secnsec_to_dt(sec, nsec);
            datetime.push(dt_utc);
        }

        let mut indices: Vec<usize> = (0..datetime.len()).collect();
        indices.sort_by(|&i, &j| datetime[i].cmp(&datetime[j]));

        let datetime: Vec<DateTime<Utc>> = indices.iter().map(|&i| datetime[i]).collect();
        let diag: Vec<i64> = indices.iter().map(|&i| diag[i]).collect();
        let mut sorted_gas_data: HashMap<GasType, Vec<Option<f64>>> = HashMap::new();

        for (&gas_type, gas_values) in &gas_data {
            sorted_gas_data
                .insert(gas_type, indices.iter().map(|&i| Some(gas_values[i])).collect());
        }

        let df = GasData {
            header,
            instrument_model,
            instrument_serial,
            datetime,
            gas: sorted_gas_data,
            diag,
        };
        Ok(df)
    }
}

pub fn parse_secnsec_to_dt(sec: i64, nsec: i64) -> DateTime<Utc> {
    match Helsinki.timestamp_opt(sec, nsec as u32) {
        LocalResult::Single(dt) => return dt.with_timezone(&Utc),
        LocalResult::Ambiguous(dt1, _) => return dt1.with_timezone(&Utc),
        LocalResult::None => {
            eprintln!("Impossible local time: sec={} nsec={}", sec, nsec);
        },
    };

    // Default fallback timestamp if parsing fails
    Utc.timestamp_opt(0, 0).single().unwrap() // Returns Unix epoch (1970-01-01 00:00:00 UTC)
}

pub fn get_instrument_by_model(model: InstrumentType) -> Option<Li7810> {
    match model {
        InstrumentType::Li7810 => Some(Li7810::default()),
        _ => None,
    }
}
