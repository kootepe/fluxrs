use crate::structs;
use crate::structs::GasData;
use chrono::offset::LocalResult;
use chrono::prelude::DateTime;
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Helsinki;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum Gas {
    CH4(Vec<f64>),
    CO2(Vec<f64>),
    H2O(Vec<f64>),
    N2O(Vec<f64>), // This variant is ignored in processing
}

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
impl GasType {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "CH4" => Some(GasType::CH4),
            "CO2" => Some(GasType::CO2),
            "H2O" => Some(GasType::H2O),
            "N2O" => Some(GasType::N2O),
            _ => None,
        }
    }
    fn column_name(&self) -> &'static str {
        match self {
            GasType::CH4 => "CH4",
            GasType::CO2 => "CO2",
            GasType::H2O => "H2O",
            GasType::N2O => "N2O",
        }
    }
}

pub struct Instrument {
    sep: u8,
    skiprows: i64,
    skip_after_header: i64,
    time_col: String,
    gas_cols: Vec<String>,
    diag_col: String,
    has_header: bool,
}

pub struct Li7810 {
    pub base: Instrument, // ✅ Composition: LI_7810 contains an Instrument
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
impl Default for Li7810 {
    fn default() -> Self {
        Self {
            base: Instrument {
                sep: b'\t',
                skiprows: 4,
                skip_after_header: 1,
                time_col: "SECONDS".to_string(),
                gas_cols: vec!["CO2".to_string(), "CH4".to_string(), "H2O".to_string()],
                diag_col: "DIAG".to_string(),
                has_header: true,
            },
        }
    }
}

impl Li7810 {
    pub fn mk_rdr<P: AsRef<Path>>(&self, filename: P) -> Result<csv::Reader<File>, Box<dyn Error>> {
        self.base.mk_rdr(filename)
    }
    pub fn read_data_file<P: AsRef<Path>>(&self, filename: P) -> Result<GasData, Box<dyn Error>> {
        let mut rdr = self.mk_rdr(filename)?;
        let skip = 4;
        for _ in 0..skip {
            rdr.records().next();
        }

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
            if let Some(gas_type) = GasType::from_str(h) {
                gas_indices.insert(gas_type, i);
                gas_data.insert(gas_type, Vec::new()); // Initialize gas vectors
            }
        }
        let idx_diag = header
            .iter()
            .position(|h| h == diag_col)
            .ok_or("Column not found")?;
        let idx_secs = header
            .iter()
            .position(|h| h == secs_col)
            .ok_or("Column not found")?;
        let idx_nsecs = header
            .iter()
            .position(|h| h == nsecs_col)
            .ok_or("Column not found")?;

