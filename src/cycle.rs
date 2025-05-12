use crate::constants::MIN_CALC_AREA_RANGE;
use crate::errorcode::{ErrorCode, ErrorMask};
use crate::flux::{FluxModel, LinearFlux};
use crate::fluxes_schema::{
    fluxes_col, make_insert_flux_history, make_insert_flux_results, make_insert_or_ignore_fluxes,
    make_select_fluxes, make_update_fluxes,
};
use crate::gasdata::{query_gas, query_gas2, query_gas_all};
use crate::instruments::GasType;
use crate::instruments::{get_instrument_by_model, InstrumentType};
use crate::processevent::{InsertEvent, ProcessEvent, ProgressEvent, QueryEvent, ReadEvent};
use crate::stats::{self, LinReg};
use chrono::{DateTime, TimeDelta, Utc};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rusqlite::{params, Connection, Error, Result};
use std::collections::HashMap;
use std::fmt;
use tokio::sync::mpsc;

// the window of max r must be at least 240 seconds
pub const MIN_WINDOW_SIZE: usize = 180;
// how many seconds to increment the moving window searching for max r
pub const WINDOW_INCREMENT: usize = 5;

#[derive(Clone)]
pub struct Cycle {
    pub id: i64,
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
    pub min_calc_range: f64,
    // pub has_errors: bool,
    pub error_code: ErrorMask,
    pub is_valid: bool,
    pub override_valid: Option<bool>,
    pub manual_valid: bool,
    pub main_gas: GasType,
    pub close_offset: i64,
    pub open_offset: i64,
    pub end_offset: i64,
    pub open_lag_s: f64,
    pub close_lag_s: f64,
    pub end_lag_s: f64,
    pub start_lag_s: f64,
    pub max_idx: f64,
    pub gases: Vec<GasType>,
    pub calc_range_start: HashMap<GasType, f64>,
    pub calc_range_end: HashMap<GasType, f64>,
    pub manual_adjusted: bool,
    pub min_y: HashMap<GasType, f64>,
    pub max_y: HashMap<GasType, f64>,
    // pub gas_plot: HashMap<GasType, Vec<[f64; 2]>>,
    pub flux: HashMap<GasType, f64>,
    pub linfit: HashMap<GasType, LinReg>,
    pub measurement_range_start: f64,
    pub measurement_range_end: f64,
    pub fluxes: Vec<Box<dyn FluxModel>>,
    pub measurement_r2: HashMap<GasType, f64>,
    pub calc_r2: HashMap<GasType, f64>,

    // datetime vectors
    pub dt_v: Vec<chrono::DateTime<chrono::Utc>>,
    pub calc_dt_v: HashMap<GasType, Vec<chrono::DateTime<chrono::Utc>>>,
    pub measurement_dt_v: Vec<chrono::DateTime<chrono::Utc>>,

    // gas vectors
    pub gas_v: HashMap<GasType, Vec<Option<f64>>>,
    pub gas_v_mole: HashMap<GasType, Vec<Option<f64>>>,
    pub calc_gas_v: HashMap<GasType, Vec<Option<f64>>>,
    pub measurement_gas_v: HashMap<GasType, Vec<Option<f64>>>,
    pub measurement_diag_v: Vec<i64>,
    pub t0_concentration: HashMap<GasType, f64>,

    pub diag_v: Vec<i64>,
}

