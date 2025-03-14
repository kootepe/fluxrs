use crate::instruments::InstrumentType;
use chrono::prelude::DateTime;
use chrono::Utc;
use rusqlite::Error;
use rusqlite::{params, Connection, Result};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::ops::{BitOr, BitOrAssign};
use std::{thread, time};

use csv::StringRecord;
// use std::error::Error;

use crate::gas_plot;
use crate::instruments::GasType;
use crate::query::get_nearest_meteo_data;
use crate::stats;

pub const ERROR_INT: i64 = -9999;
pub const ERROR_FLOAT: f64 = -9999.;
// the window of max r must be at least 240 seconds
pub const MIN_WINDOW_SIZE: usize = 180;
// how many seconds to increment the moving window searching for max r
pub const WINDOW_INCREMENT: usize = 5;

pub trait EqualLen {
    fn validate_lengths(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ErrorsInMeasurement,
    LowR,
    TooManyMeasurements,
    TooFewMeasurements,
    FewUnique,
    ManualInvalid,
    TooManyDiagErrors,
}

impl ErrorCode {
    // Define bitmask constants
    pub const DIAG_ERROR_IN_MEASUREMENT: u16 = 1 << 0;
    pub const LOW_R: u16 = 1 << 1;
    pub const FEW_UNIQUE: u16 = 1 << 2;
    pub const TOO_MANY_MEASUREMENTS: u16 = 1 << 3;
    pub const TOO_FEW_MEASUREMENTS: u16 = 1 << 4;
    pub const MANUAL_INVALID: u16 = 1 << 5;
    pub const MOSTLY_DIAG_ERRORS: u16 = 1 << 6;

    /// Convert an `ErrorCode` to its corresponding bitmask
    pub fn to_mask(&self) -> u16 {
        match self {
            ErrorCode::ErrorsInMeasurement => Self::DIAG_ERROR_IN_MEASUREMENT,
            ErrorCode::LowR => Self::LOW_R,
            ErrorCode::FewUnique => Self::FEW_UNIQUE,
            ErrorCode::TooManyMeasurements => Self::TOO_MANY_MEASUREMENTS,
            ErrorCode::TooFewMeasurements => Self::TOO_FEW_MEASUREMENTS,
            ErrorCode::ManualInvalid => Self::MANUAL_INVALID,
            ErrorCode::TooManyDiagErrors => Self::MOSTLY_DIAG_ERRORS,
        }
    }

    /// Convert a bitmask into a list of `ErrorCode` values
    pub fn from_mask(mask: u16) -> Vec<ErrorCode> {
        let mut errors = Vec::new();
        for error in [
            ErrorCode::ErrorsInMeasurement,
            ErrorCode::LowR,
            ErrorCode::FewUnique,
            ErrorCode::TooManyMeasurements,
            ErrorCode::TooFewMeasurements,
            ErrorCode::ManualInvalid,
            ErrorCode::TooManyDiagErrors,
        ] {
            if mask & error.to_mask() != 0 {
                errors.push(error);
            }
        }
        errors
    }
}

/// Wrapper struct for managing the error bitmask
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorMask(pub u16);

/// Implement `BitOr` for `ErrorCode`, allowing `ErrorCode | ErrorCode`
impl BitOr for ErrorCode {
    type Output = ErrorMask;
    fn bitor(self, rhs: Self) -> Self::Output {
        ErrorMask(self.to_mask() | rhs.to_mask())
    }
}

/// Implement `BitOrAssign<ErrorCode>` for `ErrorMask` (allow `ErrorMask |= ErrorCode`)
impl BitOrAssign<ErrorCode> for ErrorMask {
    fn bitor_assign(&mut self, rhs: ErrorCode) {
        self.0 |= rhs.to_mask();
    }
}

/// Implement `BitOrAssign<ErrorMask>` for `ErrorMask` (allow `ErrorMask |= ErrorMask`)
impl BitOrAssign<ErrorMask> for ErrorMask {
    fn bitor_assign(&mut self, rhs: ErrorMask) {
        self.0 |= rhs.0;
    }
}

impl ErrorMask {
    pub fn from_u16(value: u16) -> Self {
        ErrorMask(value)
    }
}

/// Implement `Display` for error messages
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let message = match self {
            ErrorCode::ErrorsInMeasurement => "Instrument diagnostic errors in measurement",
            ErrorCode::LowR => "Low r value",
            ErrorCode::FewUnique => "Too few unique values",
            ErrorCode::TooManyMeasurements => "Too many values",
            ErrorCode::TooFewMeasurements => "Too few values",
            ErrorCode::ManualInvalid => "Manual invalid",
            ErrorCode::TooManyDiagErrors => "Too many instrument diagnostic errors",
        };
        write!(f, "{}", message)
    }
}

pub struct CycleBuilder {
    chamber_id: Option<String>,
    start_time: Option<DateTime<Utc>>,
    close_offset: Option<i64>,
    open_offset: Option<i64>,
    end_offset: Option<i64>,
    project: Option<String>,
}

impl CycleBuilder {
    /// Create a new CycleBuilder
    pub fn new() -> Self {
        Self {
            chamber_id: None,
            start_time: None,
            close_offset: None,
            open_offset: None,
            end_offset: None,
            project: None,
        }
    }

    /// Set the chamber ID
    pub fn chamber_id(mut self, id: String) -> Self {
        self.chamber_id = Some(id);
        self
    }

    /// Set the start time
    pub fn start_time(mut self, time: DateTime<Utc>) -> Self {
        self.start_time = Some(time);
        self
    }

    /// Set the close offset (seconds from start)
    pub fn close_offset(mut self, offset: i64) -> Self {
        self.close_offset = Some(offset);
        self
    }