        for (i, r) in rdr.records().enumerate() {
            let record = r?;
            if i == 0 || i == 1 {
                continue;
            }

            for (&gas_type, &idx) in &gas_indices {
                let value = record[idx].parse::<f64>().unwrap_or(structs::ERROR_FLOAT);
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
        let mut sorted_gas_data: HashMap<GasType, Vec<f64>> = HashMap::new();

        for (&gas_type, gas_values) in &gas_data {
            sorted_gas_data.insert(gas_type, indices.iter().map(|&i| gas_values[i]).collect());
        }

        let df = GasData {
            header,
            datetime,
            gas: sorted_gas_data,
            diag,
        };
        Ok(df)
    }
    // pub fn read_data_file<P: AsRef<Path>>(&self, filename: P) -> Result<GasData, Box<dyn Error>> {
    //     let mut rdr = self.mk_rdr(filename)?;
    //     let skip = 4;
    //
    //     for _ in 0..skip {
    //         rdr.records().next();
    //     }
    //
    //     let mut ch4: Vec<f64> = Vec::new();
    //     let mut co2: Vec<f64> = Vec::new();
    //     let mut diag: Vec<i64> = Vec::new();
    //     let mut datetime: Vec<DateTime<Utc>> = Vec::new();
    //     let mut secs: Vec<i64> = Vec::new();
    //     let mut nsecs: Vec<i64> = Vec::new();
    //     let mut header = csv::StringRecord::new();
    //
    //     if let Some(result) = rdr.records().next() {
    //         header = result?;
    //     }
    //     let ch4_col = "CH4";
    //     let co2_col = "CO2";
    //     let diag_col = "DIAG";
    //     let secs_col = "SECONDS";
    //     let nsecs_col = "NANOSECONDS";
    //
    //     let idx_ch4 = header
    //         .iter()
    //         .position(|h| h == ch4_col)
    //         .ok_or("Column not found")?;
    //     let idx_co2 = header
    //         .iter()
    //         .position(|h| h == co2_col)
    //         .ok_or("Column not found")?;
    //     let idx_diag = header
    //         .iter()
    //         .position(|h| h == diag_col)
    //         .ok_or("Column not found")?;
    //     let idx_secs = header
    //         .iter()
    //         .position(|h| h == secs_col)
    //         .ok_or("Column not found")?;
    //     let idx_nsecs = header
    //         .iter()
    //         .position(|h| h == nsecs_col)
    //         .ok_or("Column not found")?;
    //     for (i, r) in rdr.records().enumerate() {
    //         let record: &csv::StringRecord = &r?;
    //         if i == 0 {
    //             header = record.clone();
    //             continue;
    //         }
    //         if i == 1 {
    //             continue;
    //         }
    //
    //         if let Ok(val) = record[idx_ch4].parse::<f64>() {
    //             ch4.push(val)
    //         } else {
    //             ch4.push(structs::ERROR_FLOAT)
    //         }
    //         if let Ok(val) = record[idx_co2].parse::<f64>() {
    //             co2.push(val)
    //         } else {
    //             co2.push(structs::ERROR_FLOAT)
    //         }
    //         if let Ok(val) = record[idx_diag].parse::<i64>() {
    //             diag.push(val)
    //         }
    //         let sec = record[idx_secs].parse::<i64>()?;
    //         let nsec = record[idx_nsecs].parse::<i64>()?;
    //         let dt_utc = parse_secnsec_to_dt(sec, nsec);
    //         datetime.push(dt_utc);
    //
    //         if let Ok(val) = record[idx_secs].parse::<i64>() {
    //             secs.push(val)
    //         }
    //         if let Ok(val) = record[idx_nsecs].parse::<i64>() {
    //             nsecs.push(val)
    //         }
    //     }
    //     let mut indices: Vec<usize> = (0..datetime.len()).collect();
    //     indices.sort_by(|&i, &j| datetime[i].cmp(&datetime[j]));
    //
    //     let datetime: Vec<chrono::DateTime<Utc>> = indices.iter().map(|&i| datetime[i]).collect();
    //     let gas: Vec<f64> = indices.iter().map(|&i| ch4[i]).collect();
    //     let diag: Vec<i64> = indices.iter().map(|&i| diag[i]).collect();
    //
    //     let df = GasData {
    //         header,
    //         datetime,
    //         gas,
    //         diag,
    //     };
    //     Ok(df)
    // }
    // pub fn read_csv<P: AsRef<Path>>(self, filename: P) -> Result<GasData, Box<dyn Error>> {
    //     // NOTE: currently generating a new reader for every file, is it possible to use the same
    //     // reader for multiple?
    //     let mut rdr = self.mk_rdr(filename)?;
    //     let skip = self.base.skiprows;
    //
    //     for _ in 0..skip {
    //         rdr.records().next();
    //     }
    //
    //     let mut gas_map: HashMap<GasType, Vec<f64>> = HashMap::new();
    //     let mut ch4: Vec<f64> = Vec::new();
    //     let mut co2: Vec<f64> = Vec::new();
    //     let mut diag: Vec<i64> = Vec::new();
    //     let mut datetime: Vec<DateTime<Utc>> = Vec::new();
    //     let mut secs: Vec<i64> = Vec::new();
    //     let mut nsecs: Vec<i64> = Vec::new();
    //     let mut header = csv::StringRecord::new();
    //
    //     if let Some(result) = rdr.records().next() {
    //         header = result?;
    //     }
    //     let diag_col = "DIAG";
    //     let secs_col = "SECONDS";
    //     let nsecs_col = "NANOSECONDS";
    //
    //     // let gas_indices = [GasType::CH4, GasType::CO2, GasType::H2O]
    //     let gas_indices: HashMap<GasType, usize> = [GasType::CH4, GasType::CO2, GasType::H2O]
    //         .iter()
    //         .filter_map(|gas| {
    //             header
    //                 .iter()
    //                 .position(|h| h == gas.column_name())
    //                 .map(|idx| (gas.clone(), idx))
    //         })
    //         .collect();
    //
    //     let idx_diag = header
    //         .iter()
    //         .position(|h| h == diag_col)
    //         .ok_or("Column not found")?;
    //     let idx_secs = header
    //         .iter()
    //         .position(|h| h == secs_col)
    //         .ok_or("Column not found")?;
    //     let idx_nsecs = header
    //         .iter()
    //         .position(|h| h == nsecs_col)
    //         .ok_or("Column not found")?;
    //     for (i, r) in rdr.records().enumerate() {
    //         let record: &csv::StringRecord = &r?;
    //         if i == 0 {
    //             header = record.clone();
    //             continue;
    //         }
    //         if i == 1 {
    //             continue;
    //         }
    //
    //         // if let Ok(val) = record[idx_ch4].parse::<f64>() {
    //         //     ch4.push(val)
    //         // } else {
    //         //     ch4.push(structs::ERROR_FLOAT)
    //         // }
    //         // if let Ok(val) = record[idx_co2].parse::<f64>() {
    //         //     co2.push(val)
    //         // } else {
    //         //     co2.push(structs::ERROR_FLOAT)
    //         // }
    //         for (gas_type, &idx) in &gas_indices {
    //             let value = record[idx].parse::<f64>().unwrap_or(structs::ERROR_FLOAT);
    //             gas_map
    //                 .entry(*gas_type)
    //                 .or_insert_with(Vec::new)
    //                 .push(value);
    //         }
    //         if let Ok(val) = record[idx_diag].parse::<i64>() {
    //             diag.push(val)
    //         }
    //         let sec = record[idx_secs].parse::<i64>()?;
    //         let nsec = record[idx_nsecs].parse::<i64>()?;
    //         let dt_utc = parse_secnsec_to_dt(sec, nsec);
    //         datetime.push(dt_utc);
    //
    //         if let Ok(val) = record[idx_secs].parse::<i64>() {
    //             secs.push(val)
    //         }
    //         if let Ok(val) = record[idx_nsecs].parse::<i64>() {
    //             nsecs.push(val)
    //         }
    //     }
    //     let mut indices: Vec<usize> = (0..datetime.len()).collect();
    //     indices.sort_by(|&i, &j| datetime[i].cmp(&datetime[j]));
    //
    //     let datetime: Vec<chrono::DateTime<Utc>> = indices.iter().map(|&i| datetime[i]).collect();
    //     // let gas: Vec<f64> = indices.iter().map(|&i| ch4[i]).collect();
    //     let diag: Vec<i64> = indices.iter().map(|&i| diag[i]).collect();
    //     // let gas: Vec<Gas> = gas_map
    //     //     .into_iter()
    //     //     .filter_map(|(gas_type, values)| match gas_type {
    //     //         GasType::CH4 => Some(Gas::CH4(values)),
    //     //         GasType::CO2 => Some(Gas::CO2(values)),
    //     //         GasType::H2O => Some(Gas::H2O(values)),
    //     //         _ => None,
    //     //     })
    //     //     .collect();
    //     let gas = ch4;
    //
    //     let df = GasData {
    //         header,
    //         datetime,
    //         gas,
    //         diag,
    //     };
    //     Ok(df)
    // }
}

pub fn parse_secnsec_to_dt(sec: i64, nsec: i64) -> DateTime<Utc> {
    match Helsinki.timestamp_opt(sec, nsec as u32) {
        LocalResult::Single(dt) => return dt.with_timezone(&Utc),
        LocalResult::Ambiguous(dt1, _) => return dt1.with_timezone(&Utc),
        LocalResult::None => {
            eprintln!("Impossible local time: sec={} nsec={}", sec, nsec);
        }
    };

    // Default fallback timestamp if parsing fails
    Utc.timestamp_opt(0, 0).single().unwrap() // Returns Unix epoch (1970-01-01 00:00:00 UTC)
}