impl fmt::Debug for Cycle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // let len: usize = self.measurement_dt_v.len();
        write!(
            f,
            // "Cycle id: {}, \nlag: {}, \nstart: {}, \nmeas_s: {}, \nmeas_e: {}",
            "start: {}",
            self.start_time,
            // self.dt_v.len(),
            // self.calc_dt_v.get(&GasType::CH4).unwrap_or(Vec::new()).len(),
            // len,
            // self.measurement_dt_v.get(&GasType::CH4).unwrap().len()
        )
    }
}
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

    pub fn get_is_valid(&self) -> bool {
        self.is_valid
    }
    pub fn get_start(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.start_lag_s
    }
    pub fn get_end(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.end_lag_s + self.end_offset as f64
    }

    pub fn get_start_no_lag(&self) -> f64 {
        self.start_time.timestamp() as f64
    }
    pub fn get_end_no_lag(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.end_offset as f64
    }

    pub fn get_close(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.close_offset as f64
    }
    pub fn get_open(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.open_offset as f64
    }

    pub fn get_adjusted_close(&self) -> f64 {
        self.get_start() + self.close_offset as f64 + self.open_lag_s + self.close_lag_s
    }
    pub fn get_adjusted_open(&self) -> f64 {
        self.get_start() + self.open_offset as f64 + self.open_lag_s
    }
    pub fn get_slope(&self, gas_type: &GasType) -> f64 {
        self.linfit.get(gas_type).unwrap().slope
    }
    pub fn get_intercept(&self, gas_type: &GasType) -> f64 {
        self.linfit.get(gas_type).unwrap().intercept
    }

    pub fn toggle_valid(&mut self) {
        self.is_valid = !self.is_valid; // Toggle `is_valid`
    }
    pub fn dt_v_as_float(&self) -> Vec<f64> {
        self.dt_v.iter().map(|x| x.timestamp() as f64).collect()
    }
    pub fn set_calc_start(&mut self, gas_type: GasType, value: f64) {
        let range_min = self.get_adjusted_close();
        // the calc area cant go beyond the measurement area
        if range_min > value {
            self.calc_range_start.insert(gas_type, range_min);
        } else {
            self.calc_range_start.insert(gas_type, value);
        }
    }
    pub fn set_calc_end(&mut self, gas_type: GasType, value: f64) {
        let range_max = self.get_adjusted_open();
        // the calc area cant go beyond the measurement area
        if value > range_max {
            self.calc_range_end.insert(gas_type, range_max);
        } else {
            self.calc_range_end.insert(gas_type, value);
        }
    }

    pub fn _increment_lag(&mut self, value: f64) {
        self.open_lag_s += value;

        let range_min = self.get_adjusted_close(); // new earliest bound
        let range_max = self.get_adjusted_open(); // new latest bound
        let min_range = self.min_calc_range;

        for gas_type in self.gases.iter().copied() {
            let mut start = *self.calc_range_start.get(&gas_type).unwrap_or(&range_min);
            let mut end = *self.calc_range_end.get(&gas_type).unwrap_or(&range_max);

            // Clamp start and end within measurement bounds
            if start < range_min {
                start = range_min;
            }
            if end > range_max {
                end = range_max;
            }

            // Ensure the range is not shorter than allowed
            let current_range = end - start;
            if current_range < min_range {
                let needed = min_range - current_range;
                let half = needed / 2.0;

                // Try to expand symmetrically, clamp to bounds
                let new_start = (start - half).max(range_min);
                let new_end = (end + half).min(range_max);

                // Recalculate final range
                if new_end - new_start >= min_range {
                    start = new_start;
                    end = new_end;
                } else {
                    // Fallback: stretch one side only
                    end = start + min_range;
                    if end > range_max {
                        start = range_max - min_range;
                        end = range_max;
                    }
                }
            }

            self.calc_range_start.insert(gas_type, start);
            self.calc_range_end.insert(gas_type, end);
        }
    }
    pub fn set_close_lag(&mut self, new_lag: f64) {
        self.close_lag_s = new_lag;

        let range_min = self.get_adjusted_close();
        let range_max = self.get_adjusted_open();
        let min_range = self.min_calc_range;

        for gas_type in self.gases.iter().copied() {
            let mut start = *self.calc_range_start.get(&gas_type).unwrap_or(&range_min);
            let mut end = *self.calc_range_end.get(&gas_type).unwrap_or(&range_max);

            // Clamp to bounds
            if start < range_min {
                start = range_min;
            }
            if end > range_max {
                end = range_max;
            }

            // Ensure min range
            let current_range = end - start;
            if current_range < min_range {
                let needed = min_range - current_range;
                let half = needed / 2.0;

                let new_start = (start - half).max(range_min);
                let new_end = (end + half).min(range_max);

                if new_end - new_start >= min_range {
                    start = new_start;
                    end = new_end;
                } else {
                    end = start + min_range;
                    if end > range_max {
                        start = range_max - min_range;
                        end = range_max;
                    }
                }
            }

            self.calc_range_start.insert(gas_type, start);
            self.calc_range_end.insert(gas_type, end);
        }
        self.get_measurement_datas();
        self.check_errors();
        self.find_highest_r_windows();
        self.get_calc_datas();
        self.calculate_calc_rs();
        self.calculate_measurement_rs();
        self.compute_all_fluxes();
    }

    pub fn set_start_lag_s(&mut self, new_lag: f64) {
        let old_lag = self.start_lag_s;
        self.start_lag_s = new_lag;
        if self.get_start() > self.get_adjusted_close() {
            self.start_lag_s = old_lag;
            println!("Can't remove data from measurement.");
            return;
        }
        self.reload_gas_data();
    }
    pub fn calculate_concentration_at_t0(&mut self) {
        for gas_type in self.gases.clone() {
            if self.measurement_gas_v.get(&gas_type).unwrap().is_empty() {
                self.t0_concentration.insert(gas_type, 0.0);
            } else {
                let t0 =
                    self.measurement_gas_v.get(&gas_type).unwrap().first().unwrap().unwrap_or(0.0);
                self.t0_concentration.insert(gas_type, t0);
            }
        }
    }

    pub fn set_end_lag_s(&mut self, new_lag: f64) {
        let old_lag = self.end_lag_s;
        self.end_lag_s = new_lag;
        if self.get_adjusted_open() > self.get_end() {
            self.end_lag_s = old_lag;
            println!("Can't remove data from the measurement.");
            return;
        }
        self.reload_gas_data();
    }

    pub fn set_open_lag(&mut self, new_lag: f64) {
        self.open_lag_s = new_lag;

        let range_min = self.get_adjusted_close();
        let range_max = self.get_adjusted_open();
        let min_range = self.min_calc_range;

        for gas_type in self.gases.iter().copied() {
            let mut start = *self.calc_range_start.get(&gas_type).unwrap_or(&range_min);
            let mut end = *self.calc_range_end.get(&gas_type).unwrap_or(&range_max);

            // Clamp to bounds
            if start < range_min {
                start = range_min;
            }
            if end > range_max {
                end = range_max;
            }

            // Ensure min range
            let current_range = end - start;
            if current_range < min_range {
                let needed = min_range - current_range;
                let half = needed / 2.0;

                let new_start = (start - half).max(range_min);
                let new_end = (end + half).min(range_max);

                if new_end - new_start >= min_range {
                    start = new_start;
                    end = new_end;
                } else {
                    end = start + min_range;
                    if end > range_max {
                        start = range_max - min_range;
                        end = range_max;
                    }
                }
            }

            self.calc_range_start.insert(gas_type, start);
            self.calc_range_end.insert(gas_type, end);
        }
        self.get_measurement_datas();
        self.check_errors();
        self.find_highest_r_windows();
        self.get_calc_datas();
        self.calculate_calc_rs();
        self.calculate_measurement_rs();
        self.compute_all_fluxes();
    }

    pub fn set_measurement_start(&mut self, value: f64) {
        self.measurement_range_start = value;
    }
    pub fn set_measurement_end(&mut self, value: f64) {
        self.measurement_range_end = value;
    }
    pub fn get_calc_start(&self, gas_type: GasType) -> f64 {
        *self.calc_range_start.get(&gas_type).unwrap_or(&0.0)
    }
    pub fn get_calc_end(&self, gas_type: GasType) -> f64 {
        *self.calc_range_end.get(&gas_type).unwrap_or(&0.0)
    }
    pub fn _get_measurement_start(&mut self) -> f64 {
        self.measurement_range_start
    }
    pub fn _get_measurement_end(&mut self) -> f64 {
        self.measurement_range_end
    }
    pub fn set_automatic_valid(&mut self, valid: bool) {
        if self.override_valid.is_none() {
            self.is_valid = valid && self.error_code.0 == 0; // Ensure error codes affect validity
        }
    }
    pub fn toggle_manual_valid(&mut self) {
        let before_valid = self.is_valid;
        let before_override = self.override_valid;
        let before_errors = self.error_code.0;

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

        let after_valid = self.is_valid;
        let after_override = self.override_valid;
        let after_errors = self.error_code.0;

        if before_valid != after_valid
            || before_override != after_override
            || before_errors != after_errors
        {
            self.manual_adjusted = true;
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
                    let lags = (peak_time
                        - (self.start_time + chrono::TimeDelta::seconds(self.open_offset)))
                    .num_seconds() as f64;
                    self.set_open_lag(lags);

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
                    self.open_lag_s = (peak_time
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
            + chrono::TimeDelta::seconds(self.open_lag_s as i64)
    }

    pub fn calculate_max_y(&mut self) {
        for (gas_type, gas_v) in &self.gas_v {
            let max_value = gas_v
            .iter()
            .filter_map(|&v| v) // discard None
            .filter(|v| !v.is_nan())
            .fold(f64::NEG_INFINITY, f64::max);

            self.max_y.insert(*gas_type, max_value);
        }
    }

    pub fn calculate_min_y(&mut self) {
        for (gas_type, gas_v) in &self.gas_v {
            let min_value = gas_v
            .iter()
            .filter_map(|&v| v) // discard None
            .filter(|v| !v.is_nan())
            .fold(f64::INFINITY, f64::min);

            self.min_y.insert(*gas_type, min_value);
        }
    }
    // pub fn calculate_min_y(&mut self) {
    //     for (gas_type, gas_v) in &self.gas_v {
    //         let min_value =
    //             gas_v.iter().copied().filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min);
    //
    //         self.min_y.insert(*gas_type, min_value);
    //     }
    // }

    pub fn calculate_measurement_r(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
            let dt_vv: Vec<f64> =
                self.measurement_dt_v.iter().map(|x| x.timestamp() as f64).collect();

            // Zip and filter together so only valid (Some) pairs remain
            let filtered: Vec<(f64, f64)> = dt_vv
                .iter()
                .zip(gas_v.iter())
                .filter_map(|(&t, &g)| g.map(|g_val| (t, g_val)))
                .collect();

            // Unzip to separate vectors again
            let (filtered_x, filtered_y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

            // Calculate and store r^2
            let r2 = stats::pearson_correlation(&filtered_x, &filtered_y).unwrap_or(0.0).powi(2);
            self.measurement_r2.insert(gas_type, r2);
        }
    }
    // pub fn calculate_measurement_r(&mut self, gas_type: GasType) {
    //     if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
    //         let dt_vv: Vec<f64> =
    //             self.measurement_dt_v.iter().map(|x| x.timestamp() as f64).collect();
    //         // self.measurement_r = stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0);
    //         self.measurement_r2
    //             .insert(gas_type, stats::pearson_correlation(&dt_vv, gas_v).unwrap_or(0.0).powi(2));
    //     }
    // }
    // pub fn calculate_calc_r(&mut self, gas_type: GasType) {
    //     if let Some(gas_v) = self.gas_v.get(&gas_type) {
    //         let s = self.calc_range_start.get(&gas_type).unwrap();
    //         let e = self.calc_range_end.get(&gas_type).unwrap();
    //         let mut dt_v: Vec<_> = Vec::new();
    //         let mut filtered_gas_v: Vec<_> = Vec::new();
    //
    //         self.dt_v
    //             .iter()
    //             .zip(gas_v.iter()) // Pair timestamps with gas values
    //             .filter(|(t, _)| (t.timestamp() as f64) >= *s && (t.timestamp() as f64) <= *e) // Filter by time range
    //             .for_each(|(t, d)| {
    //                 dt_v.push(*t);
    //                 filtered_gas_v.push(*d);
    //             });
    //
    //         let dt_vv: Vec<f64> = dt_v.iter().map(|x| x.timestamp() as f64).collect();
    //         // self.calc_r = stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0);
    //         self.calc_r2.insert(
    //             gas_type,
    //             stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0).powi(2),
    //         );
    //     }
    // }
    pub fn calculate_calc_r(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let s = *self.calc_range_start.get(&gas_type).unwrap();
            let e = *self.calc_range_end.get(&gas_type).unwrap();

            let filtered: Vec<(f64, f64)> = self
                .dt_v
                .iter()
                .zip(gas_v.iter())
                .filter_map(|(t, v)| {
                    let ts = t.timestamp() as f64;
                    if ts >= s && ts <= e {
                        v.map(|val| (ts, val))
                    } else {
                        None
                    }
                })
                .collect();

            let (x, y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

            self.calc_r2
                .insert(gas_type, stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2));
        }
    }

    pub fn calculate_calc_rs(&mut self) {
        for gas_type in self.gases.clone() {
            if let Some(gas_v) = self.gas_v.get(&gas_type) {
                let s = *self.calc_range_start.get(&gas_type).unwrap();
                let e = *self.calc_range_end.get(&gas_type).unwrap();

                let filtered: Vec<(f64, f64)> = self
                    .dt_v
                    .iter()
                    .zip(gas_v.iter())
                    .filter_map(|(t, v)| {
                        let ts = t.timestamp() as f64;
                        if ts >= s && ts <= e {
                            v.map(|val| (ts, val))
                        } else {
                            None
                        }
                    })
                    .collect();

                let (x, y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

                self.calc_r2
                    .insert(gas_type, stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2));
            }
        }
    }
    // pub fn calculate_calc_rs(&mut self) {
    //     for gas_type in self.gases.clone() {
    //         if let Some(gas_v) = self.gas_v.get(&gas_type) {
    //             let s = self.calc_range_start.get(&gas_type).unwrap();
    //             let e = self.calc_range_end.get(&gas_type).unwrap();
    //             let mut dt_v: Vec<_> = Vec::new();
    //             let mut filtered_gas_v: Vec<_> = Vec::new();
    //
    //             self.dt_v
    //             .iter()
    //             .zip(gas_v.iter()) // Pair timestamps with gas values
    //             .filter(|(t, _)| (t.timestamp() as f64) >= *s && (t.timestamp() as f64) <= *e) // Filter by time range
    //             .for_each(|(t, d)| {
    //                 dt_v.push(*t);
    //                 filtered_gas_v.push(*d);
    //             });
    //
    //             let dt_vv: Vec<f64> = dt_v.iter().map(|x| x.timestamp() as f64).collect();
    //             // self.calc_r = stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0);
    //             self.calc_r2.insert(
    //                 gas_type,
    //                 stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0).powi(2),
    //             );
    //         }
    //     }
    // }
    pub fn find_highest_r_windows(&mut self) {
        // Precompute timestamps as float
        let dt_v: Vec<f64> = self.measurement_dt_v.iter().map(|dt| dt.timestamp() as f64).collect();

        // Precompute timestamp gaps (difference > 1.0 sec)
        let gaps: Vec<bool> = dt_v.windows(2).map(|w| (w[1] - w[0]).abs() > 1.0).collect();

        // Run analysis in parallel for all gases
        let results: Vec<_> = self
            .gases
            .par_iter()
            .filter_map(|&gas| {
                let gas_v = self.measurement_gas_v.get(&gas)?;

                if gas_v.len() < MIN_WINDOW_SIZE || dt_v.len() < MIN_WINDOW_SIZE {
                    return None;
                }

                find_best_window_for_gas(&dt_v, gas_v, &gaps, MIN_WINDOW_SIZE, WINDOW_INCREMENT)
                    .map(|(start, end, r2, best_y)| (gas, start, end, r2, best_y))
            })
            .collect();

        // Store results
        for (gas, start, end, r2, best_y) in results {
            self.calc_r2.insert(gas, r2);
            self.calc_range_start.insert(gas, self.measurement_dt_v[start].timestamp() as f64);
            self.calc_range_end
                .insert(gas, self.measurement_dt_v[end.saturating_sub(1)].timestamp() as f64);
            self.calc_dt_v.insert(gas, self.measurement_dt_v[start..end].to_vec());
            self.calc_gas_v.insert(gas, best_y);
        }
    }
    pub fn find_highest_r_window(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
            if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
                println!("Short data");
                return;
            }

            // Keep everything aligned with Option<f64>
            let dt_v: Vec<f64> =
                self.measurement_dt_v.iter().map(|dt| dt.timestamp() as f64).collect();

            let max_window = gas_v.len();
            let mut max_r = f64::MIN;
            let mut start_idx = 0;
            let mut end_idx = 0;

            for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
                for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
                    let end = start + win_size;

                    let x_win = &dt_v[start..end];
                    let y_win = &gas_v[start..end];

                    // Extract only the (Some) valid pairs
                    let valid: Vec<(f64, f64)> = x_win
                        .iter()
                        .zip(y_win.iter())
                        .filter_map(|(&x, &y)| y.map(|val| (x, val)))
                        .collect();

                    let (x_vals, y_vals): (Vec<f64>, Vec<f64>) = valid.into_iter().unzip();

                    let r = stats::pearson_correlation(&x_vals, &y_vals).unwrap_or(0.0);
                    if r > max_r {
                        max_r = r;
                        start_idx = start;
                        end_idx = end;
                    }
                }
            }

            // Store full window (including None values) from original slice
            if max_r != f64::MIN {
                self.calc_r2.insert(gas_type, max_r);
                self.calc_range_start
                    .insert(gas_type, self.measurement_dt_v[start_idx].timestamp() as f64);
                self.calc_range_end
                    .insert(gas_type, self.measurement_dt_v[end_idx - 1].timestamp() as f64);
                self.calc_dt_v.insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
                self.calc_gas_v.insert(gas_type, gas_v[start_idx..end_idx].to_vec());
                // <- keeps Option<f64>
            }
        }
    }
    // pub fn find_highest_r_window(&mut self, gas_type: GasType) {
    //     if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
    //         if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
    //             println!("Short data");
    //             return;
    //         }
    //
    //         let max_window = gas_v.len();
    //         let mut max_r = f64::MIN;
    //         let mut start_idx = 0;
    //         let mut end_idx = 0;
    //         let dt_v: Vec<f64> =
    //             self.measurement_dt_v.iter().map(|dt| dt.timestamp() as f64).collect();
    //         let mut best_window_y = Vec::new();
    //
    //         for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
    //             for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
    //                 let end = start + win_size;
    //                 let x_win = &dt_v[start..end];
    //                 let y_win = &gas_v[start..end];
    //                 let r = stats::pearson_correlation(x_win, y_win).unwrap_or(0.0);
    //                 if r > max_r {
    //                     max_r = r;
    //                     start_idx = start;
    //                     end_idx = end;
    //                     best_window_y = y_win.to_vec();
    //                 }
    //             }
    //         }
    //
    //         self.calc_r2.insert(gas_type, max_r);
    //         self.calc_range_start
    //             .insert(gas_type, self.measurement_dt_v[start_idx].timestamp() as f64);
    //         self.calc_range_end
    //             .insert(gas_type, self.measurement_dt_v[end_idx - 1].timestamp() as f64);
    //         self.calc_dt_v.insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
    //         self.calc_gas_v.insert(gas_type, best_window_y);
    //     }
    // }
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
                    + chrono::TimeDelta::seconds(
                        self.close_offset + self.open_lag_s as i64 + self.close_lag_s as i64,
                    );
                let open_time = self.start_time
                    + chrono::TimeDelta::seconds(self.open_offset + self.open_lag_s as i64);
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

                let filtered: Vec<(f64, f64)> = dt_vv
                    .iter()
                    .zip(gas_v.iter())
                    .filter_map(|(&t, &v)| v.map(|val| (t, val)))
                    .collect();

                let (x, y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

                self.measurement_r2
                    .insert(gas_type, stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2));
            }
        }
    }
    // pub fn calculate_measurement_rs(&mut self) {
    //     for &gas_type in &self.gases {
    //         if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
    //             let dt_vv: Vec<f64> =
    //                 self.measurement_dt_v.iter().map(|x| x.timestamp() as f64).collect();
    //
    //             self.measurement_r2.insert(
    //                 gas_type,
    //                 stats::pearson_correlation(&dt_vv, gas_v).unwrap_or(0.0).abs().powi(2),
    //             );
    //         }
    //     }
    // }

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
    pub fn check_main_r(&mut self) {
        if self.measurement_r2.get(&self.main_gas).unwrap_or(&0.0) < &0.98 {
            self.add_error(ErrorCode::LowR);
        } else {
            self.remove_error(ErrorCode::LowR)
        }
    }

    pub fn check_missing(&mut self) {
        if let Some(values) = self.gas_v.get(&self.main_gas) {
            let valid_count = values.iter().filter(|v| v.is_some()).count();
            let threshold = self.end_offset as f64 * 0.7;
            let check = (valid_count as f64) < threshold;

            if check {
                self.add_error(ErrorCode::TooFewMeasurements);
            } else {
                self.remove_error(ErrorCode::TooFewMeasurements);
            }
        } else {
            self.add_error(ErrorCode::TooFewMeasurements);
        }
    }
    // pub fn check_missing(&mut self) {
    //     if let Some(values) = self.gas_v.get(&self.main_gas) {
    //         let valid_count = values.iter().filter(|v| !v.is_nan()).count();
    //         let threshold = self.end_offset as f64 * 0.7;
    //         let check = (valid_count as f64) < threshold;
    //
    //         if check {
    //             self.add_error(ErrorCode::TooFewMeasurements);
    //         } else {
    //             self.remove_error(ErrorCode::TooFewMeasurements);
    //         }
    //     } else {
    //         // Handle the missing key case however you want
    //         self.add_error(ErrorCode::TooFewMeasurements);
    //     }
    // }
    pub fn check_errors(&mut self) {
        self.check_main_r();
        self.check_measurement_diag();
        self.check_missing();
        if self.error_code.0 == 0 || self.override_valid == Some(true) {
            self.is_valid = true
        }
    }
    pub fn reset(&mut self) {
        self.manual_adjusted = false;
        self.close_lag_s = 0.;
        self.open_lag_s = 0.;
        if self.end_lag_s != 0. || self.start_lag_s != 0. {
            self.end_lag_s = 0.;
            self.start_lag_s = 0.;
            self.reload_gas_data();
        }
        self.check_diag();
        self.check_missing();

        if !self.has_error(ErrorCode::TooManyDiagErrors)
            && !self.has_error(ErrorCode::TooFewMeasurements)
        {
            self.get_peak_datetime(self.main_gas);
            self.get_measurement_datas();
            self.calculate_concentration_at_t0();
            self.calculate_measurement_rs();
            self.check_main_r();
            self.find_highest_r_windows();
            self.compute_all_fluxes();
            self.calculate_max_y();
            self.calculate_min_y();
            self.check_errors();
        }
    }
    pub fn change_measurement_range(&mut self) {
        self.get_measurement_datas();
        self.calculate_measurement_rs();
        self.find_highest_r_windows();
        self.compute_all_fluxes();
    }
    pub fn recalc_r(&mut self) {
        self.find_highest_r_windows();
        self.compute_all_fluxes();
    }

    pub fn update_calc_attributes(&mut self, gas_type: GasType) {
        self.get_calc_data(gas_type);
        self.calculate_calc_r(gas_type);
        self.compute_single_flux(gas_type);
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
            let close_time = self.start_time
                + chrono::TimeDelta::seconds(
                    self.close_offset + self.open_lag_s as i64 + self.close_lag_s as i64,
                );
            let open_time = self.start_time
                + chrono::TimeDelta::seconds(self.open_offset + self.open_lag_s as i64);
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
            let time_vec: Vec<f64> = self
                .calc_dt_v
                .get(&gas_type)
                .unwrap()
                .iter()
                .map(|dt| dt.timestamp() as f64)
                .collect();

            // Filter (x, y) pairs where y is Some
            let filtered: Vec<(f64, f64)> = time_vec
                .iter()
                .zip(gas_v.iter())
                .filter_map(|(&t, &v)| v.map(|val| (t, val)))
                .collect();

            let (x_vals, y_vals): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

            let linreg = stats::LinReg::train(&x_vals, &y_vals);
            self.linfit.insert(gas_type, linreg);
        } else {
            self.linfit.insert(gas_type, LinReg::default());
        }
    }

    // pub fn recalculate_fluxes(&mut self) {
    //     for &gas in &self.gases.clone() {
    //         self.calculate_flux(gas);
    //     }
    // }
    pub fn compute_all_fluxes(&mut self) {
        for &gas in &self.gases.clone() {
            self.calculate_slope(gas);
            self.calculate_flux(gas);
        }
    }
    pub fn compute_single_flux(&mut self, gas: GasType) {
        self.calculate_slope(gas);
        self.calculate_flux(gas);
    }

    pub fn calculate_flux(&mut self, gas_type: GasType) {
        let mol_mass = gas_type.mol_mass();
        let slope_ppm = self.linfit.get(&gas_type).unwrap().slope / gas_type.conv_factor();
        let slope_ppm_hour = slope_ppm * 60. * 60.;
        let p = self.air_pressure * 100.0;
        let t = self.air_temperature + 273.15;
        let r = 8.314;
        let flux = slope_ppm_hour / 1_000_000.0
            * self.chamber_volume
            * ((mol_mass * p) / (r * t))
            * 1000.0;

        self.flux.insert(gas_type, flux);
    }

    pub fn ppb_to_nmol(&mut self) {
        // Constants
        const R: f64 = 8.314462618; // J/mol·K

        let pressure_pa = self.air_pressure * 100.0; // Convert hPa to Pa
        let temperature_k = self.air_temperature + 273.15; // Convert °C to K
        let volume_m3 = self.chamber_volume / 1000.0; // Convert L to m³

        let conversion_factor = (pressure_pa * volume_m3) / (R * temperature_k); // mol / mol-fraction
        let ppb_to_nmol = conversion_factor * 1e-9 * 1e9; // mol → nmol, and ppb = 1e-9
        let mut converted: HashMap<GasType, Vec<Option<f64>>> = HashMap::new();
        for gas_type in self.gases.clone() {
            if let Some(values) = self.gas_v.get(&gas_type) {
                let new_vals = values.iter().map(|v| v.map(|val| val * ppb_to_nmol)).collect();
                converted.insert(gas_type, new_vals);
            }
            // if let Some(values) = self.gas_v.get_mut(&gas_type) {
            //     for value in values.iter_mut().flatten() {
            //         let val = *value *= ppb_to_nmol;
            //     }
            // }
        }
        self.gas_v_mole = converted;
    }
    pub fn update_cycle(&mut self, _project: String) {
        self.get_calc_datas();
        self.get_measurement_datas();
        self.calculate_measurement_rs();
        self.find_highest_r_windows();
        self.check_errors();
        self.compute_all_fluxes();
    }
    pub fn reload_gas_data(&mut self) {
        println!("###### Reload gas data #######");
        let conn = match Connection::open("fluxrs.db") {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to open database: {}", e);
                return;
            },
        };

        let start = match DateTime::from_timestamp(self.get_start() as i64, 0) {
            Some(dt) => dt,
            None => {
                eprintln!("Invalid start timestamp for cycle");
                return;
            },
        };
        let end = match DateTime::from_timestamp(self.get_end() as i64, 0) {
            Some(dt) => dt,
            None => {
                eprintln!("Invalid end timestamp for cycle");
                return;
            },
        };

        match query_gas_all(
            &conn,
            start,
            end,
            self.project_name.clone(),
            self.instrument_serial.clone(),
        ) {
            Ok(gasdata) => {
                self.gas_v = gasdata.gas;
                self.dt_v = gasdata.datetime;
                self.diag_v = gasdata.diag;
            },
            Err(e) => {
                eprintln!("Error while loading gas data: {}", e);
            },
        }
        self.get_calc_datas();
        self.get_measurement_datas();
        self.calculate_max_y();
        self.calculate_min_y();
    }
}
#[derive(Debug, Default, Clone)]
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
            min_calc_range: MIN_CALC_AREA_RANGE,
            close_time: start + chrono::Duration::seconds(close),
            open_time: start + chrono::Duration::seconds(open),
            end_time: start + chrono::Duration::seconds(end),
            close_offset: close,
            open_offset: open,
            end_offset: end,
            main_gas: GasType::CH4,
            error_code: ErrorMask(0),
            manual_adjusted: false,
            calc_range_end: HashMap::new(),
            calc_range_start: HashMap::new(),
            min_y: HashMap::new(),
            max_y: HashMap::new(),
            t0_concentration: HashMap::new(),
            open_lag_s: 0.,
            close_lag_s: 0.,
            end_lag_s: 0.,
            start_lag_s: 0.,
            max_idx: 0.,
            flux: HashMap::new(),
            fluxes: Vec::new(),
            linfit: HashMap::new(),
            calc_r2: HashMap::new(),
            measurement_r2: HashMap::new(),
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            diag_v: vec![],
            dt_v: vec![],
            gas_v: HashMap::new(),
            gas_v_mole: HashMap::new(),
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
    pub fn build(self) -> Result<Cycle, Box<dyn std::error::Error + Send + Sync>> {
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
            min_calc_range: MIN_CALC_AREA_RANGE,
            project_name: project,
            start_time: start,
            close_time: start + chrono::Duration::seconds(close),
            open_time: start + chrono::Duration::seconds(open),
            end_time: start + chrono::Duration::seconds(end),
            close_offset: close,
            open_offset: open,
            end_offset: end,
            error_code: ErrorMask(0),
            main_gas: GasType::CH4,
            manual_adjusted: false,
            calc_range_end: HashMap::new(),
            calc_range_start: HashMap::new(),
            min_y: HashMap::new(),
            max_y: HashMap::new(),
            t0_concentration: HashMap::new(),
            open_lag_s: 0.,
            close_lag_s: 0.,
            end_lag_s: 0.,
            start_lag_s: 0.,
            max_idx: 0.,
            flux: HashMap::new(),
            fluxes: Vec::new(),
            linfit: HashMap::new(),
            calc_r2: HashMap::new(),
            measurement_r2: HashMap::new(),
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            diag_v: vec![],
            dt_v: vec![],
            gas_v: HashMap::new(),
            gas_v_mole: HashMap::new(),
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

pub fn insert_fluxes_ignore_duplicates(
    conn: &mut Connection,
    cycles: &[Option<Cycle>],
    project: String,
) -> Result<(usize, usize)> {
    let mut inserted = 0;
    let mut skipped = 0;
    let tx = conn.transaction()?; // Start transaction for bulk insertion

    {
        let mut insert_stmt = tx.prepare(&make_insert_or_ignore_fluxes())?;
        for cycle in cycles {
            match cycle {
                Some(c) => {
                    execute_insert(&mut insert_stmt, c, &project)?;
                    inserted += 1;
                },
                None => {
                    skipped += 1;
                },
            }
            // execute_insert(&mut insert_stmt, cycle, &project)?;
            // inserted += 1;
        }
    }

    tx.commit()?;
    Ok((inserted, skipped))
}

pub fn insert_flux_results(
    conn: &mut Connection,
    cycle_id: i64,
    fluxes: &[Box<dyn FluxModel>],
) -> rusqlite::Result<(usize, usize)> {
    let mut inserted = 0;
    let mut skipped = 0;

    let tx = conn.transaction()?;

    {
        let mut stmt = tx.prepare(&make_insert_flux_results())?;

        for model in fluxes {
            // Only handling LinearFlux for now — add others as needed
            if let Some(lin) = model.as_any().downcast_ref::<LinearFlux>() {
                // Skip if flux is NaN or invalid
                if lin.flux.is_nan() || lin.r2.is_nan() {
                    skipped += 1;
                    continue;
                }

                stmt.execute(params![
                    cycle_id,
                    lin.fit_id,
                    lin.gas_type.to_string(),
                    lin.flux,
                    lin.r2,
                    lin.model.intercept,
                    lin.model.slope,
                    lin.range_start,
                    lin.range_end,
                ])?;
                inserted += 1;
            } else {
                skipped += 1;
            }
        }
    }

    tx.commit()?;
    Ok((inserted, skipped))
}
// pub fn insert_flux_results(
//     conn: &mut Connection,
//     cycle_id: i64,
//     fluxes: &[Box<dyn FluxModel>],
// ) -> Result<usize, usize> {
//     let mut inserted = 0;
//     let mut skipped = 0;
//     let tx = conn.transaction()?;
//
//     {
//         let mut stmt = tx.prepare(
//             "INSERT INTO flux_results (
//             cycle_id, fit_id, gas_type,
//             flux, r2, intercept, slope,
//             range_start, range_end
//         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
//         )?;
//
//         for model in fluxes {
//             if let Some(lin) = model.as_any().downcast_ref::<LinearFlux>() {
//                 match model {
//                     Some(m) => {
//                         stmt.execute(params![
//                             cycle_id,
//                             lin.fit_id,
//                             lin.gas_type.to_string(),
//                             lin.flux,
//                             lin.r2,
//                             lin.model.intercept,
//                             lin.model.slope,
//                             lin.range_start,
//                             lin.range_end,
//                         ])?;
//                     },
//                     None => println!("fail"),
//                 };
//             }
//             // Add other model types here if needed
//         }
//     }
//
//     tx.commit()?;
//     Ok((inserted, skipped))
// }
pub fn update_fluxes(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tx = conn.transaction()?; // Start transaction for consistency
    {
        let mut update_stmt = tx.prepare(&make_update_fluxes())?;

        for cycle in cycles {
            match execute_update(&mut update_stmt, cycle, &project) {
                Ok(_) => println!("Fluxes updated successfully!"),
                Err(e) => eprintln!("Error updating fluxes: {}", e),
            }
        }
    }
    tx.commit()?;
    Ok(())
}
pub fn insert_flux_history(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let archived_at = Utc::now().to_rfc3339();
    let tx = conn.transaction()?; // Start transaction for consistency
    {
        let mut insert_stmt = tx.prepare(&make_insert_flux_history())?;

        for cycle in cycles {
            match execute_history_insert(&mut insert_stmt, &archived_at, cycle, &project) {
                Ok(_) => println!("Archived cycle successfully."),
                Err(e) => eprintln!("Error archiving fluxes: {}", e),
            }
        }
    }
    tx.commit()?;
    Ok(())
}
fn execute_history_insert(
    stmt: &mut rusqlite::Statement,
    archived_at: &String,
    cycle: &Cycle,
    project: &String,
) -> Result<()> {
    stmt.execute(params![
        archived_at,
        cycle.start_time.timestamp(),
        cycle.chamber_id,
        cycle.instrument_model.to_string(),
        cycle.instrument_serial,
        cycle.main_gas.column_name(),
        project,
        cycle.close_offset,
        cycle.open_offset,
        cycle.end_offset,
        cycle.open_lag_s as i64,
        cycle.close_lag_s as i64,
        cycle.end_lag_s as i64,
        cycle.start_lag_s as i64,
        cycle.air_pressure,
        cycle.air_temperature,
        cycle.chamber_volume,
        cycle.error_code.0,
        cycle.is_valid,
        cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
        cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::CH4).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::CH4).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::CO2).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::CO2).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::H2O).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::H2O).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::N2O).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::N2O).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.manual_adjusted,
        cycle.manual_valid,
    ])?;
    Ok(())
}
fn execute_insert(stmt: &mut rusqlite::Statement, cycle: &Cycle, project: &String) -> Result<()> {
    stmt.execute(params![
        cycle.start_time.timestamp(),
        cycle.chamber_id,
        cycle.instrument_model.to_string(),
        cycle.instrument_serial,
        cycle.main_gas.column_name(),
        project,
        cycle.close_offset,
        cycle.open_offset,
        cycle.end_offset,
        cycle.open_lag_s as i64,
        cycle.close_lag_s as i64,
        cycle.end_lag_s as i64,
        cycle.start_lag_s as i64,
        cycle.air_pressure,
        cycle.air_temperature,
        cycle.chamber_volume,
        cycle.error_code.0,
        cycle.is_valid,
        cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
        cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::CH4).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::CH4).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::CO2).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::CO2).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::H2O).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::H2O).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::N2O).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::N2O).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.manual_adjusted,
        cycle.manual_valid,
    ])?;
    Ok(())
}
fn execute_update(stmt: &mut rusqlite::Statement, cycle: &Cycle, project: &String) -> Result<()> {
    stmt.execute(params![
        cycle.start_time.timestamp(),
        cycle.chamber_id,
        cycle.instrument_model.to_string(),
        cycle.instrument_serial,
        cycle.main_gas.column_name(),
        project,
        cycle.close_offset,
        cycle.open_offset,
        cycle.end_offset,
        cycle.open_lag_s as i64,
        cycle.close_lag_s as i64,
        cycle.end_lag_s as i64,
        cycle.start_lag_s as i64,
        cycle.air_pressure,
        cycle.air_temperature,
        cycle.chamber_volume,
        cycle.error_code.0,
        cycle.is_valid,
        cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
        cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::CH4).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::CH4).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::CO2).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::CO2).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::H2O).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::H2O).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.linfit.get(&GasType::N2O).unwrap_or(&LinReg::new()).intercept,
        cycle.linfit.get(&GasType::N2O).unwrap_or(&LinReg::new()).slope,
        cycle.calc_range_start.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.t0_concentration.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.manual_adjusted,
        cycle.manual_valid,
    ])?;
    Ok(())
}
// pub fn load_fluxes(
//     conn: &mut Connection,
//     start: DateTime<Utc>,
//     end: DateTime<Utc>,
//     project: String,
//     instrument_serial: String,
//     progress_sender: mpsc::UnboundedSender<ProcessEvent>,
// ) -> Result<Vec<Cycle>> {
//     let mut stmt = conn.prepare(&make_select_fluxes())?;
//     let gas_data = query_gas(conn, start, end, project.clone(), instrument_serial)?;
//     let s = start.timestamp();
//     let e = end.timestamp();
//     let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
//     let column_index: HashMap<String, usize> =
//         column_names.iter().enumerate().map(|(i, name)| (name.clone(), i)).collect();
//     let mut date: Option<String> = None;
//     let cycle_iter = stmt.query_map(params![s, e, project.clone()], |row| {
//         // Basic fields
//         let instrument_model: String = row.get(*column_index.get("instrument_model").unwrap())?;
//         let instrument_serial: String = row.get(*column_index.get("instrument_serial").unwrap())?;
//         let start_timestamp: i64 = row.get(*column_index.get("start_time").unwrap())?;
//         let chamber_id: String = row.get(*column_index.get("chamber_id").unwrap())?;
//         // let flux_key = (chamber_id.clone(), start_timestamp);
//
//         let gases = get_instrument_by_model(&instrument_model).unwrap().base.gas_cols;
//         let gastypes: Vec<GasType> =
//             gases.iter().filter_map(|name| name.parse::<GasType>().ok()).collect();
//
//         let main_gas_str: String = row.get(*column_index.get("main_gas").unwrap())?;
//         let main_gas = main_gas_str.parse::<GasType>().ok().unwrap();
//         let start_time = chrono::DateTime::from_timestamp(start_timestamp, 0).unwrap();
//         let day = start_time.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
//         if let Some(prev_date) = date.clone() {
//             if prev_date != day {
//                 progress_sender.send(ProcessEvent::Progress(ProgressEvent::Day(day.clone()))).ok();
//             }
//         }
//
//         date = Some(day.clone());
//         let close_offset: i64 = row.get(*column_index.get("close_offset").unwrap())?;
//         let open_offset: i64 = row.get(*column_index.get("open_offset").unwrap())?;
//         let end_offset: i64 = row.get(*column_index.get("end_offset").unwrap())?;
//         // needs to be on two rows
//         let open_lag_s: f64 = row.get(*column_index.get("open_lag_s").unwrap())?;
//         let close_lag_s: f64 = row.get(*column_index.get("close_lag_s").unwrap())?;
//         let end_lag_s: f64 = row.get(*column_index.get("end_lag_s").unwrap())?;
//         let start_lag_s: f64 = row.get(*column_index.get("start_lag_s").unwrap())?;
//
//         let air_pressure: f64 = row.get(*column_index.get("air_pressure").unwrap())?;
//         let air_temperature: f64 = row.get(*column_index.get("air_temperature").unwrap())?;
//
//         let error_code_u16: u16 = row.get(*column_index.get("error_code").unwrap())?;
//         let error_code = ErrorMask::from_u16(error_code_u16);
//
//         let is_valid: bool = row.get(*column_index.get("is_valid").unwrap())?;
//         // let main_gas_r2: f64 = row.get(fluxes_col::MAIN_GAS_R2)?;
//
//         // Compute derived times from start_time and offsets.
//         let close_time = start_time + TimeDelta::seconds(close_offset);
//         let open_time = start_time + TimeDelta::seconds(open_offset);
//         let end_time = start_time + TimeDelta::seconds(end_offset);
//
//         let gas_columns = vec![
//             (GasType::CH4, fluxes_col::CH4_FLUX),
//             (GasType::CO2, fluxes_col::CO2_FLUX),
//             (GasType::H2O, fluxes_col::H2O_FLUX),
//             (GasType::N2O, fluxes_col::N2O_FLUX),
//         ];
//
//         let manual_adjusted = row.get(*column_index.get("manual_adjusted").unwrap())?;
//         let project_name = row.get(*column_index.get("project_id").unwrap())?;
//         let manual_valid: bool = row.get(*column_index.get("manual_valid").unwrap())?;
//         let chamber_volume: f64 = row.get(*column_index.get("chamber_volume").unwrap())?;
//         let mut override_valid = None;
//         if manual_valid {
//             override_valid = Some(is_valid);
//         }
//         let filtered: Vec<(GasType, usize)> =
//             gas_columns.into_iter().filter(|(gas, _)| gastypes.contains(gas)).collect();
//         // Initialize the HashMaps.
//         let mut flux = HashMap::new();
//         let mut calc_r2 = HashMap::new();
//         // let mut measurement_r2 = HashMap::new();
//         let mut slope_map = HashMap::new();
//         let mut calc_range_start_map = HashMap::new();
//         let mut calc_range_end_map = HashMap::new();
//         let mut t0_concentration = HashMap::new();
//         let mut calc_gas_v = HashMap::new();
//         let mut calc_dt_v = HashMap::new();
//         let mut measurement_dt_v = Vec::new();
//         let measurement_diag_v = Vec::new();
//         let mut measurement_gas_v = HashMap::new();
//         let measurement_range_start = close_time + TimeDelta::seconds(open_lag_s as i64);
//         let measurement_range_end = open_time + TimeDelta::seconds(open_lag_s as i64);
//         let mut dt_v = Vec::new();
//         let mut diag_v = Vec::new();
//         let mut gas_v = HashMap::new();
//         let mut min_y = HashMap::new();
//         let mut max_y = HashMap::new();
//         let mut measurement_r2 = HashMap::new();
//
//         if let Some(gas_data_day) = gas_data.get(&day) {
//             // --- Calculation & Measurement Filtering for Each Gas ---
//             for (gas, _base_idx) in filtered {
//                 if let Some(g_values) = gas_data_day.gas.get(&gas) {
//                     // Here you extract per-gas values from the flux row.
//                     // (We assume that part of the code remains the same.)
//                     let gas_flux: f64 = row.get(*column_index.get(&gas.flux_col()).unwrap())?;
//                     let gas_r2: f64 = row.get(*column_index.get(&gas.r2_col()).unwrap())?;
//                     let gas_measurement_r2: f64 =
//                         row.get(*column_index.get(&gas.measurement_r2_col()).unwrap())?;
//                     let gas_slope = LinReg::from_val(
//                         row.get(*column_index.get(&gas.intercept_col()).unwrap())?,
//                         row.get(*column_index.get(&gas.slope_col()).unwrap())?,
//                     );
//                     let gas_calc_range_start: f64 =
//                         row.get(*column_index.get(&gas.calc_range_start_col()).unwrap())?;
//                     let gas_calc_range_end: f64 =
//                         row.get(*column_index.get(&gas.calc_range_end_col()).unwrap())?;
//                     let gas_t0_concentration: f64 =
//                         row.get(*column_index.get(&gas.t0_concentration_col()).unwrap())?;
//
//                     flux.insert(gas, gas_flux);
//                     calc_r2.insert(gas, gas_r2);
//                     measurement_r2.insert(gas, gas_measurement_r2);
//                     slope_map.insert(gas, gas_slope);
//                     calc_range_start_map.insert(gas, gas_calc_range_start);
//                     calc_range_end_map.insert(gas, gas_calc_range_end);
//                     calc_range_end_map.insert(gas, gas_calc_range_end);
//                     t0_concentration.insert(gas, gas_t0_concentration);
//
//                     // Filter for calculation range using the per-gas calc range.
//                     let (calc_dt, calc_vals) = filter_data_in_range(
//                         &gas_data_day.datetime,
//                         g_values,
//                         calc_range_start_map.get(&gas).copied().unwrap_or(0.0),
//                         calc_range_end_map.get(&gas).copied().unwrap_or(0.0),
//                     );
//
//                     calc_dt_v.insert(gas, calc_dt);
//                     calc_gas_v.insert(gas, calc_vals);
//
//                     // Filter for measurement range using the cycle's measurement range.
//                     let (meas_dt, meas_vals) = filter_data_in_range(
//                         &gas_data_day.datetime,
//                         g_values,
//                         measurement_range_start.timestamp() as f64,
//                         measurement_range_end.timestamp() as f64,
//                     );
//                     if gas == main_gas {
//                         // For the main gas, assign the filtered datetime vector.
//                         measurement_dt_v = meas_dt;
//                     }
//                     measurement_gas_v.insert(gas, meas_vals);
//                 }
//             }
//
//             // --- Overall Cycle Data Filtering ---
//             // Filter diagnostic data (diag_v) and full datetime (dt_v) for the entire cycle:
//             let (dt_v_full, diag_v_full) = filter_diag_data(
//                 &gas_data_day.datetime,
//                 &gas_data_day.diag,
//                 start_time.timestamp() as f64 + start_lag_s,
//                 end_time.timestamp() as f64 + end_lag_s,
//             );
//             dt_v = dt_v_full; // Assign overall datetime vector.
//             diag_v = diag_v_full; // Assign overall diagnostic vector.
//             if dt_v.is_empty() {
//                 return Ok(None); // Use `None` to skip cycle
//             }
//             for &gas in &gastypes {
//                 if let Some(g_values) = gas_data_day.gas.get(&gas) {
//                     let (_full_dt, full_vals) = filter_data_in_range(
//                         &gas_data_day.datetime,
//                         g_values,
//                         start_time.timestamp() as f64 + start_lag_s,
//                         end_time.timestamp() as f64 + end_lag_s,
//                     );
//                     max_y.insert(gas, calculate_max_y_from_vec(&full_vals));
//                     min_y.insert(gas, calculate_min_y_from_vec(&full_vals));
//                     gas_v.insert(gas, full_vals);
//                     // Optionally, store full_dt in a dedicated dt map if needed.
//                 }
//             }
//         }
//         Ok(Some(Cycle {
//             instrument_model: InstrumentType::from_str(&instrument_model),
//             instrument_serial,
//             project_name,
//             manual_adjusted,
//             chamber_id,
//             min_calc_range: MIN_CALC_AREA_RANGE,
//             start_time,
//             calc_dt_v,
//             t0_concentration,
//             calc_gas_v,
//             diag_v,
//             dt_v,
//             gas_v,
//             gas_v_mole: HashMap::new(),
//             max_y,
//             min_y,
//             measurement_dt_v,
//             measurement_gas_v,
//             measurement_diag_v,
//             close_time,
//             open_time,
//             end_time,
//             air_temperature,
//             air_pressure,
//             chamber_volume,
//             error_code,
//             is_valid,
//             override_valid,
//             manual_valid,
//             main_gas,
//             close_offset,
//             open_offset,
//             end_offset,
//             open_lag_s,
//             close_lag_s,
//             end_lag_s,
//             start_lag_s,
//             max_idx: 0.0, // Default value.
//             gases: gastypes,
//             calc_range_start: calc_range_start_map,
//             calc_range_end: calc_range_end_map,
//             // The following fields were not stored; use defaults.
//             measurement_range_start: (start_time
//                 + TimeDelta::seconds(start_lag_s as i64)
//                 + TimeDelta::seconds(close_offset)
//                 + TimeDelta::seconds(open_lag_s as i64))
//             .timestamp() as f64,
//             measurement_range_end: (start_time
//                 + TimeDelta::seconds(end_lag_s as i64)
//                 + TimeDelta::seconds(close_offset)
//                 + TimeDelta::seconds(open_lag_s as i64))
//             .timestamp() as f64,
//             linfit: slope_map,
//             flux,
//             fluxes: Vec::new(),
//             measurement_r2,
//             calc_r2,
//             // Other fields (dt_v, calc_dt_v, etc.) can be initialized as needed.
//         }))
//     })?;
//
//     let mut cycles: Vec<Cycle> =
//         cycle_iter.collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect();
//     if cycles.is_empty() {
//         // return Err("No cycles found".into());
//         return Err(rusqlite::Error::InvalidQuery);
//     }
//     Ok(cycles)
// }

