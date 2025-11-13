use crate::concentrationunit::ConcentrationUnit;
use crate::cycle::gaskey::GasKey;
use crate::data_formats::gasdata::insert_measurements;
use crate::datatype::DataType;
use crate::gaschannel::{ChannelConfig, GasChannel};
use crate::gastype::GasType;
use crate::processevent::{InsertEvent, ProcessEvent, QueryEvent, ReadEvent};
use crate::project::Project;
use crate::utils::get_or_insert_data_file;
use chrono::{DateTime, LocalResult, NaiveDateTime, TimeZone};
use chrono_tz::{Tz, UTC};
use rusqlite::{params, types::ValueRef, Connection, Result};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::sync::mpsc;

pub struct InstrumentMeasurement {
    pub instrument: Instrument,
    pub datetime: Vec<i64>,
    pub gas: HashMap<GasType, Vec<Option<f64>>>,
    pub diag: Vec<i64>,
}

impl InstrumentMeasurement {
    pub fn validate_lengths(&self) -> bool {
        // check that all fields are equal length
        let mut gas_lengths = Vec::new();
        for gas in self.instrument.model.available_gases() {
            let len = self.gas.get(&gas).unwrap().len();
            gas_lengths.push(len);
        }
        let lengths = [&self.datetime.len(), &self.gas.len(), &self.diag.len()];
        let mut check: bool = true;

        for vec_len in lengths.iter() {
            let len = vec_len;
            if vec_len != len {
                check = false;
                break;
            } else {
                continue;
            };
        }
        check
    }
}
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Instrument {
    // id in instruments table from db
    pub id: Option<i64>,
    pub model: InstrumentType,
    pub serial: String,
}

