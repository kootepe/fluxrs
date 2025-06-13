use crate::gasdata::GasData;
use crate::validation_app::GasKey;
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
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
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
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstrumentType {
    #[default]
    LI7810,
    LI7820,
    Other, // Placeholder for additional instruments
}

impl fmt::Display for InstrumentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InstrumentType::LI7810 => write!(f, "LI-7810"),
            InstrumentType::LI7820 => write!(f, "LI-7820"),
            InstrumentType::Other => write!(f, "Other"),
        }
    }
}

impl InstrumentType {
    /// Convert a `&str` into an `InstrumentType`
    pub fn from_str(s: &str) -> Self {
        match s {
            "LI-7810" => InstrumentType::LI7810,
            "LI-7820" => InstrumentType::LI7820,
            "LI7810" => InstrumentType::LI7810,
            "LI7820" => InstrumentType::LI7820,
            "li-7810" => InstrumentType::LI7810,
            "li-7820" => InstrumentType::LI7820,
            "li7810" => InstrumentType::LI7810,
            "li7820" => InstrumentType::LI7820,
            _ => InstrumentType::Other,
        }
    }

    /// Return a list of available instruments (for UI dropdown)
    pub fn available_instruments() -> Vec<InstrumentType> {
        vec![InstrumentType::LI7810, InstrumentType::LI7820, InstrumentType::Other]
        // Expand this list as needed
    }
    pub fn available_gases(&self) -> Vec<GasType> {
        match self {
            InstrumentType::LI7810 => InstrumentConfig::li7810().available_gases,
            InstrumentType::LI7820 => InstrumentConfig::li7820().available_gases,
            InstrumentType::Other => vec![GasType::N2O], // Example for another instrument
        }
    }
}

#[derive(Debug, Clone)]
pub enum TimeSource {
    /// For data that has SECONDS and optional NANOSECONDS
    SecsNsecs(i64, i64),
    /// For data that has a full timestamp string (e.g., ISO 8601)
    StringTimestamp(String, String),
    /// For data that only has SECONDS
    SecondsOnly(i64),
}

impl TimeSource {
    pub fn to_datetime(&self) -> DateTime<Utc> {
        match self {
            TimeSource::SecsNsecs(sec, nsec) => parse_secnsec_to_dt(*sec, *nsec),
            TimeSource::SecondsOnly(sec) => parse_secnsec_to_dt(*sec, 0),
            TimeSource::StringTimestamp(s, fmt) => NaiveDateTime::parse_from_str(s, fmt)
                .ok()
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
                .unwrap(),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum TimeSourceKind {
    Seconds,
    SecondsAndNanos,
    StringFormat,
}
#[derive(Debug, Clone)]
pub struct InstrumentConfig {
    pub name: String,
    pub model: String,
    pub sep: u8,
    pub skiprows: usize,
    pub skip_after_header: usize,
    pub time_col: String,
    pub nsecs_col: String,
    pub gas_cols: Vec<String>,
    pub flux_cols: Vec<String>,
    pub diag_col: String,
    pub has_header: bool,
    pub available_gases: Vec<GasType>,
    pub time_source: TimeSourceKind,
    pub time_fmt: Option<String>,
}
impl fmt::Display for InstrumentConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}, {}", self.model, self.name,)
    }
}
impl InstrumentConfig {
    pub fn li7810() -> Self {
        Self {
            name: "LI-7810".to_string(),
            model: "LI-7810".to_string(),
            sep: b'\t',
            skiprows: 3,
            skip_after_header: 1,
            time_col: "SECONDS".to_string(),
            nsecs_col: "NANOSECONDS".to_string(),
            gas_cols: vec!["CO2".to_string(), "CH4".to_string(), "H2O".to_string()],
            flux_cols: vec!["CO2".to_string(), "CH4".to_string()],
            diag_col: "DIAG".to_string(),
            has_header: false,
            available_gases: vec![GasType::CO2, GasType::CH4, GasType::H2O],
            time_source: TimeSourceKind::SecondsAndNanos,
            time_fmt: None,
        }
    }
    pub fn li7820() -> Self {
        Self {
            name: "LI-7820".to_string(),
            model: "LI-7820".to_string(),
            sep: b'\t',
            skiprows: 3,
            skip_after_header: 1,
            time_col: "SECONDS".to_owned(),
            nsecs_col: "NANOSECONDS".to_owned(),
            gas_cols: vec!["N2O".to_owned(), "H2O".to_owned()],
            flux_cols: vec!["N2O".to_owned()],
            diag_col: "DIAG".to_owned(),
            has_header: false,
            available_gases: vec![GasType::N2O, GasType::H2O],
            time_source: TimeSourceKind::SecondsAndNanos,
            time_fmt: None,
        }
    }

