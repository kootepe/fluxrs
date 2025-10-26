use crate::concentrationunit::ConcentrationUnit;
use crate::data_formats::gasdata::GasData;
use crate::gaschannel::{ChannelConfig, GasChannel};
use crate::gastype::GasType;
use crate::ui::validation_ui::GasKey;
use chrono::offset::LocalResult;
use chrono::prelude::DateTime;
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::{Tz, UTC};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::Path;
use std::str::FromStr;

#[derive(Debug)]
pub struct ParseInstrumentError(String);

impl fmt::Display for ParseInstrumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ParseInstrumentError {}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstrumentType {
    #[default]
    LI7810,
    LI7820,
}

impl fmt::Display for InstrumentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InstrumentType::LI7810 => write!(f, "LI-7810"),
            InstrumentType::LI7820 => write!(f, "LI-7820"),
        }
    }
}

impl FromStr for InstrumentType {
    type Err = ParseInstrumentError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "li-7810" => Ok(InstrumentType::LI7810),
            "li-7820" => Ok(InstrumentType::LI7820),
            "li7810" => Ok(InstrumentType::LI7810),
            "li7820" => Ok(InstrumentType::LI7820),
            other => Err(ParseInstrumentError(format!("invalid instrument: {other}"))),
        }
    }
}

impl InstrumentType {
    /// Return a list of available instruments (for UI dropdown)
    pub fn available_instruments() -> Vec<InstrumentType> {
        vec![InstrumentType::LI7810, InstrumentType::LI7820]
        // Expand this list as needed
    }
    pub fn available_gases(&self) -> Vec<GasType> {
        match self {
            InstrumentType::LI7810 => InstrumentConfig::li7810().available_gases,
            InstrumentType::LI7820 => InstrumentConfig::li7820().available_gases,
        }
    }
    pub fn get_config(&self) -> InstrumentConfig {
        match self {
            InstrumentType::LI7810 => InstrumentConfig::li7810(),
            InstrumentType::LI7820 => InstrumentConfig::li7820(),
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
    // pub fn to_datetime(&self) -> DateTime<Utc> {
    //     match self {
    //         TimeSource::SecsNsecs(sec, nsec) => parse_secnsec_to_dt(*sec, *nsec),
    //         TimeSource::SecondsOnly(sec) => parse_secnsec_to_dt(*sec, 0),
    //         TimeSource::StringTimestamp(s, fmt) => NaiveDateTime::parse_from_str(s, fmt)
    //             .ok()
    //             .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
    //             .unwrap(),
    //     }
    // }
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
    pub serial: Option<String>,
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
    pub channels: Vec<ChannelConfig>,
    pub time_source: TimeSourceKind,
    pub time_fmt: Option<String>,
}
impl fmt::Display for InstrumentConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.serial {
            None => write!(f, "{}", self.model),
            _ => write!(f, "{} {}", self.model, self.serial.clone().unwrap()),
        }
    }
}
impl InstrumentConfig {
    pub fn li7810() -> Self {
        Self {
            name: "LI-7810".to_owned(),
            model: "LI-7810".to_owned(),
            serial: None,
            sep: b'\t',
            skiprows: 2,
            skip_after_header: 1,
            time_col: "SECONDS".to_owned(),
            nsecs_col: "NANOSECONDS".to_owned(),
            gas_cols: vec!["CO2".to_owned(), "CH4".to_owned(), "H2O".to_owned()],
            flux_cols: vec!["CO2".to_owned(), "CH4".to_owned()],
            diag_col: "DIAG".to_owned(),
            has_header: false,
            available_gases: vec![GasType::CO2, GasType::CH4, GasType::H2O],
            channels: vec![
                ChannelConfig {
                    gas: GasType::CO2,
                    concentration_col: "CO2".to_owned(),
                    unit: ConcentrationUnit::Ppm,
                    instrument_id: "LI-7810".to_owned(),
                },
                ChannelConfig {
                    gas: GasType::CH4,
                    concentration_col: "CH4".to_owned(),
                    unit: ConcentrationUnit::Ppb,
                    instrument_id: "LI-7810".to_owned(),
                },
                ChannelConfig {
                    gas: GasType::H2O,
                    concentration_col: "H2O".to_owned(),
                    unit: ConcentrationUnit::Ppm,
                    instrument_id: "LI-7810".to_owned(),
                },
            ],
            time_source: TimeSourceKind::SecondsAndNanos,
            time_fmt: None,
        }
    }
    pub fn li7820() -> Self {
        Self {
            name: "LI-7820".to_string(),
            model: "LI-7820".to_string(),
            serial: None,
            sep: b'\t',
            skiprows: 2,
            skip_after_header: 1,
            time_col: "SECONDS".to_owned(),
            nsecs_col: "NANOSECONDS".to_owned(),
            gas_cols: vec!["N2O".to_owned(), "H2O".to_owned()],
            flux_cols: vec!["N2O".to_owned()],
            diag_col: "DIAG".to_owned(),
            has_header: false,
            available_gases: vec![GasType::N2O, GasType::H2O],
            channels: vec![
                ChannelConfig {
                    gas: GasType::N2O,
                    concentration_col: "N2O".to_owned(),
                    unit: ConcentrationUnit::Ppb,
                    instrument_id: "LI-7820".to_owned(),
                },
                ChannelConfig {
                    gas: GasType::H2O,
                    concentration_col: "H2O".to_owned(),
                    unit: ConcentrationUnit::Ppm,
                    instrument_id: "LI-7820".to_owned(),
                },
            ],
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
        let mut tz_str = String::new();

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
        if let Some(result) = rdr.records().next() {
            tz_str = result?.get(1).unwrap_or("").to_string();
        }

        let mut gas_data: HashMap<GasType, Vec<f64>> = HashMap::new();
        // let mut diag: Vec<i64> = Vec::new();
        let mut datetime_map: HashMap<String, Vec<DateTime<Tz>>> = HashMap::new();
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
                    parse_secnsec_to_dt(sec, 0, tz_str.clone())
                },
                TimeSourceKind::SecondsAndNanos => {
                    let sec = record.get(idx_secs).unwrap_or("0").parse::<i64>()?;
                    let nsec = record.get(idx_nsecs).unwrap_or("0").parse::<i64>()?;
                    parse_secnsec_to_dt(sec, nsec, tz_str.clone())
                },
                TimeSourceKind::StringFormat => {
                    let time_str = record.get(idx_secs).unwrap_or("");
                    let fmt = self.time_fmt.as_deref().unwrap_or("%Y-%m-%d %H:%M:%S");
                    parse_local_in_tz(time_str, fmt, UTC)
                },
            };