impl fmt::Display for Instrument {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "model: {}, serial: {}", self.model, self.serial,)
    }
}

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

    pub fn read_data_file<P: AsRef<Path>>(
        &self,
        filename: P,
    ) -> Result<InstrumentMeasurement, Box<dyn Error>> {
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

        for _ in 0..self.skiprows {
            rdr.records().next();
        }
        if let Some(result) = rdr.records().next() {
            tz_str = result?.get(1).unwrap_or("").to_string();
        }

        let mut gas_data: HashMap<GasType, Vec<f64>> = HashMap::new();
        let mut datetime_vec: Vec<i64> = Vec::new();
        let mut diag_vec: Vec<i64> = Vec::new();
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
                return Err(format!("Warning: Gas column '{}' not found in header", gas_col).into());
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
                diag_vec.push(val);
            }

            let timestamp = match self.time_source {
                TimeSourceKind::Seconds => {
                    let sec = record.get(idx_secs).unwrap_or("0").parse::<i64>()?;
                    parse_secnsec_to_dt(sec, 0, tz_str.clone())
                },
                TimeSourceKind::SecondsAndNanos => {
                    let sec = record.get(idx_secs).unwrap_or("0").parse::<i64>()?;
                    let nsec = record.get(idx_nsecs).unwrap_or("0").parse::<u32>()?;
                    parse_secnsec_to_dt(sec, nsec, tz_str.clone())
                },
                TimeSourceKind::StringFormat => {
                    let time_str = record.get(idx_secs).unwrap_or("");
                    let fmt = self.time_fmt.as_deref().unwrap_or("%Y-%m-%d %H:%M:%S");
                    let tz: Tz = tz_str.parse().expect("Invalid timezone string");
                    parse_local_in_tz(time_str, fmt, tz)
                },
            };

            datetime_vec.push(timestamp.timestamp());
        }

        // Sort timestamps and align data accordingly
        let mut datetime_sorted: Vec<i64> = Vec::new();
        let mut diag_sorted: Vec<i64> = Vec::new();
        // let mut diag_sorted: Vec<i64> = Vec::new();
        let mut sorted_gas_data: HashMap<GasKey, Vec<Option<f64>>> = HashMap::new();
        let mut instruments: HashSet<Instrument> = HashSet::new();

        let mut indices: Vec<usize> = (0..datetime_vec.len()).collect();

        // Sort indices based on the datetime values
        indices.sort_by_key(|&i| datetime_vec[i]);

        // Sort datetime and diag vectors using the same ordering
        let datetime_sorted: Vec<i64> = indices.iter().map(|&i| datetime_vec[i]).collect();
        let diag_sorted: Vec<i64> = indices.iter().map(|&i| diag_vec[i]).collect();

        // Sort all gas data vectors consistently
        let mut sorted_gas_data: HashMap<GasType, Vec<Option<f64>>> = HashMap::new();
        for (&gas_type, values) in &gas_data {
            let sorted: Vec<_> = indices.iter().map(|&i| Some(values[i])).collect();
            sorted_gas_data.insert(gas_type, sorted);
        }
        if instrument_model != self.model {
            return Err("Given instrument model and file instrument model don't match.".into());
        }

        // model_key.insert(instrument_serial.clone(), InstrumentType::from_str(&self.model.clone()));
        // let model_string = self.model.clone().parse::<InstrumentType>().ok();
        let ins_model = match self.model.parse::<InstrumentType>() {
            Ok(val) => val,
            Err(_) => {
                return Err(format!(
                    "Unexpected invalid instrument type from DB: '{}'",
                    self.model
                )
                .into());
            },
        };

        let instrument = Instrument { model: ins_model, serial: instrument_serial, id: None };

        Ok(InstrumentMeasurement {
            instrument,
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

pub fn parse_secnsec_to_dt(sec: i64, nsec: u32, tz_str: String) -> DateTime<Tz> {
    let tz: Tz = tz_str.parse().expect("Invalid timezone string");
    match tz.timestamp_opt(sec, nsec) {
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
pub fn get_or_insert_instrument(
    conn: &Connection,
    instrument: &Instrument,
    project_id: i64,
) -> Result<i64> {
    // First, check if the file already exists for this project
    if let Ok(existing_id) = conn.query_row(
        "SELECT id FROM instruments WHERE instrument_serial = ?1 AND project_link = ?2",
        params![instrument.serial, project_id],
        |row| row.get::<_, i64>(0),
    ) {
        // Found existing entry
        return Ok(existing_id);
    }

    // If not found, insert it
    conn.execute(
        "INSERT INTO instruments (instrument_model, instrument_serial, project_link) VALUES (?1, ?2, ?3)",
        params![instrument.model.to_string(), instrument.serial, project_id],
    )?;

    // Return the new ID
    Ok(conn.last_insert_rowid())
}
pub fn upload_gas_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    for path in &selected_paths {
        let mut instrument = match project.instrument.model {
            InstrumentType::LI7810 => Some(InstrumentConfig::li7810()),
            InstrumentType::LI7820 => Some(InstrumentConfig::li7820()),
        };
        if let Some(upload_type) = project.upload_from {
            instrument = match upload_type {
                InstrumentType::LI7810 => Some(InstrumentConfig::li7810()),
                InstrumentType::LI7820 => Some(InstrumentConfig::li7820()),
            };
        }
        if let Some(ref mut inst) = instrument {
            match inst.read_data_file(path) {
                Ok(data) => {
                    // inst.serial = Some(data.instrument.serial.clone());
                    if data.validate_lengths() {
                        let _rows = data.datetime.len();
                        let project_id = project.id.unwrap();

                        let file_name = match path.file_name().and_then(|n| n.to_str()) {
                            Some(name) => name,
                            None => {
                                eprintln!("Skipping path with invalid filename: {:?}", path);
                                // Optionally notify UI:
                                let _ =
                                    progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                                        path.to_string_lossy().to_string(),
                                        "Invalid file name (non-UTF8)".to_string(),
                                    )));
                                return (); // or `continue` if this is in a loop
                            },
                        };
                        let tx = match conn.transaction() {
                            Ok(tx) => tx,
                            Err(e) => {
                                eprintln!("Failed to start transaction: {}", e);
                                let _ = progress_sender.send(ProcessEvent::Insert(
                                    InsertEvent::Fail(format!(
                                        "Could not start transaction for '{}': {}",
                                        file_name, e
                                    )),
                                ));
                                continue;
                            },
                        };
                        let file_id = match get_or_insert_data_file(
                            &tx,
                            DataType::Gas,
                            file_name,
                            project_id,
                        ) {
                            Ok(id) => id,
                            Err(e) => {
                                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                                // Optionally notify UI
                                let _ =
                                    progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(
                                        format!("File '{}' skipped: {}", file_name, e),
                                    )));
                                continue; // or return if not inside a loop
                            },
                        };

                        match insert_measurements(&tx, &data, project, &file_id) {
                            Ok((count, duplicates)) => {
                                let _ = progress_sender.send(ProcessEvent::Insert(
                                    InsertEvent::OkSkip(count, duplicates),
                                ));
                                let _ = progress_sender.send(ProcessEvent::Read(
                                    ReadEvent::FileDetail(
                                        path.to_string_lossy().to_string(),
                                        format!("from {}", instrument.unwrap()),
                                    ),
                                ));
                                if let Err(e) = tx.commit() {
                                    eprintln!(
                                        "Failed to commit transaction for '{}': {}",
                                        file_name, e
                                    );
                                    let _ = progress_sender.send(ProcessEvent::Insert(
                                        InsertEvent::Fail(format!(
                                            "Commit failed for file '{}': {}",
                                            file_name, e
                                        )),
                                    ));
                                    // no commit = rollback
                                    continue;
                                }
                            },
                            Err(e) => {
                                println!("{}", e);
                                let _ = progress_sender.send(ProcessEvent::Insert(
                                    InsertEvent::Fail(format!("{}", e)),
                                ));
                            },
                        }
                    }
                },
                Err(e) => {
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                        path.to_str().unwrap().to_owned(),
                        e.to_string(),
                    )));
                },
            }
        }
        let _ = progress_sender.send(ProcessEvent::Query(QueryEvent::InitEnded));
    }
    let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
}