    /// Set the open offset (seconds from start)
    pub fn open_offset(mut self, offset: i64) -> Self {
        self.open_offset = Some(offset);
        self
    }

    /// Set the end offset (seconds from start)
    pub fn end_offset(mut self, offset: i64) -> Self {
        self.end_offset = Some(offset);
        self
    }
    pub fn project_name(mut self, project: String) -> Self {
        self.project = Some(project);
        self
    }

    /// Build the Cycle struct
    pub fn build_db(self) -> Result<Cycle, Error> {
        let start =
            self.start_time.ok_or(Error::InvalidColumnName("Start time is required".to_owned()))?;
        let chamber =
            self.chamber_id.ok_or(Error::InvalidColumnName("Chamber ID is required".to_owned()))?;
        let close = self
            .close_offset
            .ok_or(Error::InvalidColumnName("Close offset is required".to_owned()))?;
        let open = self
            .open_offset
            .ok_or(Error::InvalidColumnName("Open offset is required".to_owned()))?;
        let end =
            self.end_offset.ok_or(Error::InvalidColumnName("End offset is required".to_owned()))?;

        Ok(Cycle {
            chamber_id: chamber,
            start_time: start,
            instrument_model: InstrumentType::Li7810,
            instrument_serial: String::new(),
            project_name: String::new(),
            close_time: start + chrono::Duration::seconds(close),
            open_time: start + chrono::Duration::seconds(open),
            end_time: start + chrono::Duration::seconds(end),
            close_offset: close,
            open_offset: open,
            end_offset: end,
            main_gas: GasType::CH4,
            // has_errors: false,
            error_code: ErrorMask(0),
            manual_adjusted: false,
            // calc_range_end: (start + chrono::Duration::seconds(open)).timestamp() as f64,
            // calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            // gas_plot: HashMap::new(),
            calc_range_end: HashMap::new(),
            calc_range_start: HashMap::new(),
            min_y: HashMap::new(),
            max_y: HashMap::new(),
            // calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            lag_s: 0.,
            max_idx: 0.,
            flux: HashMap::new(),
            slope: HashMap::new(),
            calc_r2: HashMap::new(),
            measurement_r2: HashMap::new(),
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            diag_v: vec![],
            dt_v: vec![],
            gas_v: HashMap::new(),
            calc_gas_v: HashMap::new(),
            calc_dt_v: HashMap::new(),
            measurement_gas_v: HashMap::new(),
            measurement_dt_v: vec![],
            measurement_diag_v: Vec::new(),
            gases: vec![],
            air_pressure: 1000.,
            air_temperature: 10.,
            chamber_volume: 1.,
            is_valid: true,
            override_valid: None,
            manual_valid: false,
        })
    }
    pub fn build(self) -> Result<Cycle, Box<dyn std::error::Error>> {
        let start = self.start_time.ok_or("Start time is required")?;
        let chamber = self.chamber_id.ok_or("Chamber ID is required")?;
        let close = self.close_offset.ok_or("Close offset is required")?;
        let open = self.open_offset.ok_or("Open offset is required")?;
        let end = self.end_offset.ok_or("End offset is required")?;
        let project = self.project.ok_or("Project is required")?;

        Ok(Cycle {
            chamber_id: chamber,
            instrument_model: InstrumentType::Li7810,
            instrument_serial: String::new(),
            project_name: project,
            start_time: start,
            close_time: start + chrono::Duration::seconds(close),
            open_time: start + chrono::Duration::seconds(open),
            end_time: start + chrono::Duration::seconds(end),
            close_offset: close,
            open_offset: open,
            end_offset: end,
            // has_errors: false,
            error_code: ErrorMask(0),
            main_gas: GasType::CH4,
            manual_adjusted: false,
            // calc_range_end: (start + chrono::Duration::seconds(open)).timestamp() as f64,
            // calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            // gas_plot: HashMap::new(),
            calc_range_end: HashMap::new(),
            calc_range_start: HashMap::new(),
            min_y: HashMap::new(),
            max_y: HashMap::new(),
            // calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            lag_s: 0.,
            max_idx: 0.,
            flux: HashMap::new(),
            slope: HashMap::new(),
            calc_r2: HashMap::new(),
            measurement_r2: HashMap::new(),
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            diag_v: vec![],
            dt_v: vec![],
            gas_v: HashMap::new(),
            calc_gas_v: HashMap::new(),
            calc_dt_v: HashMap::new(),
            measurement_gas_v: HashMap::new(),
            measurement_dt_v: vec![],
            measurement_diag_v: Vec::new(),
            gases: vec![],
            air_pressure: 1000.,
            air_temperature: 10.,
            chamber_volume: 1.,
            is_valid: true,
            override_valid: None,
            manual_valid: false,
        })
    }
}
#[derive(Clone)]
pub struct Cycle {
    pub chamber_id: String,
    pub instrument_model: InstrumentType,
    pub instrument_serial: String,
    pub project_name: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub close_time: chrono::DateTime<chrono::Utc>,
    pub open_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub air_temperature: f64,
    pub air_pressure: f64,
    pub chamber_volume: f64,
    // pub has_errors: bool,
    pub error_code: ErrorMask,
    pub is_valid: bool,
    pub override_valid: Option<bool>,
    pub manual_valid: bool,
    pub main_gas: GasType,
    pub close_offset: i64,
    pub open_offset: i64,
    pub end_offset: i64,
    pub lag_s: f64,
    pub max_idx: f64,
    pub gases: Vec<GasType>,
    pub calc_range_start: HashMap<GasType, f64>,
    pub calc_range_end: HashMap<GasType, f64>,
    pub manual_adjusted: bool,
    pub min_y: HashMap<GasType, f64>,
    pub max_y: HashMap<GasType, f64>,
    // pub gas_plot: HashMap<GasType, Vec<[f64; 2]>>,
    pub flux: HashMap<GasType, f64>,
    pub slope: HashMap<GasType, f64>,
    pub measurement_range_start: f64,
    pub measurement_range_end: f64,
    pub measurement_r2: HashMap<GasType, f64>,
    pub calc_r2: HashMap<GasType, f64>,

