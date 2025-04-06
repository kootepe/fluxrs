use crate::errorcode::{ErrorCode, ErrorMask};
use crate::instruments::GasType;
use crate::instruments::{get_instrument_by_model, InstrumentType};
use crate::query::{calculate_max_y_from_vec, calculate_min_y_from_vec};
use crate::query_gas;
use crate::stats;
use chrono::{DateTime, TimeDelta, Utc};
use rusqlite::{params, Connection, Error, Result};

use std::collections::{HashMap, HashSet};
use std::fmt;

// the window of max r must be at least 240 seconds
pub const MIN_WINDOW_SIZE: usize = 180;
// how many seconds to increment the moving window searching for max r
pub const WINDOW_INCREMENT: usize = 5;

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
        // let len: usize = self.measurement_dt_v.len();
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
        if let Some(values) = self.gas_v.get(&self.main_gas) {
            let valid_count = values.iter().filter(|v| !v.is_nan()).count();
            let threshold = self.end_offset as f64 * 0.7;
            let check = (valid_count as f64) < threshold;

            if check {
                self.add_error(ErrorCode::TooFewMeasurements);
            } else {
                self.remove_error(ErrorCode::TooFewMeasurements);
            }
        } else {
            // Handle the missing key case however you want
            self.add_error(ErrorCode::TooFewMeasurements);
        }
    }
    // pub fn check_missing(&mut self) {
    //     if let Some(values) = self.gas_v.get(&self.main_gas) {
    //         let valid_count = values.iter().filter(|v| v.is_some()).count();
    //         let check = self.end_offset as f64 * 0.7 > valid_count as f64;
    //
    //         if check {
    //             self.add_error(ErrorCode::TooFewMeasurements);
    //         } else {
    //             self.remove_error(ErrorCode::TooFewMeasurements);
    //         }
    //     } else {
    //         // Optional: handle case where main_gas is missing
    //         self.add_error(ErrorCode::TooFewMeasurements);
    //     }
    // }
    // pub fn check_missing(&mut self) {
    //     let valid_count =
    //         self.gas_v.get(&self.main_gas).unwrap().iter().filter(|v| v.is_some()).count();
    //     let check = self.end_offset as f64 * 0.7 > valid_count as f64;
    //     if check {
    //         self.add_error(ErrorCode::TooFewMeasurements);
    //     } else {
    //         self.remove_error(ErrorCode::TooFewMeasurements)
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

    pub fn recalculate_fluxes(&mut self) {
        for &gas in &self.gases.clone() {
            self.recalculate_flux(gas);
        }
    }
    pub fn recalculate_flux(&mut self, gas_type: GasType) {
        let mol_mass = gas_type.mol_mass();
        let slope_ppm = self.slope.get(&gas_type).unwrap() * gas_type.conv_factor();
        let slope_ppm_hour = slope_ppm * 60. * 60.;
        let flux = slope_ppm_hour / 1_000_000.0
            * self.chamber_volume
            * ((mol_mass * (self.air_pressure * 1000.0))
                / (8.314 * (self.air_temperature + 273.15)))
            * 1000.0;

        self.flux.insert(gas_type, flux);
    }
    pub fn calculate_fluxes(&mut self) {
        for &gas in &self.gases.clone() {
            self.calculate_flux(gas);
        }
    }

    pub fn calculate_flux(&mut self, gas_type: GasType) {
        self.calculate_slope(gas_type);
        let mol_mass = gas_type.mol_mass();
        let slope_ppm = self.slope.get(&gas_type).unwrap() * gas_type.conv_factor();
        let slope_ppm_hour = slope_ppm * 60. * 60.;
        let flux = slope_ppm_hour / 1_000_000.0
            * self.chamber_volume
            * ((mol_mass * (self.air_pressure * 1000.0))
                / (8.314 * (self.air_temperature + 273.15)))
            * 1000.0;

        self.flux.insert(gas_type, flux);
    }
    pub fn update_cycle(&mut self, _project: String) {
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

pub fn insert_flux(conn: &Connection, cycle: &Cycle) -> Result<()> {
    // Compute/collect the values that are common to both queries.
    let start_timestamp = cycle.start_time.timestamp();
    let close_offset = cycle.close_offset;
    let open_offset = cycle.open_offset;
    let end_offset = cycle.end_offset;
    let lag_s = cycle.lag_s as i64;

    // Gas values (defaulting to 0.0 if absent).
    let ch4_flux = cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0);
    let ch4_r2 = cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(0.0);
    let ch4_measurement_r2 = cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(0.0);
    let ch4_slope = cycle.slope.get(&GasType::CH4).copied().unwrap_or(0.0);

    let co2_flux = cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0);
    let co2_r2 = cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(0.0);
    let co2_measurement_r2 = cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(0.0);
    let co2_slope = cycle.slope.get(&GasType::CO2).copied().unwrap_or(0.0);

    let h2o_flux = cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0);
    let h2o_r2 = cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(0.0);
    let h2o_measurement_r2 = cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(0.0);
    let h2o_slope = cycle.slope.get(&GasType::H2O).copied().unwrap_or(0.0);

    let n2o_flux = cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0);
    let n2o_r2 = cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(0.0);
    let n2o_measurement_r2 = cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(0.0);
    let n2o_slope = cycle.slope.get(&GasType::N2O).copied().unwrap_or(0.0);

    // r value for the main gas.
    let main_gas_r2 = cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(0.0);

    // Convert error code.
    let error_code = cycle.error_code.0;
    let chamber_volume = cycle.chamber_volume;

    // First, attempt an INSERT OR IGNORE.
    conn.execute(
        "INSERT OR IGNORE INTO fluxes (
            instrument_model, instrument_serial, chamber_id, start_time, close_offset,
            open_offset, end_offset, lag_s, air_pressure, air_temperature, error_code,
            is_valid, main_gas_r2, ch4_flux, ch4_r2, ch4_measurement_r2, ch4_slope, co2_flux, co2_r2, co2_measurement_r2, co2_slope,
            h2o_flux, h2o_r2, h2o_measurement_r2, h2o_slope, n2o_flux, n2o_r2, n2o_measurement_r2, n2o_slope,
            chamber_volume
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                  ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30)",
        params![
            cycle.instrument_model.to_string(),
            cycle.instrument_serial,
            cycle.chamber_id,
            start_timestamp,
            close_offset,
            open_offset,
            end_offset,
            lag_s,
            cycle.air_pressure,
            cycle.air_temperature,
            error_code,
            cycle.is_valid,
            main_gas_r2,
            ch4_flux,
            ch4_r2,
            ch4_measurement_r2,
            ch4_slope,
            co2_flux,
            co2_r2,
            co2_measurement_r2,
            co2_slope,
            h2o_flux,
            h2o_r2,
            h2o_measurement_r2,
            h2o_slope,
            n2o_flux,
            n2o_r2,
            n2o_measurement_r2,
            n2o_slope,
            chamber_volume,
        ],
    )?;

    // Then, update the row (even if it was just inserted) with the latest values.
    // We use instrument_serial and start_time as the unique keys to identify the record.
    conn.execute(
        "UPDATE fluxes SET
            instrument_model = ?1,
            chamber_id = ?2,
            close_offset = ?3,
            open_offset = ?4,
            end_offset = ?5,
            lag_s = ?6,
            air_pressure = ?7,
            air_temperature = ?8,
            error_code = ?9,
            is_valid = ?10,
            main_gas_r2 = ?11,
            ch4_flux = ?12,
            ch4_r2 = ?13,
            ch4_measurement_r2 = ?14,
            ch4_slope = ?15,
            co2_flux = ?16,
            co2_r2 = ?17,
            co2_measurement_r2 = ?18,
            co2_slope = ?19,
            h2o_flux = ?20,
            h2o_r2 = ?21,
            h2o_measurement_r2 = ?22,
            h2o_slope = ?23,
            n2o_flux = ?24,
            n2o_r2 = ?25,
            n2o_measurement_r2 = ?26,
            n2o_slope = ?27,
            chamber_volume = ?28
         WHERE instrument_serial = ?29 AND start_time = ?30",
        params![
            cycle.instrument_model.to_string(),
            cycle.chamber_id,
            close_offset,
            open_offset,
            end_offset,
            lag_s,
            cycle.air_pressure,
            cycle.air_temperature,
            error_code,
            cycle.is_valid,
            main_gas_r2,
            ch4_flux,
            ch4_r2,
            ch4_measurement_r2,
            ch4_slope,
            co2_flux,
            co2_r2,
            co2_measurement_r2,
            co2_slope,
            h2o_flux,
            h2o_r2,
            h2o_measurement_r2,
            h2o_slope,
            n2o_flux,
            n2o_r2,
            n2o_measurement_r2,
            n2o_slope,
            chamber_volume,
            cycle.instrument_serial,
            start_timestamp,
        ],
    )?;

    Ok(())
}
pub fn insert_fluxes_ignore_duplicates(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: String,
) -> Result<usize> {
    let mut inserted = 0;
    let tx = conn.transaction()?; // Start transaction for bulk insertion

    {
        let mut insert_stmt = tx.prepare(
            "INSERT OR IGNORE INTO fluxes (
            instrument_model, instrument_serial, chamber_id, main_gas, start_time,
            close_offset, open_offset, end_offset, lag_s, air_pressure, air_temperature,
            error_code, is_valid, main_gas_r2, ch4_flux, ch4_r2, ch4_measurement_r2, ch4_slope,
            ch4_calc_range_start, ch4_calc_range_end, co2_flux, co2_r2, co2_measurement_r2, co2_slope,
            co2_calc_range_start, co2_calc_range_end, h2o_flux, h2o_r2, h2o_measurement_r2, h2o_slope,
            h2o_calc_range_start, h2o_calc_range_end, n2o_flux, n2o_r2, n2o_measurement_r2, n2o_slope,
            n2o_calc_range_start, n2o_calc_range_end, project_id, manual_adjusted, manual_valid, chamber_volume
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                  ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
                  ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42)",
        )?;
        for cycle in cycles {
            execute_insert(&mut insert_stmt, cycle, &project)?;
            inserted += 1;
        }
    }

    tx.commit()?;
    Ok(inserted)
}
pub fn update_fluxes(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tx = conn.transaction()?; // Start transaction for consistency
    {
        let mut update_stmt = tx.prepare(
            "UPDATE fluxes SET
            instrument_model = ?1, chamber_id = ?2, main_gas = ?3, close_offset = ?4,
            open_offset = ?5, end_offset = ?6, lag_s = ?7, air_pressure = ?8,
            air_temperature = ?9, error_code = ?10, is_valid = ?11, main_gas_r2 = ?12,
            ch4_flux = ?13, ch4_r2 = ?14, ch4_measurement_r2 = ?15, ch4_slope = ?16,
            ch4_calc_range_start = ?17, ch4_calc_range_end = ?18, co2_flux = ?19,
            co2_r2 = ?20, co2_measurement_r2 = ?21, co2_slope = ?22, co2_calc_range_start = ?23,
            co2_calc_range_end = ?24, h2o_flux = ?25, h2o_r2 = ?26, h2o_measurement_r2 = ?27,
            h2o_slope = ?28, h2o_calc_range_start = ?29, h2o_calc_range_end = ?30, n2o_flux = ?31,
            n2o_r2 = ?32, n2o_measurement_r2 = ?33, n2o_slope = ?34, n2o_calc_range_start = ?35,
            n2o_calc_range_end = ?36, project_id = ?37, manual_adjusted = ?38, manual_valid = ?39, chamber_volume = ?40
         WHERE instrument_serial = ?41 AND start_time = ?42",
        )?;

        for cycle in cycles {
            match execute_update(&mut update_stmt, cycle, &project)
            // execute_update(
            //     &mut conn,
            //     &[self.cycles[*self.index].clone()],
            //     self.selected_project.as_ref().unwrap().clone(),
             {
                Ok(_) => println!("Fluxes updated successfully!"),
                Err(e) => eprintln!("Error inserting fluxes: {}", e),
            }
        }
    }
    tx.commit()?;
    Ok(())
}
fn execute_insert(stmt: &mut rusqlite::Statement, cycle: &Cycle, project: &String) -> Result<()> {
    stmt.execute(params![
        cycle.instrument_model.to_string(),
        cycle.instrument_serial,
        cycle.chamber_id,
        cycle.main_gas.column_name(),
        cycle.start_time.timestamp(),
        cycle.close_offset,
        cycle.open_offset,
        cycle.end_offset,
        cycle.lag_s as i64,
        cycle.air_pressure,
        cycle.air_temperature,
        cycle.error_code.0,
        cycle.is_valid,
        cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
        cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::N2O).copied().unwrap_or(0.0),
        project,
        cycle.manual_adjusted,
        cycle.manual_valid,
        cycle.chamber_volume,
    ])?;
    Ok(())
}
fn execute_update(stmt: &mut rusqlite::Statement, cycle: &Cycle, project: &String) -> Result<()> {
    println!("{}", cycle.lag_s);
    stmt.execute(params![
        cycle.instrument_model.to_string(),
        cycle.chamber_id,
        cycle.main_gas.column_name(),
        cycle.close_offset,
        cycle.open_offset,
        cycle.end_offset,
        cycle.lag_s as i64,
        cycle.air_pressure,
        cycle.air_temperature,
        cycle.error_code.0,
        cycle.is_valid,
        cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
        cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::N2O).copied().unwrap_or(0.0),
        project,
        cycle.manual_adjusted,
        cycle.manual_valid,
        cycle.chamber_volume,
        cycle.instrument_serial,
        cycle.start_time.timestamp(),
    ])?;
    Ok(())
}
/// Loads cycles from the fluxes table. The SELECT order must match the INSERT order:
/// 0: instrument_model
/// 1: instrument_serial
/// 2: chamber_id
/// 3: main_gas (stored as a string)
/// 4: start_time (Unix timestamp)
/// 5: close_offset
/// 6: open_offset
/// 7: end_offset
/// 8: lag_s (stored as integer)
/// 9: air_pressure
/// 10: air_temperature
/// 11: error_code (u16)
/// 12: is_valid (bool)
/// 13: main_gas_r2
/// 14: ch4_flux
/// 15: ch4_r
/// 16: ch4_slope
/// 17: ch4_calc_range_start
/// 18: ch4_calc_range_end
/// 19: co2_flux
/// 20: co2_r
/// 21: co2_slope
/// 22: co2_calc_range_start
/// 23: co2_calc_range_end
/// 24: h2o_flux
/// 25: h2o_r
/// 26: h2o_slope
/// 27: h2o_calc_range_start
/// 28: h2o_calc_range_end
/// 29: n2o_flux
/// 30: n2o_r
/// 31: n2o_slope
/// 32: n2o_calc_range_start
/// 33: n2o_calc_range_end
pub fn load_fluxes(
    conn: &mut Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
    instrument_serial: String,
    only_flux_keys: Option<&HashSet<(String, i64)>>, // <-- optional filter
) -> Result<Vec<Cycle>> {
    let ch4_col = 14;
    let co2_col = 20;
    let h2o_col = 26;
    let n2o_col = 32;
    let mut stmt = conn.prepare(
        "SELECT instrument_model,
                instrument_serial,
                start_time,
                chamber_id,
                main_gas,
                close_offset,
                open_offset,
                end_offset,
                lag_s,
                air_pressure,
                air_temperature,
                error_code,
                is_valid,
                main_gas_r2,
                ch4_flux,
                ch4_r2,
                ch4_measurement_r2,
                ch4_slope,
                ch4_calc_range_start,
                ch4_calc_range_end,
                co2_flux,
                co2_r2,
                co2_measurement_r2,
                co2_slope,
                co2_calc_range_start,
                co2_calc_range_end,
                h2o_flux,
                h2o_r2,
                h2o_measurement_r2,
                h2o_slope,
                h2o_calc_range_start,
                h2o_calc_range_end,
                n2o_flux,
                n2o_r2,
                n2o_measurement_r2,
                n2o_slope,
                n2o_calc_range_start,
                n2o_calc_range_end,
                manual_adjusted,
                project_id,
                manual_valid,
                chamber_volume
         FROM fluxes WHERE start_time BETWEEN ?1 AND ?2 and project_id = ?3
         ORDER BY start_time",
    )?;
    let gas_data = query_gas(conn, start, end, project.clone(), instrument_serial)?;

    let s = start.timestamp();
    let e = end.timestamp();
    let cycle_iter = stmt.query_map(params![s, e, project.clone()], |row| {
        // Basic fields
        let instrument_model: String = row.get(0)?;
        let instrument_serial: String = row.get(1)?;
        let start_timestamp: i64 = row.get(2)?;
        let chamber_id: String = row.get(3)?;
        let flux_key = (chamber_id.clone(), start_timestamp);

        if let Some(filter_set) = only_flux_keys {
            if !filter_set.contains(&flux_key) {
                return Ok(None); // skip flux
            }
        }

        let gases = get_instrument_by_model(&instrument_model).unwrap().base.gas_cols;
        let gastypes: Vec<GasType> =
            gases.iter().filter_map(|name| GasType::from_str(name)).collect();

        let main_gas_str: String = row.get(4)?;
        let main_gas = GasType::from_str(&main_gas_str).unwrap_or(GasType::CH4);

        let start_time = chrono::DateTime::from_timestamp(start_timestamp, 0).unwrap();

        let day = start_time.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD

        let close_offset: i64 = row.get(5)?;
        let open_offset: i64 = row.get(6)?;
        let end_offset: i64 = row.get(7)?;
        let lag_s: i64 = row.get(8)?;
        let lag_s = lag_s as f64; // convert back to f64

        let air_pressure: f64 = row.get(9)?;
        let air_temperature: f64 = row.get(10)?;

        let error_code_u16: u16 = row.get(11)?;
        let error_code = ErrorMask::from_u16(error_code_u16);

        let is_valid: bool = row.get(12)?;
        let main_gas_r2: f64 = row.get(13)?;

        // Compute derived times from start_time and offsets.
        let close_time = start_time + TimeDelta::seconds(close_offset);
        let open_time = start_time + TimeDelta::seconds(open_offset);
        let end_time = start_time + TimeDelta::seconds(end_offset);

        let gas_columns = vec![
            (GasType::CH4, ch4_col),
            (GasType::CO2, co2_col),
            (GasType::H2O, h2o_col),
            (GasType::N2O, n2o_col),
        ];

        let manual_adjusted = row.get(38)?;
        let project_name = row.get(39)?;
        let manual_valid: bool = row.get(40)?;
        let chamber_volume: f64 = row.get(41)?;
        let mut override_valid = None;
        if manual_valid {
            override_valid = Some(is_valid);
        }
        let filtered: Vec<(GasType, usize)> =
            gas_columns.into_iter().filter(|(gas, _)| gastypes.contains(gas)).collect();
        // Initialize the HashMaps.
        let mut flux = HashMap::new();
        let mut calc_r2 = HashMap::new();
        // let mut measurement_r2 = HashMap::new();
        let mut slope_map = HashMap::new();
        let mut calc_range_start_map = HashMap::new();
        let mut calc_range_end_map = HashMap::new();
        let mut calc_gas_v = HashMap::new();
        let mut calc_dt_v = HashMap::new();
        let mut measurement_dt_v = Vec::new();
        let mut measurement_diag_v = Vec::new();
        let mut measurement_gas_v = HashMap::new();
        let measurement_range_start = close_time + TimeDelta::seconds(lag_s as i64);
        let measurement_range_end = open_time + TimeDelta::seconds(lag_s as i64);
        let mut dt_v = Vec::new();
        let mut diag_v = Vec::new();
        let mut gas_v = HashMap::new();
        let mut min_y = HashMap::new();
        let mut max_y = HashMap::new();
        let mut measurement_r2 = HashMap::new();

        if let Some(gas_data_day) = gas_data.get(&day) {
            // --- Calculation & Measurement Filtering for Each Gas ---
            for (gas, base_idx) in filtered {
                if let Some(g_values) = gas_data_day.gas.get(&gas) {
                    // Here you extract per-gas values from the flux row.
                    // (We assume that part of the code remains the same.)
                    let gas_flux: f64 = row.get(base_idx).unwrap_or(0.0);
                    let gas_r2: f64 = row.get(base_idx + 1)?;
                    let gas_measurement_r2: f64 = row.get(base_idx + 2)?;
                    let gas_slope: f64 = row.get(base_idx + 3)?;
                    let gas_calc_range_start: f64 = row.get(base_idx + 4)?;
                    let gas_calc_range_end: f64 = row.get(base_idx + 5)?;

                    flux.insert(gas, gas_flux);
                    calc_r2.insert(gas, gas_r2);
                    measurement_r2.insert(gas, gas_measurement_r2);
                    slope_map.insert(gas, gas_slope);
                    calc_range_start_map.insert(gas, gas_calc_range_start);
                    calc_range_end_map.insert(gas, gas_calc_range_end);

                    // Filter for calculation range using the per-gas calc range.
                    let (calc_dt, calc_vals) = filter_data_in_range(
                        &gas_data_day.datetime,
                        g_values,
                        calc_range_start_map.get(&gas).copied().unwrap_or(0.0),
                        calc_range_end_map.get(&gas).copied().unwrap_or(0.0),
                    );

                    calc_dt_v.insert(gas, calc_dt);
                    calc_gas_v.insert(gas, calc_vals);

                    // Filter for measurement range using the cycle's measurement range.
                    let (meas_dt, meas_vals) = filter_data_in_range(
                        &gas_data_day.datetime,
                        g_values,
                        measurement_range_start.timestamp() as f64,
                        measurement_range_end.timestamp() as f64,
                    );
                    // let timestamps: Vec<f64> =
                    //     meas_dt.iter().map(|x| x.timestamp() as f64).collect();
                    // measurement_r2.insert(
                    //     gas,
                    //     stats::pearson_correlation(&timestamps[..], &meas_vals).unwrap_or(0.0),
                    // );
                    // measurement_r2.insert(
                    //     &gas,
                    //     stats::pearson_correlation(
                    //         &meas_dt.iter().map(|x| x.timestamp() as f64).collect(),
                    //         &meas_vals,
                    //     )
                    //     .unwrap_or(0.0),
                    // );
                    if gas == main_gas {
                        // For the main gas, assign the filtered datetime vector.
                        measurement_dt_v = meas_dt;
                    }
                    measurement_gas_v.insert(gas, meas_vals);
                }
            }

            // --- Overall Cycle Data Filtering ---
            // Filter diagnostic data (diag_v) and full datetime (dt_v) for the entire cycle:
            let (dt_v_full, diag_v_full) = filter_diag_data(
                &gas_data_day.datetime,
                &gas_data_day.diag,
                start_time.timestamp() as f64,
                end_time.timestamp() as f64,
            );
            dt_v = dt_v_full; // Assign overall datetime vector.
            diag_v = diag_v_full; // Assign overall diagnostic vector.
            if dt_v.is_empty() {
                return Ok(None); // Use `None` to skip cycle
            }
            for &gas in &gastypes {
                if let Some(g_values) = gas_data_day.gas.get(&gas) {
                    let (_full_dt, full_vals) = filter_data_in_range(
                        &gas_data_day.datetime,
                        g_values,
                        start_time.timestamp() as f64,
                        end_time.timestamp() as f64,
                    );
                    max_y.insert(gas, calculate_max_y_from_vec(&full_vals));
                    min_y.insert(gas, calculate_min_y_from_vec(&full_vals));
                    gas_v.insert(gas, full_vals);
                    // Optionally, store full_dt in a dedicated dt map if needed.
                }
            }
        }
        Ok(Some(Cycle {
            instrument_model: InstrumentType::from_str(&instrument_model),
            instrument_serial,
            project_name,
            manual_adjusted,
            chamber_id,
            start_time,
            calc_dt_v,
            calc_gas_v,
            diag_v,
            dt_v,
            gas_v,
            max_y,
            min_y,
            measurement_dt_v,
            measurement_gas_v,
            measurement_diag_v,
            close_time,
            open_time,
            end_time,
            air_temperature,
            air_pressure,
            chamber_volume,
            error_code,
            is_valid,
            override_valid,
            manual_valid,
            main_gas,
            close_offset,
            open_offset,
            end_offset,
            lag_s,
            max_idx: 0.0, // Default value.
            gases: gastypes,
            calc_range_start: calc_range_start_map,
            calc_range_end: calc_range_end_map,
            // The following fields were not stored; use defaults.
            measurement_range_start: (start_time
                + TimeDelta::seconds(close_offset)
                + TimeDelta::seconds(lag_s as i64))
            .timestamp() as f64,
            measurement_range_end: (start_time
                + TimeDelta::seconds(close_offset)
                + TimeDelta::seconds(lag_s as i64))
            .timestamp() as f64,
            slope: slope_map,
            flux,
            measurement_r2,
            calc_r2,
            // Other fields (dt_v, calc_dt_v, etc.) can be initialized as needed.
        }))
    })?;

    // let cycles: Vec<Cycle> = cycle_iter.filter_map(|res| res.ok().flatten()).collect();
    // let cycles: Result<Vec<Cycle>> = cycle_iter.collect();
    // let cycles: Vec<Cycle> =
    //     cycle_iter.collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect();
    let cycles: Vec<Cycle> =
        cycle_iter.collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect();
    if cycles.is_empty() {
        // return Err("No cycles found".into());
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(cycles)
}
fn filter_data_in_range(
    datetimes: &[DateTime<Utc>],
    values: &[f64],
    range_start: f64,
    range_end: f64,
) -> (Vec<DateTime<Utc>>, Vec<f64>) {
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