            datetime_map.entry(instrument_serial.clone()).or_default().push(timestamp);
        }

        // Sort timestamps and align data accordingly
        let mut datetime_sorted: HashMap<String, Vec<DateTime<Tz>>> = HashMap::new();
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
        // model_key.insert(instrument_serial.clone(), InstrumentType::from_str(&self.model.clone()));
        // let model_string = self.model.clone().parse::<InstrumentType>().ok();
        let model_string = match self.model.parse::<InstrumentType>() {
            Ok(val) => val,
            Err(_) => {
                return Err(format!(
                    "Unexpected invalid instrument type from DB: '{}'",
                    self.model
                )
                .into());
            },
        };

        model_key.insert(instrument_serial.clone(), model_string);

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

    pub fn gas_channels(&self) -> Vec<GasChannel> {
        self.channels
            .iter()
            .map(|ch| GasChannel {
                gas: ch.gas,
                unit: ch.unit,
                instrument_id: format!(
                    "{}{}",
                    self.model,
                    self.serial.as_deref().map(|s| format!(" S/N {}", s)).unwrap_or_default()
                ),
            })
            .collect()
    }
}

pub fn parse_secnsec_to_dt(sec: i64, nsec: i64, tz_str: String) -> DateTime<Tz> {
    let tz: Tz = tz_str.parse().expect("Invalid timezone string");
    match tz.timestamp_opt(sec, nsec as u32) {
        LocalResult::Single(dt) => return dt.with_timezone(&UTC),
        LocalResult::Ambiguous(dt1, _) => return dt1.with_timezone(&UTC),
        LocalResult::None => {
            eprintln!("Impossible local time: sec={} nsec={}", sec, nsec);
        },
    };

    // Default fallback timestamp if parsing fails
    UTC.timestamp_opt(0, 0).single().unwrap() // Returns Unix epoch (1970-01-01 00:00:00 UTC)
}

pub fn get_instrument_by_model(model: InstrumentType) -> Option<InstrumentConfig> {
    match model {
        InstrumentType::LI7810 => Some(InstrumentConfig::li7810()),
        InstrumentType::LI7820 => Some(InstrumentConfig::li7820()),
        _ => None,
    }
}

fn parse_local_in_tz<Tz: TimeZone>(time_str: &str, fmt: &str, tz: Tz) -> chrono::DateTime<Tz> {
    let naive = NaiveDateTime::parse_from_str(time_str, fmt).unwrap();
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt,
        LocalResult::Ambiguous(a, b) => {
            // pick one or surface an error; here we choose the earlier
            a
        },
        LocalResult::None => {
            // invalid local time (skipped by DST); decide how to handle
            tz.from_local_datetime(&(naive - chrono::Duration::hours(1))).unwrap()
        },
    }
}