    // datetime vectors
    pub dt_v: Vec<chrono::DateTime<chrono::Utc>>,
    pub calc_dt_v: HashMap<GasType, Vec<chrono::DateTime<chrono::Utc>>>,
    pub measurement_dt_v: Vec<chrono::DateTime<chrono::Utc>>,

    // gas vectors
    pub gas_v: HashMap<GasType, Vec<f64>>,
    pub calc_gas_v: HashMap<GasType, Vec<f64>>,
    pub measurement_gas_v: HashMap<GasType, Vec<f64>>,
    pub measurement_diag_v: Vec<i64>,

    pub diag_v: Vec<i64>,
}
impl fmt::Debug for Cycle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let len: usize = self.measurement_dt_v.len();
        write!(
            f,
            // "Cycle id: {}, \nlag: {}, \nstart: {}, \nmeas_s: {}, \nmeas_e: {}",
            "Cycle id: {}, \nproj: {}",
            self.chamber_id,
            self.project_name // self.lag_s,
                              // self.start_time,
                              // self.dt_v.len(),
                              // self.calc_dt_v.get(&GasType::CH4).unwrap_or(Vec::new()).len(),
                              // len,
                              // self.measurement_dt_v.get(&GasType::CH4).unwrap().len()
        )
    }
}
#[allow(clippy::needless_lifetimes)]
// #[allow(needless_lifetimes)]
impl Cycle {
    // pub fn _to_html_row(&self) -> Result<String, Box<dyn Error>> {
    //     let _plot_path = gas_plot::draw_gas_plot(self)?; // Call your plot function and get the path
    //     Ok(format!(
    //         "<tr>\
    //             <td>{}</td>\
    //             <td>{}</td>\
    //             <td>{}</td>\
    //             <td>{:.4}</td>\
    //             <td>{:.4}</td>\
    //         </tr>",
    //         self.chamber_id,
    //         self.start_time.to_rfc3339(),
    //         self.lag_s,
    //         self.r,
    //         self.flux
    //     ))
    // }
    pub fn toggle_valid(&mut self) {
        self.is_valid = !self.is_valid; // Toggle `is_valid`
    }
    pub fn dt_v_as_float(&self) -> Vec<f64> {
        self.dt_v.iter().map(|x| x.timestamp() as f64).collect()
    }
    // pub fn validate(&mut self) {
    //     if self.override_valid.is_none() {
    //         self.is_valid = self.automatic_validation();
    //     } else {
    //         self.is_valid = self.override_valid.unwrap();
    //     }
    // }
    // pub fn set_automatic_valid(&mut self, valid: bool) {
    //     if self.override_valid.is_none() {
    //         self.is_valid = valid;
    //     }
    // }
    pub fn set_automatic_valid(&mut self, valid: bool) {
        if self.override_valid.is_none() {
            self.is_valid = valid && self.error_code.0 == 0; // Ensure error codes affect validity
        }
    }
    pub fn toggle_manual_valid(&mut self) {
        if self.override_valid.is_some() {
            // if we hit override after already toggling it, it will reset to None
            self.override_valid = None;
        } else {
            // if we have overriden, it should be opposite of valid
            self.override_valid = Some(!self.is_valid);
            // if self.is_valid {
            //     self.error_code = ErrorMask(0);
            // }
        }

        // always toggle is_valid
        self.is_valid = !self.is_valid;
        // manual valid to true if override is not None
        self.manual_valid = self.override_valid.is_some(); // Track manual changes
        if self.manual_valid && self.override_valid == Some(false) {
            self.add_error(ErrorCode::ManualInvalid)
        } else {
            self.remove_error(ErrorCode::ManualInvalid)
        }
        if self.manual_valid && self.override_valid == Some(true) {
            self.error_code = ErrorMask(0);
            // self.add_error(ErrorCode::ManualInvalid)
        }
    }