pub fn load_cycles(
    conn: &Connection,
    project: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) -> Result<Vec<Cycle>> {
    let mut date: Option<String> = None;
    let start = start.timestamp();
    let end = end.timestamp();
    let gas_data = query_gas2(conn, start, end, project.to_owned())?;
    let mut stmt = conn.prepare(
        "SELECT * FROM fluxes
         WHERE project_id = ?1 AND start_time BETWEEN ?2 AND ?3
         ORDER BY start_time",
    )?;

    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let column_index: HashMap<String, usize> =
        column_names.iter().enumerate().map(|(i, name)| (name.clone(), i)).collect();
    let mut cycles = Vec::new();
    let rows = stmt.query_map(params![project, start, end], |row| {
        let id: i64 = row.get(*column_index.get("id").unwrap())?;
        let model_string: String = row.get(*column_index.get("instrument_model").unwrap())?;
        let instrument_model = InstrumentType::from_str(&model_string);
        let instrument_serial: String = row.get(*column_index.get("instrument_serial").unwrap())?;
        let start_timestamp: i64 = row.get(*column_index.get("start_time").unwrap())?;
        let chamber_id: String = row.get(*column_index.get("chamber_id").unwrap())?;

        let gastypes = get_instrument_by_model(instrument_model).unwrap().base.gas_cols;
        let gases: Vec<GasType> =
            gastypes.iter().filter_map(|name| name.parse::<GasType>().ok()).collect();

        let main_gas_str: String = row.get(*column_index.get("main_gas").unwrap())?;
        let main_gas = main_gas_str.parse::<GasType>().ok().unwrap();
        let start_time = chrono::DateTime::from_timestamp(start_timestamp, 0).unwrap();
        let day = start_time.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
        if let Some(prev_date) = date.clone() {
            if prev_date != day {
                progress_sender.send(ProcessEvent::Progress(ProgressEvent::Day(day.clone()))).ok();
            }
        }

        date = Some(day.clone());
        let close_offset: i64 = row.get(*column_index.get("close_offset").unwrap())?;
        let open_offset: i64 = row.get(*column_index.get("open_offset").unwrap())?;
        let end_offset: i64 = row.get(*column_index.get("end_offset").unwrap())?;
        // needs to be on two rows
        let open_lag_s: f64 = row.get(*column_index.get("open_lag_s").unwrap())?;
        let close_lag_s: f64 = row.get(*column_index.get("close_lag_s").unwrap())?;
        let end_lag_s: f64 = row.get(*column_index.get("end_lag_s").unwrap())?;
        let start_lag_s: f64 = row.get(*column_index.get("start_lag_s").unwrap())?;

        let air_pressure: f64 = row.get(*column_index.get("air_pressure").unwrap())?;
        let air_temperature: f64 = row.get(*column_index.get("air_temperature").unwrap())?;
        let chamber_volume: f64 = row.get(*column_index.get("chamber_volume").unwrap())?;

        let close_time = start_time + TimeDelta::seconds(close_offset);
        let open_time = start_time + TimeDelta::seconds(open_offset);
        let end_time = start_time + TimeDelta::seconds(end_offset);

        let error_code_u16: u16 = row.get(*column_index.get("error_code").unwrap())?;
        let error_code = ErrorMask::from_u16(error_code_u16);
        let is_valid: bool = row.get(*column_index.get("is_valid").unwrap())?;
        let project_name = row.get(*column_index.get("project_id").unwrap())?;
        let manual_adjusted = row.get(*column_index.get("manual_adjusted").unwrap())?;
        let manual_valid: bool = row.get(*column_index.get("manual_valid").unwrap())?;

        let mut override_valid = None;
        if manual_valid {
            override_valid = Some(is_valid);
        }

        let mut dt_v = Vec::new();
        let mut diag_v = Vec::new();
        let mut gas_v = HashMap::new();
        let mut min_y = HashMap::new();
        let mut max_y = HashMap::new();
        let mut t0_concentration = HashMap::new();
        let mut measurement_r2 = HashMap::new();
        let mut calc_range_start = HashMap::new();
        let mut calc_range_end = HashMap::new();

        if let Some(gas_data_day) = gas_data.get(&day) {
            for (i, gas) in instrument_model.available_gases().iter().enumerate() {
                if let Some(g_values) = gas_data_day.gas.get(&gas) {
                    let (meas_dt, meas_vals) = filter_data_in_range(
                        &gas_data_day.datetime,
                        g_values,
                        start_time.timestamp() as f64,
                        end_time.timestamp() as f64,
                    );
                    if i == 0 {
                        dt_v = meas_dt;
                    }
                    gas_v.insert(*gas, meas_vals.clone());
                    max_y.insert(*gas, calculate_max_y_from_vec(&meas_vals));
                    min_y.insert(*gas, calculate_min_y_from_vec(&meas_vals));
                    let target = close_offset + close_lag_s as i64 + open_lag_s as i64;

                    let s = target as usize;
                    let e = (open_offset + close_lag_s as i64 + open_lag_s as i64) as usize;

                    let y: Vec<f64> = dt_v[s..e].iter().map(|d| d.timestamp() as f64).collect();
                    let x: Vec<f64> = meas_vals[s..e].iter().map(|g| g.unwrap()).collect();
                    let r2 = stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2);
                    let t0 = meas_vals[target as usize].unwrap();
                    t0_concentration.insert(*gas, t0);
                    measurement_r2.insert(*gas, r2);
                }
                if i == 0 {
                    let (_, diag_vals) = filter_diag_data(
                        &gas_data_day.datetime,
                        &gas_data_day.diag,
                        start_time.timestamp() as f64,
                        end_time.timestamp() as f64,
                    );
                    diag_v = diag_vals;
                }
            }
        }
        // pub fn get_measurement_data2(&self, gas_type: GasType) -> (Vec<f64>, Vec<Option<f64>>) {
        //         let mut gas_ret = Vec::new();
        //         let mut dt_ret = Vec::new();
        //         if let Some(gas_v) = self.gas_v.get(&gas_type) {
        //             let s = self.get_adjusted_close_offset() as usize;
        //             let e = self.get_adjusted_open_offset() as usize;
        //             dt_ret = self.dt_v[s..e].iter().map(|d| d.timestamp() as f64).collect();
        //             gas_ret = gas_v[s..e].to_vec();
        //         }
        //         (dt_ret, gas_ret)
        //     }
        Ok(Cycle {
            id,
            instrument_model,
            instrument_serial,
            chamber_id,
            project_name,
            start_time,
            close_offset,
            open_offset,
            end_offset,
            open_lag_s,
            close_lag_s,
            end_lag_s,
            start_lag_s,
            max_idx: 0.0, // Default value.
            gases,
            calc_range_start,
            calc_range_end,
            // The following fields were not stored; use defaults.
            measurement_range_start: (start_time
                + TimeDelta::seconds(start_lag_s as i64)
                + TimeDelta::seconds(close_offset)
                + TimeDelta::seconds(open_lag_s as i64))
            .timestamp() as f64,
            measurement_range_end: (start_time
                + TimeDelta::seconds(end_lag_s as i64)
                + TimeDelta::seconds(close_offset)
                + TimeDelta::seconds(open_lag_s as i64))
            .timestamp() as f64,
            linfit: slope_map,
            flux,
            measurement_r2,
            calc_r2,
            // Other fields (dt_v, calc_dt_v, etc.) can be initialized as needed.
        })
    })?;

    let cycles: Vec<Cycle> = rows.collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect();
    if cycles.is_empty() {
        // return Err("No cycles found".into());
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(cycles)
}
// pub fn load_fluxes(
//     conn: &mut Connection,
//     start: DateTime<Utc>,
//     end: DateTime<Utc>,
//     project: String,
//     instrument_serial: String,
// ) -> Result<Vec<Cycle>> {
//     let mut stmt = conn.prepare(&make_select_fluxes())?;
//     let gas_data = query_gas(conn, start, end, project.clone(), instrument_serial)?;
//
//     let s = start.timestamp();
//     let e = end.timestamp();
//     let cycle_iter = stmt.query_map(params![s, e, project.clone()], |row| {
//         // Basic fields
//         let instrument_model: String = row.get(fluxes_col::INSTRUMENT_MODEL)?;
//         let instrument_serial: String = row.get(fluxes_col::INSTRUMENT_SERIAL)?;
//         let start_timestamp: i64 = row.get(fluxes_col::START_TIME)?;
//         let chamber_id: String = row.get(fluxes_col::CHAMBER_ID)?;
//
//         let gases = get_instrument_by_model(&instrument_model).unwrap().base.gas_cols;
//         let gastypes: Vec<GasType> =
//             gases.iter().filter_map(|name| name.parse::<GasType>().ok()).collect();
//
//         let main_gas_str: String = row.get(fluxes_col::MAIN_GAS)?;
//         // let main_gas = GasType::from_str(&main_gas_str).unwrap_or(GasType::CH4);
//         let main_gas = main_gas_str.parse::<GasType>().ok().unwrap();
//
//         let start_time = chrono::DateTime::from_timestamp(start_timestamp, 0).unwrap();
//         let day = start_time.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
//
//         let close_offset: i64 = row.get(fluxes_col::CLOSE_OFFSET)?;
//         let open_offset: i64 = row.get(fluxes_col::OPEN_OFFSET)?;
//         let end_offset: i64 = row.get(fluxes_col::END_OFFSET)?;
//         // needs to be on two rows
//         let open_lag_s: f64 = row.get(fluxes_col::OPEN_LAG_S)?;
//         let close_lag_s: f64 = row.get(fluxes_col::CLOSE_LAG_S)?;
//         let end_lag_s: f64 = row.get(fluxes_col::END_LAG_S)?;
//         let start_lag_s: f64 = row.get(fluxes_col::START_LAG_S)?;
//
//         let air_pressure: f64 = row.get(fluxes_col::AIR_PRESSURE)?;
//         let air_temperature: f64 = row.get(fluxes_col::AIR_TEMPERATURE)?;
//
//         let error_code_u16: u16 = row.get(fluxes_col::ERROR_CODE)?;
//         let error_code = ErrorMask::from_u16(error_code_u16);
//
//         let is_valid: bool = row.get(fluxes_col::IS_VALID)?;
//         // let main_gas_r2: f64 = row.get(fluxes_col::MAIN_GAS_R2)?;
//
//         // Compute derived times from start_time and offsets.
//         let close_time = start_time + TimeDelta::seconds(close_offset);
//         let open_time = start_time + TimeDelta::seconds(open_offset);
//         let end_time = start_time + TimeDelta::seconds(end_offset);
//
//         let gas_columns = vec![
//             (GasType::CH4, fluxes_col::CH4_FLUX),
//             (GasType::CO2, fluxes_col::CO2_FLUX),
//             (GasType::H2O, fluxes_col::H2O_FLUX),
//             (GasType::N2O, fluxes_col::N2O_FLUX),
//         ];
//
//         let manual_adjusted = row.get(fluxes_col::MANUAL_ADJUSTED)?;
//         let project_name = row.get(fluxes_col::PROJECT_ID)?;
//         let manual_valid: bool = row.get(fluxes_col::MANUAL_VALID)?;
//         let chamber_volume: f64 = row.get(fluxes_col::CHAMBER_VOLUME)?;
//         let mut override_valid = None;
//         if manual_valid {
//             override_valid = Some(is_valid);
//         }
//         let filtered: Vec<(GasType, usize)> =
//             gas_columns.into_iter().filter(|(gas, _)| gastypes.contains(gas)).collect();
//         // Initialize the HashMaps.
//         let mut flux = HashMap::new();
//         let mut calc_r2 = HashMap::new();
//         // let mut measurement_r2 = HashMap::new();
//         let mut slope_map = HashMap::new();
//         let mut calc_range_start_map = HashMap::new();
//         let mut calc_range_end_map = HashMap::new();
//         let mut calc_gas_v = HashMap::new();
//         let mut calc_dt_v = HashMap::new();
//         let mut measurement_dt_v = Vec::new();
//         let measurement_diag_v = Vec::new();
//         let mut measurement_gas_v = HashMap::new();
//         let measurement_range_start = close_time + TimeDelta::seconds(open_lag_s as i64);
//         let measurement_range_end = open_time + TimeDelta::seconds(open_lag_s as i64);
//         let mut dt_v = Vec::new();
//         let mut diag_v = Vec::new();
//         let mut gas_v = HashMap::new();
//         let mut min_y = HashMap::new();
//         let mut max_y = HashMap::new();
//         let mut measurement_r2 = HashMap::new();
//
//         if let Some(gas_data_day) = gas_data.get(&day) {
//             // --- Calculation & Measurement Filtering for Each Gas ---
//             for (gas, base_idx) in filtered {
//                 if let Some(g_values) = gas_data_day.gas.get(&gas) {
//                     // Here you extract per-gas values from the flux row.
//                     // (We assume that part of the code remains the same.)
//                     let gas_flux: f64 = row.get(base_idx).unwrap_or(0.0);
//                     let gas_r2: f64 = row.get(base_idx + 1)?;
//                     let gas_measurement_r2: f64 = row.get(base_idx + 2)?;
//                     let gas_slope =
//                         LinReg::from_val(row.get(base_idx + 3)?, row.get(base_idx + 4)?);
//                     let gas_calc_range_start: f64 = row.get(base_idx + 5)?;
//                     let gas_calc_range_end: f64 = row.get(base_idx + 6)?;
//
//                     flux.insert(gas, gas_flux);
//                     calc_r2.insert(gas, gas_r2);
//                     measurement_r2.insert(gas, gas_measurement_r2);
//                     slope_map.insert(gas, gas_slope);
//                     calc_range_start_map.insert(gas, gas_calc_range_start);
//                     calc_range_end_map.insert(gas, gas_calc_range_end);
//
//                     // Filter for calculation range using the per-gas calc range.
//                     let (calc_dt, calc_vals) = filter_data_in_range(
//                         &gas_data_day.datetime,
//                         g_values,
//                         calc_range_start_map.get(&gas).copied().unwrap_or(0.0),
//                         calc_range_end_map.get(&gas).copied().unwrap_or(0.0),
//                     );
//
//                     calc_dt_v.insert(gas, calc_dt);
//                     calc_gas_v.insert(gas, calc_vals);
//
//                     // Filter for measurement range using the cycle's measurement range.
//                     let (meas_dt, meas_vals) = filter_data_in_range(
//                         &gas_data_day.datetime,
//                         g_values,
//                         measurement_range_start.timestamp() as f64,
//                         measurement_range_end.timestamp() as f64,
//                     );
//                     if gas == main_gas {
//                         // For the main gas, assign the filtered datetime vector.
//                         measurement_dt_v = meas_dt;
//                     }
//                     measurement_gas_v.insert(gas, meas_vals);
//                 }
//             }
//
//             // --- Overall Cycle Data Filtering ---
//             // Filter diagnostic data (diag_v) and full datetime (dt_v) for the entire cycle:
//             let (dt_v_full, diag_v_full) = filter_diag_data(
//                 &gas_data_day.datetime,
//                 &gas_data_day.diag,
//                 start_time.timestamp() as f64 + start_lag_s,
//                 end_time.timestamp() as f64 + end_lag_s,
//             );
//             dt_v = dt_v_full; // Assign overall datetime vector.
//             diag_v = diag_v_full; // Assign overall diagnostic vector.
//             if diag_v.iter().sum::<i64>() != 0 {
//                 return Ok(None);
//             }
//             if dt_v.is_empty() {
//                 return Ok(None); // Use `None` to skip cycle
//             }
//             for &gas in &gastypes {
//                 if let Some(g_values) = gas_data_day.gas.get(&gas) {
//                     let (_full_dt, full_vals) = filter_data_in_range(
//                         &gas_data_day.datetime,
//                         g_values,
//                         start_time.timestamp() as f64,
//                         end_time.timestamp() as f64,
//                     );
//                     max_y.insert(gas, calculate_max_y_from_vec(&full_vals));
//                     min_y.insert(gas, calculate_min_y_from_vec(&full_vals));
//                     gas_v.insert(gas, full_vals);
//                     // Optionally, store full_dt in a dedicated dt map if needed.
//                 }
//             }
//         }
//         Ok(Some(Cycle {
//             instrument_model: InstrumentType::from_str(&instrument_model),
//             instrument_serial,
//             project_name,
//             manual_adjusted,
//             chamber_id,
//             min_calc_range: MIN_CALC_AREA_RANGE,
//             start_time,
//             calc_dt_v,
//             calc_gas_v,
//             diag_v,
//             dt_v,
//             gas_v,
//             max_y,
//             min_y,
//             measurement_dt_v,
//             measurement_gas_v,
//             measurement_diag_v,
//             close_time,
//             open_time,
//             end_time,
//             air_temperature,
//             air_pressure,
//             chamber_volume,
//             error_code,
//             is_valid,
//             override_valid,
//             manual_valid,
//             main_gas,
//             close_offset,
//             open_offset,
//             end_offset,
//             open_lag_s,
//             close_lag_s,
//             end_lag_s,
//             start_lag_s,
//             max_idx: 0.0, // Default value.
//             gases: gastypes,
//             calc_range_start: calc_range_start_map,
//             calc_range_end: calc_range_end_map,
//             // The following fields were not stored; use defaults.
//             measurement_range_start: (start_time
//                 + TimeDelta::seconds(start_lag_s as i64)
//                 + TimeDelta::seconds(close_offset)
//                 + TimeDelta::seconds(open_lag_s as i64))
//             .timestamp() as f64,
//             measurement_range_end: (start_time
//                 + TimeDelta::seconds(end_lag_s as i64)
//                 + TimeDelta::seconds(close_offset)
//                 + TimeDelta::seconds(open_lag_s as i64))
//             .timestamp() as f64,
//             linfit: slope_map,
//             flux,
//             measurement_r2,
//             calc_r2,
//             // Other fields (dt_v, calc_dt_v, etc.) can be initialized as needed.
//         }))
//     })?;
//
//     let cycles: Vec<Cycle> =
//         cycle_iter.collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect();
//     if cycles.is_empty() {
//         return Err(rusqlite::Error::QueryReturnedNoRows);
//     }
//     Ok(cycles)
// }
fn filter_data_in_range(
    datetimes: &[DateTime<Utc>],
    values: &[Option<f64>],
    range_start: f64,
    range_end: f64,
) -> (Vec<DateTime<Utc>>, Vec<Option<f64>>) {
    // Zip the datetimes and values, filter by comparing each datetime's timestamp
    // to the given range, and then unzip the filtered pairs.
    datetimes
        .iter()
        .zip(values.iter())
        .filter(|(dt, _)| {
            let t = dt.timestamp() as f64;
            t >= range_start && t <= range_end
        })
        .map(|(dt, &v)| (dt, v))
        .unzip()
}
fn filter_diag_data(
    datetimes: &[DateTime<Utc>],
    diag: &[i64],
    range_start: f64,
    range_end: f64,
) -> (Vec<DateTime<Utc>>, Vec<i64>) {
    datetimes
        .iter()
        .zip(diag.iter())
        .filter(|(dt, _)| {
            let t = dt.timestamp() as f64;
            t >= range_start && t <= range_end
        })
        .map(|(dt, &d)| (dt, d))
        .unzip()
}