    pub fn read_data_file<P: AsRef<Path>>(&self, filename: P) -> Result<GasData, Box<dyn Error>> {
        let file = File::open(filename)?;
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(self.sep)
            .has_headers(self.has_header)
            .flexible(true)
            .from_reader(file);

        let mut instrument_serial = String::new();
        let mut instrument_model = String::new();

        if let Some(result) = rdr.records().next() {
            instrument_model = result?.get(1).unwrap_or("").to_string();
        }
        if let Some(result) = rdr.records().next() {
            instrument_serial = result?.get(1).unwrap_or("").to_string();
        }
        if instrument_model != self.model {
            return Err("Given instrument model and file instrument model don't match.".into());
        }

        for _ in 0..self.skiprows {
            rdr.records().next();
        }

        let mut gas_data: HashMap<GasType, Vec<f64>> = HashMap::new();
        // let mut diag: Vec<i64> = Vec::new();
        let mut datetime_map: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();
        let mut diag_map: HashMap<String, Vec<i64>> = HashMap::new();
        let mut header = csv::StringRecord::new();

        if let Some(result) = rdr.records().next() {
            header = result?;
        }

        let mut gas_indices: HashMap<GasType, usize> = HashMap::new();
        let idx_diag = header.iter().position(|h| h == self.diag_col).unwrap_or(0);
        let idx_secs = header.iter().position(|h| h == self.time_col).unwrap_or(0);
        let idx_nsecs = header.iter().position(|h| h == self.nsecs_col).unwrap_or(0);

        for gas_col in &self.gas_cols {
            if let Some((i, _)) = header.iter().enumerate().find(|(_, h)| h == gas_col) {
                if let Ok(gas_type) = gas_col.parse::<GasType>() {
                    gas_indices.insert(gas_type, i);
                    gas_data.insert(gas_type, Vec::new());
                } else {
                    eprintln!("Warning: Could not parse gas column '{}' as GasType", gas_col);
                }
            } else {
                eprintln!("Warning: Gas column '{}' not found in header", gas_col);
            }
        }

        for record in rdr.records().skip(self.skip_after_header) {
            let record = record?;
            // println!("{:?}", record);

            for (&gas_type, &idx) in &gas_indices {
                let value = record.get(idx).unwrap_or("NaN").parse::<f64>().unwrap_or(f64::NAN);
                if let Some(gas_vector) = gas_data.get_mut(&gas_type) {
                    gas_vector.push(value);
                }
            }

            if let Ok(val) = record.get(idx_diag).unwrap_or("0").parse::<i64>() {
                diag_map.entry(instrument_serial.clone()).or_default().push(val);
            }

            let timestamp = match self.time_source {
                TimeSourceKind::Seconds => {
                    let sec = record.get(idx_secs).unwrap_or("0").parse::<i64>()?;
                    parse_secnsec_to_dt(sec, 0)
                },
                TimeSourceKind::SecondsAndNanos => {
                    let sec = record.get(idx_secs).unwrap_or("0").parse::<i64>()?;
                    let nsec = record.get(idx_nsecs).unwrap_or("0").parse::<i64>()?;
                    parse_secnsec_to_dt(sec, nsec)
                },
                TimeSourceKind::StringFormat => {
                    let time_str = record.get(idx_secs).unwrap_or("");
                    let fmt = self.time_fmt.as_deref().unwrap_or("%Y-%m-%d %H:%M:%S");
                    let naive = NaiveDateTime::parse_from_str(time_str, fmt)?;
                    DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
                },
            };

            datetime_map.entry(instrument_serial.clone()).or_default().push(timestamp);
        }

        // Sort timestamps and align data accordingly
        let mut datetime_sorted: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();
        let mut diag_sorted: HashMap<String, Vec<i64>> = HashMap::new();
        // let mut diag_sorted: Vec<i64> = Vec::new();
        let mut sorted_gas_data: HashMap<GasKey, Vec<Option<f64>>> = HashMap::new();

        if let Some(dt_list) = datetime_map.get(&instrument_serial) {
            let mut indices: Vec<usize> = (0..dt_list.len()).collect();
            indices.sort_by_key(|&i| dt_list[i]);

            datetime_sorted
                .insert(instrument_serial.clone(), indices.iter().map(|&i| dt_list[i]).collect());
            diag_sorted.insert(
                instrument_serial.clone(),
                indices.iter().map(|&i| diag_map.get(&instrument_serial).unwrap()[i]).collect(),
            );

            // diag_sorted = indices.iter().map(|&i| diag[i]).collect();

            for (&gas_type, values) in &gas_data {
                let sorted: Vec<_> = indices.iter().map(|&i| Some(values[i])).collect();
                sorted_gas_data
                    .insert(GasKey::from((&gas_type, instrument_serial.as_str())), sorted);
            }
        }

        let mut model_key = HashMap::new();
        model_key.insert(instrument_serial.clone(), InstrumentType::from_str(&self.model.clone()));

        Ok(GasData {
            header,
            instrument_model: self.model.clone(),
            instrument_serial,
            model_key,
            datetime: datetime_sorted,
            gas: sorted_gas_data,
            diag: diag_sorted,
        })
    }
    // pub fn read_data_file<P: AsRef<Path>>(&self, filename: P) -> Result<GasData, Box<dyn Error>> {
    //     let file = File::open(filename)?;
    //     let mut rdr = csv::ReaderBuilder::new()
    //         .delimiter(self.sep)
    //         .has_headers(self.has_header)
    //         .flexible(true)
    //         .from_reader(file);
    //
    //     let mut instrument_serial = String::new();
    //
    //     if let Some(result) = rdr.records().next() {
    //         instrument_serial = result?.get(1).unwrap_or("").to_string();
    //     }
    //
    //     for _ in 0..self.skiprows {
    //         rdr.records().next();
    //     }
    //
    //     let mut gas_data: HashMap<GasType, Vec<f64>> = HashMap::new();
    //     let mut diag: Vec<i64> = Vec::new();
    //     let mut datetime: Vec<DateTime<Utc>> = Vec::new();
    //     // let mut time_sources: Vec<TimeSource> = Vec::new();
    //     let mut header = csv::StringRecord::new();
    //
    //     if let Some(result) = rdr.records().next() {
    //         header = result?;
    //     }
    //
    //     let mut gas_indices: HashMap<GasType, usize> = HashMap::new();
    //     let idx_diag = header.iter().position(|h| h == self.diag_col).unwrap_or(0);
    //     let idx_secs = header.iter().position(|h| h == self.time_col).unwrap_or(0);
    //     let idx_nsecs = header.iter().position(|h| h == self.nsecs_col).unwrap_or(0);
    //
    //     for gas_col in &self.gas_cols {
    //         if let Some((i, _)) = header.iter().enumerate().find(|(_, h)| h == gas_col) {
    //             if let Ok(gas_type) = gas_col.parse::<GasType>() {
    //                 gas_indices.insert(gas_type, i);
    //                 gas_data.insert(gas_type, Vec::new());
    //             } else {
    //                 eprintln!("Warning: Could not parse gas column '{}' as GasType", gas_col);
    //             }
    //         } else {
    //             eprintln!("Warning: Gas column '{}' not found in header", gas_col);
    //         }
    //     }
    //
    //     for record in rdr.records().skip(self.skip_after_header) {
    //         let record = record?;
    //
    //         for (&gas_type, &idx) in &gas_indices {
    //             let value = record.get(idx).unwrap_or("NaN").parse::<f64>().unwrap_or(f64::NAN);
    //             if let Some(gas_vector) = gas_data.get_mut(&gas_type) {
    //                 gas_vector.push(value);
    //             }
    //         }
    //
    //         if let Ok(val) = record.get(idx_diag).unwrap_or("0").parse::<i64>() {
    //             diag.push(val);
    //         }
    //
    //         let timestamp = match self.time_source {
    //             TimeSourceKind::Seconds => {
    //                 let sec = record.get(idx_secs).unwrap_or("0").parse::<i64>()?;
    //                 parse_secnsec_to_dt(sec, 0)
    //             },
    //             TimeSourceKind::SecondsAndNanos => {
    //                 let sec = record.get(idx_secs).unwrap_or("0").parse::<i64>()?;
    //                 let nsec = record.get(idx_nsecs).unwrap_or("0").parse::<i64>()?;
    //                 parse_secnsec_to_dt(sec, nsec)
    //             },
    //             TimeSourceKind::StringFormat => {
    //                 let time_str = record.get(idx_secs).unwrap_or("");
    //                 let fmt = self.time_fmt.as_deref().unwrap_or("%Y-%m-%d %H:%M:%S");
    //                 let naive = NaiveDateTime::parse_from_str(time_str, fmt)?;
    //                 DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
    //             },
    //         };
    //         datetime.push(timestamp);
    //     }
    //
    //     let mut indices: Vec<usize> = (0..datetime.len()).collect();
    //     indices.sort_by_key(|&i| datetime[i]);
    //
    //     let datetime: Vec<_> = indices.iter().map(|&i| datetime[i]).collect();
    //     let diag: Vec<_> = indices.iter().map(|&i| diag[i]).collect();
    //
    //     let mut sorted_gas_data: HashMap<GasKey, Vec<Option<f64>>> = HashMap::new();
    //     for (&gas_type, values) in &gas_data {
    //         let sorted: Vec<_> = indices.iter().map(|&i| Some(values[i])).collect();
    //         sorted_gas_data.insert((gas_type, instrument_serial.clone()), sorted);
    //     }
    //     let mut model_key = HashMap::new();
    //     model_key.insert(instrument_serial.clone(), InstrumentType::from_str(&self.model.clone()));
    //
    //     Ok(GasData {
    //         header,
    //         instrument_model: self.model.clone(),
    //         instrument_serial,
    //         model_key,
    //         datetime,
    //         gas: sorted_gas_data,
    //         diag,
    //     })
    // }
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

pub fn get_instrument_by_model(model: InstrumentType) -> Option<InstrumentConfig> {
    match model {
        InstrumentType::LI7810 => Some(InstrumentConfig::li7810()),
        InstrumentType::LI7820 => Some(InstrumentConfig::li7820()),
        _ => None,
    }
}