    pub fn get_peak_near_timestamp(
        &mut self,
        gas_type: GasType,
        target_time: i64, // Now an i64 timestamp
    ) -> Option<DateTime<Utc>> {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let len = gas_v.len();
            if len < 120 {
                println!("Less than 2minutes of data.");
                return None;
            }

            // Find index closest to `target_time` in `dt_v`
            let target_idx = self
                .dt_v
                .iter()
                .enumerate()
                .min_by_key(|(_, &dt)| (dt.timestamp() - target_time).abs())
                .map(|(idx, _)| idx)?;

            // Define search range (±5 seconds)
            let start_index = target_idx.saturating_sub(5);
            let end_index = (target_idx + 5).min(len - 1);

            // Find max in the range
            let max_idx = (start_index..=end_index).max_by(|&a, &b| {
                gas_v[a].partial_cmp(&gas_v[b]).unwrap_or(std::cmp::Ordering::Equal)
            });
            if let Some(idx) = max_idx {
                if let Some(peak_time) = self.dt_v.get(idx).cloned() {
                    self.lag_s = (peak_time
                        - (self.start_time + chrono::TimeDelta::seconds(self.open_offset)))
                    .num_seconds() as f64;

                    return Some(peak_time);
                }
            }
        }
        None
    }
    pub fn get_peak_datetime(&mut self, gas_type: GasType) -> Option<DateTime<Utc>> {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let len = gas_v.len();
            if len < 120 {
                return None;
            }
            // println!("{}", gas_v.len());

            let start_index = len.saturating_sub(240);
            let max_idx = gas_v[start_index..]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| start_index + idx);

            if let Some(idx) = max_idx {
                if let Some(peak_time) = self.dt_v.get(idx).cloned() {
                    self.lag_s = (peak_time
                        - (self.start_time + chrono::TimeDelta::seconds(self.open_offset)))
                    .num_seconds() as f64;

                    return Some(peak_time);
                }
            }
        }
        self.get_measurement_datas();
        None
    }
    pub fn check_diag(&mut self) {
        let total_count = self.diag_v.len();
        let nonzero_count = self.diag_v.iter().filter(|&&x| x != 0).count();

        // Check if more than 50% of the values are nonzero
        let check = nonzero_count as f64 / total_count as f64 > 0.5;
        if check {
            self.add_error(ErrorCode::TooManyDiagErrors)
        } else {
            self.remove_error(ErrorCode::TooManyDiagErrors)
        }
    }
    // pub fn check_diag(&mut self) -> bool {
    //     self.diag_v.iter().sum::<i64>() != 0
    // }
    pub fn check_measurement_diag(&mut self) -> bool {
        let check = self.measurement_diag_v.iter().sum::<i64>() != 0;
        if check {
            self.add_error(ErrorCode::ErrorsInMeasurement)
        } else {
            self.remove_error(ErrorCode::ErrorsInMeasurement)
        }
        check
    }

    pub fn adjust_open_time(&mut self) {
        self.open_time = self.start_time
            + chrono::TimeDelta::seconds(self.open_offset)
            + chrono::TimeDelta::seconds(self.lag_s as i64)
    }
    pub fn calculate_max_y(&mut self) {
        for (gas_type, gas_v) in &self.gas_v {
            let min_value =
                gas_v.iter().copied().filter(|v| !v.is_nan()).fold(f64::NEG_INFINITY, f64::max);

            self.max_y.insert(*gas_type, min_value);
        }
    }
    pub fn calculate_min_y(&mut self) {
        for (gas_type, gas_v) in &self.gas_v {
            let min_value =
                gas_v.iter().copied().filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min);

            self.min_y.insert(*gas_type, min_value);
        }
    }
    pub fn calculate_measurement_r(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
            let dt_vv: Vec<f64> =
                self.measurement_dt_v.iter().map(|x| x.timestamp() as f64).collect();
            // self.measurement_r = stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0);
            self.measurement_r2
                .insert(gas_type, stats::pearson_correlation(&dt_vv, gas_v).unwrap_or(0.0).powi(2));
        }
    }
    // pub fn prepare_plot_data(&mut self) {
    //     // let cycle = &self.cycles[self.index.count];
    //     self.gas_plot.clear(); // Clear existing data before recalculating
    //
    //     for (gas_type, gas_v) in &self.gas_v {
    //         let data: Vec<[f64; 2]> = self
    //             .dt_v_as_float()
    //             .iter()
    //             .copied()
    //             .zip(gas_v.iter().copied())
    //             .map(|(x, y)| [x, y])
    //             .collect();
    //
    //         self.gas_plot.insert(*gas_type, data);
    //     }
    // }
    pub fn calculate_calc_r(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let s = self.calc_range_start.get(&gas_type).unwrap();
            let e = self.calc_range_end.get(&gas_type).unwrap();
            let mut dt_v: Vec<_> = Vec::new();
            let mut filtered_gas_v: Vec<_> = Vec::new();

            self.dt_v
                .iter()
                .zip(gas_v.iter()) // Pair timestamps with gas values
                .filter(|(t, _)| (t.timestamp() as f64) >= *s && (t.timestamp() as f64) <= *e) // Filter by time range
                .for_each(|(t, d)| {
                    dt_v.push(*t);
                    filtered_gas_v.push(*d);
                });

            let dt_vv: Vec<f64> = dt_v.iter().map(|x| x.timestamp() as f64).collect();
            // self.calc_r = stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0);
            self.calc_r2.insert(
                gas_type,
                stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0).powi(2),
            );
        }
    }

    pub fn find_highest_r_window(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
            if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
                println!("Short data");
                return;
            }

            let max_window = gas_v.len();
            let mut max_r = f64::MIN;
            let mut start_idx = 0;
            let mut end_idx = 0;
            let dt_v: Vec<f64> =
                self.measurement_dt_v.iter().map(|dt| dt.timestamp() as f64).collect();
            let mut best_window_y = Vec::new();

            for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
                for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
                    let end = start + win_size;
                    let x_win = &dt_v[start..end];
                    let y_win = &gas_v[start..end];
                    let r = stats::pearson_correlation(x_win, y_win).unwrap_or(0.0);
                    if r > max_r {
                        max_r = r;
                        start_idx = start;
                        end_idx = end;
                        best_window_y = y_win.to_vec();
                    }
                }
            }

            self.calc_r2.insert(gas_type, max_r);
            self.calc_range_start
                .insert(gas_type, self.measurement_dt_v[start_idx].timestamp() as f64);
            self.calc_range_end
                .insert(gas_type, self.measurement_dt_v[end_idx - 1].timestamp() as f64);
            self.calc_dt_v.insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
            self.calc_gas_v.insert(gas_type, best_window_y);
        }
    }
    pub fn get_calc_datas(&mut self) {
        for &gas_type in &self.gases {
            if let (Some(gas_v), Some(s), Some(e)) = (
                self.gas_v.get(&gas_type),
                self.calc_range_start.get(&gas_type),
                self.calc_range_end.get(&gas_type),
            ) {
                // Clear previous results
                self.calc_gas_v.insert(gas_type, Vec::new());
                self.calc_dt_v.insert(gas_type, Vec::new());

                // Filter and store results in separate vectors
                self.dt_v
                    .iter()
                    .zip(gas_v.iter())
                    .filter(|(t, _)| (t.timestamp() as f64) >= *s && (t.timestamp() as f64) <= *e)
                    .for_each(|(t, d)| {
                        self.calc_dt_v.get_mut(&gas_type).unwrap().push(*t);
                        self.calc_gas_v.get_mut(&gas_type).unwrap().push(*d);
                    });
            }
        }
    }

    pub fn get_measurement_datas(&mut self) {
        for &gas_type in &self.gases {
            // self.get_measurement_data(gas_type);
            if let Some(gas_v) = self.gas_v.get(&gas_type) {
                let close_time = self.start_time
                    + chrono::TimeDelta::seconds(self.close_offset + self.lag_s as i64);
                let open_time = self.start_time
                    + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
                let s = close_time.timestamp() as f64;
                let e = open_time.timestamp() as f64;

                // Clear previous results
                self.measurement_gas_v.insert(gas_type, Vec::new());
                self.measurement_dt_v.clear();
                self.measurement_diag_v.clear();
                let diag_v = &self.diag_v; // Directly reference diag_v (no Option handling needed)

                // Filter and store results
                for ((t, d), diag) in self.dt_v.iter().zip(gas_v.iter()).zip(diag_v.iter()) {
                    if (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e {
                        self.measurement_dt_v.push(*t);
                        self.measurement_gas_v.get_mut(&gas_type).unwrap().push(*d);
                        self.measurement_diag_v.push(*diag);
                    }
                }
                // Filter and store results in separate vectors
                // self.dt_v
                //     .iter()
                //     .zip(gas_v.iter()) // Pair timestamps with gas values
                //     .filter(|(t, _)| (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e) // Filter by time range
                //     .for_each(|(t, d)| {
                //         self.measurement_dt_v.push(*t);
                //         self.measurement_gas_v.get_mut(&gas_type).unwrap().push(*d);
                //     });
            } else {
                println!("no measurement data for {}", gas_type);
            }
        }
    }

    pub fn calculate_measurement_rs(&mut self) {
        for &gas_type in &self.gases {
            if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
                let dt_vv: Vec<f64> =
                    self.measurement_dt_v.iter().map(|x| x.timestamp() as f64).collect();

                self.measurement_r2.insert(
                    gas_type,
                    stats::pearson_correlation(&dt_vv, gas_v).unwrap_or(0.0).abs().powi(2),
                );
            }
        }
    }

    pub fn find_highest_r_windows(&mut self) {
        for &gas_type in &self.gases {
            if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
                if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
                    println!("Short data for {:?}", gas_type);
                    continue;
                }

                let max_window = gas_v.len();
                let mut max_r = f64::MIN;
                let mut start_idx = 0;
                let mut end_idx = 0;
                let dt_v: Vec<f64> =
                    self.measurement_dt_v.iter().map(|dt| dt.timestamp() as f64).collect();
                let mut best_window_y = Vec::new();

                for win_size in (MIN_WINDOW_SIZE..=max_window).step_by(WINDOW_INCREMENT) {
                    for start in (0..max_window).step_by(WINDOW_INCREMENT) {
                        let end = (start + win_size).min(max_window); // Ensure `end` does not exceed `max_window`

                        // Skip if window is too small after clamping
                        if end - start < MIN_WINDOW_SIZE {
                            continue;
                        }

                        // Extract the window
                        let x_win = &dt_v[start..end];
                        let y_win = &gas_v[start..end];

                        // 🔹 Check for missing timestamps
                        let has_missing_time = x_win
                            .windows(2) // Pairwise check for consecutive elements
                            .any(|pair| (pair[1] - pair[0]).abs() > 1.0); // Difference > 1 second means gap

                        // 🔹 Skip calculation if there are missing timestamps
                        if has_missing_time {
                            continue;
                        }

                        // Compute Pearson correlation only for valid continuous data
                        let r = stats::pearson_correlation(x_win, y_win).unwrap_or(0.0).powi(2);

                        if r > max_r {
                            max_r = r;
                            start_idx = start;
                            end_idx = end;
                            best_window_y = y_win.to_vec();
                        }
                    }
                }

                // 🔹 Ensure `end_idx` is never `0` before using it
                if end_idx == 0 {
                    println!("No valid window found for {:?}", gas_type);
                    continue; // Skip storing results if no valid window was found
                }

                // 🔹 Store results safely
                self.calc_r2.insert(gas_type, max_r);
                self.calc_range_start.insert(
                    gas_type,
                    self.measurement_dt_v.get(start_idx).map_or(0.0, |dt| dt.timestamp() as f64),
                );
                self.calc_range_end.insert(
                    gas_type,
                    self.measurement_dt_v
                        .get(end_idx.saturating_sub(1))
                        .map_or(0.0, |dt| dt.timestamp() as f64),
                );
                self.calc_dt_v.insert(
                    gas_type,
                    self.measurement_dt_v
                        .get(start_idx..end_idx)
                        .map_or_else(Vec::new, |v| v.to_vec()),
                );
                self.calc_gas_v.insert(gas_type, best_window_y);
            }
        }
    }
    pub fn has_error(&self, error: ErrorCode) -> bool {
        self.error_code.0 & error.to_mask() != 0
    }
    pub fn add_error(&mut self, error: ErrorCode) {
        self.error_code |= error;
        if self.error_code != ErrorMask(0) {
            self.is_valid = false; // Automatically invalidate on error
        }
    }
    pub fn remove_error(&mut self, error: ErrorCode) {
        self.error_code.0 &= !error.to_mask();
        if self.error_code.0 == 0 {
            self.is_valid = true; // If no errors remain, revalidate
        }
    }
    // pub fn remove_error(&mut self, error: ErrorCode) {
    //     self.error_code &= !error;
    //     if self.error_code == ErrorMask(0) {
    //         self.is_valid = true; // If no errors remain, revalidate
    //     }
    // }
    pub fn check_main_r(&mut self) {
        if self.measurement_r2.get(&self.main_gas).unwrap_or(&0.0) < &0.98 {
            self.add_error(ErrorCode::LowR);
        } else {
            self.remove_error(ErrorCode::LowR)
        }
    }

    pub fn check_missing(&mut self) {
        let check = self.end_offset as f64 * 0.7 > self.dt_v.len() as f64;
        if check {
            self.add_error(ErrorCode::TooFewMeasurements);
        } else {
            self.remove_error(ErrorCode::TooFewMeasurements)
        }
    }
    pub fn check_errors(&mut self) {
        self.check_main_r();
        self.check_measurement_diag();
        self.check_missing();
        if self.error_code.0 == 0 || self.override_valid == Some(true) {
            self.is_valid = true
        }
    }
    pub fn reset(&mut self) {
        // self.check_errors();
        self.manual_adjusted = false;
        self.check_diag();
        self.check_missing();

        if !self.has_error(ErrorCode::TooManyDiagErrors)
            && !self.has_error(ErrorCode::TooFewMeasurements)
        {
            // let mut conn = match Connection::open("fluxrs.db") {
            //     Ok(conn) => conn,
            //     Err(e) => {
            //         eprintln!("Failed to open database: {}", e);
            //         return; // Exit early if connection fails
            //     },
            // };
            // let close_time = (self.start_time
            //     + chrono::TimeDelta::seconds(self.close_offset + self.lag_s as i64))
            // .timestamp();
            // let (temp, pressure) =
            //     get_nearest_meteo_data(&conn, self.project_name.clone(), close_time).unwrap();
            // self.air_temperature = temp;
            // self.air_pressure = pressure;
            self.get_peak_datetime(self.main_gas);
            self.get_measurement_datas();
            self.calculate_measurement_rs();
            // if self.check_measurement_diag() {
            //     self.lag_s = 0.;
            //     self.is_valid = false;
            //     return;
            // }
            self.check_main_r();
            self.find_highest_r_windows();
            self.calculate_fluxes();
            self.calculate_max_y();
            self.calculate_min_y();
            self.check_errors();
        }
    }
    pub fn change_measurement_range(&mut self) {
        self.get_measurement_datas();
        self.calculate_measurement_rs();
        self.find_highest_r_windows();
        self.calculate_fluxes();
    }
    pub fn recalc_r(&mut self) {
        self.find_highest_r_windows();
        self.calculate_fluxes();
    }

    pub fn get_calc_data(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let s = self.calc_range_start.get(&gas_type).unwrap();
            let e = self.calc_range_end.get(&gas_type).unwrap();

            // Clear previous results
            self.calc_gas_v.insert(gas_type, Vec::new());
            self.calc_dt_v.insert(gas_type, Vec::new());

            // Filter and store results in separate vectors
            self.dt_v
                .iter()
                .zip(gas_v.iter())
                .filter(|(t, _)| (t.timestamp() as f64) >= *s && (t.timestamp() as f64) <= *e)
                .for_each(|(t, d)| {
                    self.calc_dt_v.get_mut(&gas_type).unwrap().push(*t);
                    self.calc_gas_v.get_mut(&gas_type).unwrap().push(*d);
                });
        }
    }
    pub fn get_measurement_data(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let close_time =
                self.start_time + chrono::TimeDelta::seconds(self.close_offset + self.lag_s as i64);
            let open_time =
                self.start_time + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
            let s = close_time.timestamp() as f64;
            let e = open_time.timestamp() as f64;

            // Clear previous results
            self.measurement_gas_v.insert(gas_type, Vec::new());
            self.measurement_dt_v.clear();

            // Filter and store results in separate vectors
            self.dt_v
                .iter()
                .zip(gas_v.iter()) // Pair timestamps with gas values
                .filter(|(t, _)| (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e) // Filter by time range
                .for_each(|(t, d)| {
                    self.measurement_dt_v.push(*t);
                    self.measurement_gas_v.get_mut(&gas_type).unwrap().push(*d);
                });
        } else {
            println!("No gas data found for {}", gas_type);
        }
    }
    pub fn calculate_slope(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.calc_gas_v.get(&gas_type) {
            let num_ts: Vec<f64> = self
                .calc_dt_v
                .get(&gas_type)
                .unwrap()
                .iter()
                .map(|dt| dt.timestamp() as f64)
                .collect();
            let slope = stats::LinReg::train(&num_ts, gas_v).slope;
            self.slope.insert(gas_type, slope);
        } else {
            self.slope.insert(gas_type, 0.0);
        }
    }

    pub fn calculate_fluxes(&mut self) {
        for &gas in &self.gases.clone() {
            self.calculate_flux(gas);
        }
    }
    pub fn calculate_flux(&mut self, gas_type: GasType) {
        self.calculate_slope(gas_type);
        let mol_mass = match gas_type {
            GasType::CO2 => 44.0,
            GasType::CH4 => 16.0,
            GasType::H2O => 18.0,
            GasType::N2O => 44.0,
        };
        self.flux.insert(
            gas_type,
            self.slope.get(&gas_type).unwrap() / 1_000_000.0
                * self.chamber_volume
                * ((mol_mass * (self.air_pressure * 1000.0))
                    / (8.314 * (self.air_temperature + 273.15)))
                * 1000.0,
        );
    }
    pub fn update_cycle(&mut self, project: String) {
        // let mut conn = match Connection::open("fluxrs.db") {
        //     Ok(conn) => conn,
        //     Err(e) => {
        //         eprintln!("Failed to open database: {}", e);
        //         return; // Exit early if connection fails
        //     },
        // };
        // let close_time = (self.start_time
        //     + chrono::TimeDelta::seconds(self.close_offset + self.lag_s as i64))
        // .timestamp();
        // let (temp, pressure) = get_nearest_meteo_data(&conn, project, close_time).unwrap();
        //
        // self.air_temperature = temp;
        // self.air_pressure = pressure;
        self.get_calc_datas();
        self.get_measurement_datas();
        self.calculate_measurement_rs();
        self.find_highest_r_windows();
        self.calculate_fluxes();
    }
    // pub fn get_nearest_meteo_data(&self, conn: &Connection) -> Result<(f64, f64)> {
    //     let mut stmt = conn.prepare(
    //         "SELECT temperature, pressure
    //          FROM meteo
    //          WHERE project_id = ?1
    //          ORDER BY ABS(datetime - ?2)
    //          LIMIT 1",
    //     )?;
    //
    //     let result = stmt.query_row(params![&self.project_id, self.close_time], |row| {
    //         Ok((row.get(0)?, row.get(1)?))
    //     });
    //
    //     match result {
    //         Ok((temperature, pressure)) => Ok((temperature, pressure)),
    //         Err(_) => Ok((0.0, 0.0)), // Return defaults if no data is found
    //     }
    // }
}