fn find_best_window_for_gas(
    dt_v: &[f64],
    gas_v: &[Option<f64>],
    gaps: &[bool],
    min_window: usize,
    step: usize,
) -> Option<(usize, usize, f64, Vec<Option<f64>>)> {
    let max_len = gas_v.len();
    let mut best_r2 = f64::MIN;
    let mut best_range = (0, 0);
    let mut best_y = Vec::new();

    for win_size in (min_window..=max_len).step_by(step) {
        for start in (0..=max_len.saturating_sub(win_size)).step_by(step) {
            let end = start + win_size;

            if end - start < min_window {
                continue;
            }

            // Skip window if it has timestamp gaps
            if gaps[start..end.saturating_sub(1)].iter().any(|&gap| gap) {
                continue;
            }

            let window_dt = &dt_v[start..end];
            let window_gas = &gas_v[start..end];

            // Pair and filter out None
            let valid_pairs: Vec<(f64, f64)> = window_dt
                .iter()
                .zip(window_gas.iter())
                .filter_map(|(&x, &y)| y.map(|val| (x, val)))
                .collect();

            if valid_pairs.len() < min_window {
                continue;
            }

            let (x_vals, y_vals): (Vec<f64>, Vec<f64>) = valid_pairs.into_iter().unzip();
            let r2 = stats::pearson_correlation(&x_vals, &y_vals).unwrap_or(0.0).powi(2);

            if r2 > best_r2 {
                best_r2 = r2;
                best_range = (start, end);
                best_y = window_gas.to_vec(); // preserves None values
            }
        }
    }

    if best_range.1 == 0 {
        None
    } else {
        Some((best_range.0, best_range.1, best_r2, best_y))
    }
}

pub fn calculate_max_y_from_vec(values: &[Option<f64>]) -> f64 {
    values.iter().filter_map(|&v| v).filter(|v| !v.is_nan()).fold(f64::NEG_INFINITY, f64::max)
}

pub fn calculate_min_y_from_vec(values: &[Option<f64>]) -> f64 {
    values.iter().filter_map(|&v| v).filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min)
}