pub struct GasData {
    pub header: StringRecord,
    pub instrument_model: String,
    pub instrument_serial: String,
    pub datetime: Vec<DateTime<Utc>>,
    // pub gas: HashMapnew(),
    pub gas: HashMap<GasType, Vec<f64>>,
    pub diag: Vec<i64>,
}
impl fmt::Debug for GasData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "start: {}, \nstart_len: {} diag_len: {}",
            self.datetime[0],
            self.datetime.len(),
            self.diag.len(),
        )?;

        for (gas_type, values) in &self.gas {
            writeln!(f, "{:?}: {:?}", gas_type, values.len())?;
        }

        Ok(())
    }
}
impl EqualLen for GasData {
    fn validate_lengths(&self) -> bool {
        // check that all fields are equal length
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

impl Default for GasData {
    fn default() -> Self {
        Self::new()
    }
}
impl GasData {
    pub fn new() -> GasData {
        GasData {
            header: csv::StringRecord::new(),
            instrument_model: String::new(),
            instrument_serial: String::new(),
            datetime: Vec::new(),
            gas: HashMap::new(),
            diag: Vec::new(),
        }
    }
    pub fn any_col_invalid(&self) -> bool {
        // Check if all values in any vector are equal to the error value
        let gas_invalid = self.gas.values().any(|v| v.iter().all(|&x| x == ERROR_FLOAT));
        let diag_invalid = self.diag.iter().all(|&x| x == ERROR_INT);

        gas_invalid || diag_invalid
    }

    // pub fn any_col_invalid(&self) -> bool {
    //     // create a list of booleans by checking all values in the vector, if all are equal to
    //     // error value, return true to the vector
    //     let invalids: [&bool; 2] = [
    //         &self.gas.iter().all(|&x| x == ERROR_FLOAT),
    //         &self.diag.iter().all(|&x| x == ERROR_INT),
    //     ];
    //     let check = invalids.iter().any(|&x| *x);
    //     check
    // }

    pub fn summary(&self) {
        println!("dt: {} len: {}", self.datetime[0], self.diag.len());
    }
    pub fn sort(&mut self) {
        let mut indices: Vec<usize> = (0..self.datetime.len()).collect();
        indices.sort_by(|&i, &j| self.datetime[i].cmp(&self.datetime[j]));

        self.datetime = indices.iter().map(|&i| self.datetime[i]).collect();
        self.diag = indices.iter().map(|&i| self.diag[i]).collect();

        // Sort each gas type in the HashMap safely
        for values in self.gas.values_mut() {
            if values.len() == self.datetime.len() {
                // Ensure lengths match before sorting
                *values = indices.iter().filter_map(|&i| values.get(i).copied()).collect();
            } else {
                eprintln!("Warning: Mismatched lengths during sorting, skipping gas type sorting.");
            }
        }
    }

    // pub fn sort(&mut self) {
    //     let mut indices: Vec<usize> = (0..self.datetime.len()).collect();
    //     indices.sort_by(|&i, &j| self.datetime[i].cmp(&self.datetime[j]));
    //
    //     self.datetime = indices.iter().map(|&i| self.datetime[i]).collect();
    //     self.diag = indices.iter().map(|&i| self.diag[i]).collect();
    //
    //     // Sort each gas type in the HashMap
    //     for values in self.gas.values_mut() {
    //         *values = indices.iter().map(|&i| values[i]).collect();
    //     }
    // }

    // pub fn sort(&mut self) {
    //     let mut indices: Vec<usize> = (0..self.datetime.len()).collect();
    //     indices.sort_by(|&i, &j| self.datetime[i].cmp(&self.datetime[j]));
    //
    //     self.datetime = indices.iter().map(|&i| self.datetime[i]).collect();
    //     self.gas = indices.iter().map(|&i| self.gas[i]).collect();
    //     self.diag = indices.iter().map(|&i| self.diag[i]).collect();
    // }
}

#[derive(Debug)]
pub struct TimeData {
    pub chamber_id: Vec<String>,
    pub start_time: Vec<DateTime<Utc>>,
    pub close_offset: Vec<i64>,
    pub open_offset: Vec<i64>,
    pub end_offset: Vec<i64>,
    pub project: Vec<String>,
}

impl EqualLen for TimeData {
    fn validate_lengths(&self) -> bool {
        let lengths = [
            &self.chamber_id.len(),
            &self.start_time.len(),
            &self.close_offset.len(),
            &self.open_offset.len(),
            &self.end_offset.len(),
        ];
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

impl TimeData {
    pub fn new() -> TimeData {
        TimeData {
            chamber_id: Vec::new(),
            start_time: Vec::new(),
            close_offset: Vec::new(),
            open_offset: Vec::new(),
            end_offset: Vec::new(),
            project: Vec::new(),
        }
    }
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&String, &DateTime<Utc>, &i64, &i64, &i64, &String)> {
        self.chamber_id
            .iter()
            .zip(&self.start_time)
            .zip(&self.close_offset)
            .zip(&self.open_offset)
            .zip(&self.end_offset)
            .zip(&self.project)
            .map(|(((((chamber, start), close), open), end), project)| {
                (chamber, start, close, open, end, project)
            })
    }
}
// fn nan_exceeds_threshold(measurement_length: f64, threshold: f64) -> bool {
//     let total_count = values.len();
//     let nan_count = values.iter().filter(|&&x| x.is_nan()).count();
//
//     // Check if NaN count exceeds the threshold percentage
//     (nan_count as f64) / (total_count as f64) > threshold
// }
#[derive(Debug, Default)]
pub struct VolumeData {
    pub datetime: Vec<i64>,
    pub chamber_id: Vec<String>,
    pub volume: Vec<f64>,
}

impl VolumeData {
    // pub fn new() -> VolumeData {
    //     VolumeData { datetime: Vec::new(), chamber_id: Vec::new(), volume: Vec::new() }
    // }
}
#[derive(Debug, Default)]
pub struct MeteoData {
    pub datetime: Vec<i64>,
    pub temperature: Vec<f64>,
    pub pressure: Vec<f64>,
}
impl MeteoData {
    pub fn get_nearest(&self, target_timestamp: i64) -> Option<(f64, f64)> {
        if self.datetime.is_empty() {
            return None; // No data available
        }

        let mut left = 0;
        let mut right = self.datetime.len() - 1;

        while left < right {
            let mid = (left + right) / 2;
            match self.datetime[mid].cmp(&target_timestamp) {
                Ordering::Less => left = mid + 1,
                Ordering::Greater => {
                    if mid > 0 {
                        right = mid - 1;
                    } else {
                        break;
                    }
                },
                Ordering::Equal => return Some((self.temperature[mid], self.pressure[mid])),
            }
        }

        match left {
            0 => {
                let diff = (self.datetime[0] - target_timestamp).abs();
                if diff <= 1800 {
                    Some((self.temperature[0], self.pressure[0]))
                } else {
                    None
                }
            },
            _ if left >= self.datetime.len() => {
                let diff = (self.datetime[right] - target_timestamp).abs();
                if diff <= 1800 {
                    Some((self.temperature[right], self.pressure[right]))
                } else {
                    None
                }
            },
            _ => {
                let prev_idx = left - 1;
                let next_idx = left;

                let prev_diff = (self.datetime[prev_idx] - target_timestamp).abs();
                let next_diff = (self.datetime[next_idx] - target_timestamp).abs();

                let (nearest_idx, nearest_diff) = if prev_diff <= next_diff {
                    (prev_idx, prev_diff)
                } else {
                    (next_idx, next_diff)
                };

                if nearest_diff <= 1800 {
                    Some((self.temperature[nearest_idx], self.pressure[nearest_idx]))
                } else {
                    None // No valid data within 30 min
                }
            },
        }
    }
    // pub fn get_nearest(&self, target_timestamp: i64) -> Option<(f64, f64)> {
    //     if self.datetime.is_empty() || self.temperature.is_empty() || self.pressure.is_empty() {
    //         return None; // Return None if no data is available
    //     }
    //
    //     let mut left = 0;
    //     let mut right = self.datetime.len() - 1;
    //
    //     while left < right {
    //         let mid = (left + right) / 2;
    //         match self.datetime[mid].cmp(&target_timestamp) {
    //             Ordering::Less => left = mid + 1,
    //             Ordering::Greater => {
    //                 if mid > 0 {
    //                     right = mid - 1;
    //                 } else {
    //                     break;
    //                 }
    //             },
    //             Ordering::Equal => return Some((self.temperature[mid], self.pressure[mid])),
    //         }
    //     }
    //
    //     // Edge case: If left is at the start, return first entry
    //     if left == 0 {
    //         return Some((self.temperature[0], self.pressure[0]));
    //     }
    //
    //     // Edge case: If left is at the end, return last entry
    //     if left >= self.datetime.len() {
    //         return Some((
    //             self.temperature[self.datetime.len() - 1],
    //             self.pressure[self.datetime.len() - 1],
    //         ));
    //     }
    //
    //     // Find the closest timestamp
    //     let prev_idx = left - 1;
    //     let next_idx = left;
    //
    //     let prev_diff = (self.datetime[prev_idx] - target_timestamp).abs();
    //     let next_diff = (self.datetime[next_idx] - target_timestamp).abs();
    //
    //     let nearest_idx = if prev_diff <= next_diff { prev_idx } else { next_idx };
    //
    //     if nearest_diff <= 1800 {
    //         Some((self.temperature[nearest_idx], self.pressure[nearest_idx]))
    //     } else {
    //         None // No valid data within 30 min
    //     }
    //     // Some((self.temperature[nearest_idx], self.pressure[nearest_idx]))
    // }
}
