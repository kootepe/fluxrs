use chrono::prelude::DateTime;
use chrono::Utc;
use std::{thread, time};

use csv::StringRecord;
use std::error::Error;

use crate::gas_plot;
use crate::instruments::{Gas, GasType};
use crate::stats;

pub const ERROR_INT: i64 = -9999;
pub const ERROR_FLOAT: f64 = -9999.;
// the window of max r must be at least 240 seconds
pub const MIN_WINDOW_SIZE: usize = 240;
// how many seconds to increment the moving window searching for max r
pub const WINDOW_INCREMENT: usize = 5;

pub trait EqualLen {
    fn validate_lengths(&self) -> bool;
}
#[derive(Debug, Clone)]
pub enum RValue {
    CH4(f64),
    CO2(f64),
    H2O(f64),
    N2O(f64),
}

pub struct CycleBuilder {
    chamber_id: Option<String>,
    start_time: Option<DateTime<Utc>>,
    close_offset: Option<i64>,
    open_offset: Option<i64>,
    end_offset: Option<i64>,
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

    /// Build the Cycle struct
    pub fn build(self) -> Result<Cycle, &'static str> {
        let start = self.start_time.ok_or("Start time is required")?;
        let chamber = self.chamber_id.ok_or("Chamber ID is required")?;
        let close = self.close_offset.ok_or("Close offset is required")?;
        let open = self.open_offset.ok_or("Open offset is required")?;
        let end = self.end_offset.ok_or("End offset is required")?;

        Ok(Cycle {
            chamber_id: chamber,
            start_time: start,
            close_time: start + chrono::Duration::seconds(close),
            open_time: start + chrono::Duration::seconds(open),
            end_time: start + chrono::Duration::seconds(end),
            close_offset: close,
            open_offset: open,
            end_offset: end,
            calc_range_end: (start + chrono::Duration::seconds(open)).timestamp() as f64,
            calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            lag_s: 0.,
            max_idx: 0.,
            flux: 0.,
            r: 0.,
            r_values: Vec::new(),
            measurement_r_values: Vec::new(),
            calc_r: 0.,
            measurement_r: 0.,
            diag_v: Vec::new(),
            dt_v: Vec::new(),
            gas_data: Vec::new(),
            measurement_gas_data: Vec::new(),
            calc_gas_data: Vec::new(),
            calc_gas_v: Vec::new(),
            calc_dt_v: Vec::new(),
            measurement_gas_v: Vec::new(),
            measurement_dt_v: Vec::new(),
        })
    }
}
pub struct Cycle {
    pub chamber_id: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub close_time: chrono::DateTime<chrono::Utc>,
    pub open_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub close_offset: i64,
    pub open_offset: i64,
    pub end_offset: i64,
    pub lag_s: f64,
    pub max_idx: f64,
    pub calc_range_start: f64,
    pub calc_range_end: f64,
    pub r: f64,
    pub r_values: Vec<RValue>, // Store all R values in a struct
    pub measurement_r_values: Vec<RValue>,
    pub calc_r: f64,
    pub measurement_r: f64,
    pub flux: f64,
    pub dt_v: Vec<chrono::DateTime<chrono::Utc>>,
    pub gas_data: Vec<Gas>,
    pub measurement_gas_data: Vec<Gas>,
    pub calc_gas_data: Vec<Gas>,
    pub diag_v: Vec<i64>,
    pub calc_gas_v: Vec<f64>,
    pub calc_dt_v: Vec<chrono::DateTime<chrono::Utc>>,
    pub measurement_gas_v: Vec<f64>,
    pub measurement_dt_v: Vec<chrono::DateTime<chrono::Utc>>,
}

#[allow(clippy::needless_lifetimes)]
// #[allow(needless_lifetimes)]
impl Cycle {
    pub fn summary(&self) {
        println!(
            "dt: {} len: {}, gas len: {}",
            self.start_time,
            self.diag_v.len(),
            self.gas_data.len()
        );
    }
    pub fn add_r_value(&mut self, r_value: RValue) {
        self.r_values.push(r_value);
    }

    pub fn get_r_value(&self, gas_type: GasType) -> Option<f64> {
        self.r_values.iter().find_map(|r| match (r, gas_type) {
            (RValue::CH4(val), GasType::CH4) => Some(*val),
            (RValue::CO2(val), GasType::CO2) => Some(*val),
            (RValue::H2O(val), GasType::H2O) => Some(*val),
            (RValue::N2O(val), GasType::N2O) => Some(*val),
            _ => None,
        })
    }
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
    pub fn dt_v_as_float(&self) -> Vec<f64> {
        self.dt_v.iter().map(|x| x.timestamp() as f64).collect()
    }
    pub fn get_peak_datetime(&mut self) -> Option<DateTime<Utc>> {
        // Find the index of the highest gas value in the last 120 elements
        // let ch4_values =
        let ch4_values: Vec<f64> = self
            .gas_data
            .iter()
            .flat_map(|gas| match gas {
                Gas::CH4(values) => values.clone(),
                _ => vec![], // Ignore non-CH4 gas types
            })
            .collect();

        let len = ch4_values.len();
        if len < 120 {
            return None; // Return None if there aren't 120 elements
        }

        // NOTE: maybe look around the lag adjusted open time?
        // right now just looks for max in the last 240 secs
        let start_index = len.saturating_sub(240); // Get the start index for the last 240 elements

        let max_idx = ch4_values[start_index..] // Take the last x elements
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| start_index + idx); // Adjust index to original vector

        if let Some(idx) = max_idx {
            if let Some(peak_time) = self.dt_v.get(idx).cloned() {
                let max_idxx =
                    (self.start_time + chrono::TimeDelta::seconds(idx as i64)).timestamp() as f64;
                self.lag_s = max_idxx - (self.open_time.timestamp() as f64);
                // self.open_time = self.start_time
                //     + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
                return Some(peak_time);
            }
        }

        None
    }

    pub fn check_diag(&mut self) -> bool {
        self.diag_v.iter().sum::<i64>() != 0
    }

    pub fn adjust_open_time(&mut self) {
        self.open_time = self.start_time
            + chrono::TimeDelta::seconds(self.open_offset)
            + chrono::TimeDelta::seconds(self.lag_s as i64)
    }

    // pub fn calculate_measurement_r(&mut self) {
    //     self.close_time =
    //         self.start_time + chrono::TimeDelta::seconds(self.close_offset + self.lag_s as i64);
    //     self.open_time =
    //         self.start_time + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
    //     let s = self.close_time;
    //     let e = self.end_time;
    // }

    pub fn calculate_calc_r(&mut self) {
        let s = self.calc_range_start;
        let e = self.calc_range_end;
        let mut dt_v: Vec<_> = Vec::new();
        let mut gas_v: Vec<_> = Vec::new();

        self.dt_v
            .iter()
            .zip(self.calc_gas_data.iter()) // Pair timestamps with gas values
            .filter(|(t, _)| (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e) // Filter by time range
            .for_each(|(t, d)| {
                dt_v.push(*t);

                match d {
                    Gas::CH4(values) => {
                        let ch4_clone = values.clone();
                        self.gas_data.push(Gas::CH4(ch4_clone));
                    }
                    Gas::CO2(values) => {
                        let co2_clone = values.clone();
                        self.gas_data.push(Gas::CO2(co2_clone));
                    }
                    Gas::H2O(values) => {
                        let h2o_clone = values.clone();
                        self.gas_data.push(Gas::H2O(h2o_clone));
                    }
                    Gas::N2O(values) => {
                        let n2o_clone = values.clone();
                        self.gas_data.push(Gas::N2O(n2o_clone));
                    }
                }
            });
        // let dt_vv = dt_v.iter().map(|x| x.timestamp() as f64).collect();
        let dt_vv: Vec<f64> = dt_v.iter().map(|x| x.timestamp() as f64).collect();
        let gas_vv = gas_v;
        self.calc_r = stats::pearson_correlation(&dt_vv, &gas_vv).unwrap_or(0.0);
    }
    pub fn find_highest_r_window_disp(&mut self) {
        // println!("Finding highest r");
        if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
            println!("Short data");
            return;
        }

        let max_window = self.measurement_gas_v.len();
        let mut max_r = f64::MIN;

        let mut start_idx = 0;
        let mut end_idx = 0;

        let dt_v: Vec<f64> = self
            .measurement_dt_v
            .iter()
            .map(|dt| dt.timestamp() as f64)
            .collect();
        let gas_v = &self.measurement_gas_v;
        let mut best_window_y = Vec::new();

        for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
            for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
                let end = start + win_size;
                let x_win = &dt_v[start..end];
                let y_win = &gas_v[start..end];
                let r = stats::pearson_correlation(x_win, y_win).unwrap_or(0.0);
                if r > max_r {
                    // range_st = step;
                    // range_en = range_end;
                    max_r = r;

                    start_idx = start;
                    end_idx = end;
                    best_window_y = y_win.to_vec();
                }
            }
        }

        self.calc_r = max_r;
        self.calc_range_start = self.measurement_dt_v[start_idx].timestamp() as f64;
        self.calc_range_end = self.measurement_dt_v[end_idx - 1].timestamp() as f64;
        self.calc_dt_v = self.measurement_dt_v[start_idx..end_idx].to_vec();
        self.calc_gas_v = best_window_y;
    }

    pub fn find_highest_r_window(&mut self) {
        println!("Finding highest r");
        if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
            return;
        }

        let mut max_r = f64::MIN;
        let mut step: usize = 0;

        // let mut cur_window = min_window_size;
        let data_len = self.measurement_dt_v.len();
        let mut range_end = MIN_WINDOW_SIZE;
        let mut range_st = 0;
        let mut range_en = 0;
        // let mut window_dt: Vec<f64> = Vec::new();

        while step + MIN_WINDOW_SIZE <= data_len {
            range_end += step;

            if range_end > data_len {
                range_end = data_len
            }
            let window_dt: Vec<f64> = self.measurement_dt_v[step..range_end]
                .iter()
                .map(|dt| dt.timestamp() as f64)
                .collect();
            if (range_end - step) > window_dt.len() {
                // missing data points
                continue;
            }
            let window_gas = &self.measurement_gas_v[step..range_end];

            let r = stats::pearson_correlation(&window_dt, window_gas).unwrap_or(0.0);
            if range_end == data_len {
                // calculate total_r from the whole length of the measurement
                self.measurement_r = r;
            }

            if r > max_r {
                range_st = step;
                range_en = range_end;
                max_r = r;
            }
            if step == 0 {
                step += WINDOW_INCREMENT;
            }
            if range_end >= data_len {
                range_end = MIN_WINDOW_SIZE;
                step += WINDOW_INCREMENT;
            }
        }

        self.r = max_r;
        self.calc_range_start = self.calc_dt_v[range_st].timestamp() as f64;
        self.calc_range_end = self.calc_dt_v[range_en - 1].timestamp() as f64;
        self.calc_dt_v = self.calc_dt_v[range_st..range_en].to_vec();
        self.calc_gas_v = self.calc_gas_v[range_st..range_en].to_vec();
    }

    pub fn get_calc_data(&mut self) {
        // self.close_time =
        //     self.start_time + chrono::TimeDelta::seconds(self.close_offset + self.lag_s as i64);
        // self.open_time =
        //     self.start_time + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
        let s = self.calc_range_start;
        let e = self.calc_range_end;

        // Clear previous results
        self.calc_gas_v.clear();
        self.calc_dt_v.clear();

        // Filter and store results in separate vectors
        self.dt_v
            .iter()
            .zip(self.gas_data.iter()) // Pair timestamps with gas values
            .filter(|(t, _)| (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e) // Filter by time range
            .for_each(|(t, d)| {
                self.calc_dt_v.push(*t);
                match d {
                    Gas::CH4(values) => {
                        let ch4_clone = values.clone();
                        self.calc_gas_data.push(Gas::CH4(ch4_clone));
                    }
                    Gas::CO2(values) => {
                        let co2_clone = values.clone();
                        self.calc_gas_data.push(Gas::CO2(co2_clone));
                    }
                    Gas::H2O(values) => {
                        let h2o_clone = values.clone();
                        self.calc_gas_data.push(Gas::H2O(h2o_clone));
                    }
                    Gas::N2O(values) => {
                        let n2o_clone = values.clone();
                        self.calc_gas_data.push(Gas::N2O(n2o_clone));
                    }
                }
            });
    }
    pub fn get_measurement_data(&mut self) {
        let close_time =
            self.start_time + chrono::TimeDelta::seconds(self.close_offset + self.lag_s as i64);
        let open_time =
            self.start_time + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
        let s = close_time.timestamp() as f64;
        let e = open_time.timestamp() as f64;

        // Clear previous results
        self.measurement_gas_v.clear();
        self.measurement_dt_v.clear();

        // Filter and store results in separate vectors
        self.dt_v
            .iter()
            .zip(self.gas_data.iter()) // Pair timestamps with gas values
            .filter(|(t, _)| (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e) // Filter by time range
            .for_each(|(t, d)| {
                self.measurement_dt_v.push(*t);
                match d {
                    Gas::CH4(values) => {
                        let ch4_clone = values.clone();
                        self.measurement_gas_data.push(Gas::CH4(ch4_clone));
                    }
                    Gas::CO2(values) => {
                        let co2_clone = values.clone();
                        self.measurement_gas_data.push(Gas::CO2(co2_clone));
                    }
                    Gas::H2O(values) => {
                        let h2o_clone = values.clone();
                        self.measurement_gas_data.push(Gas::H2O(h2o_clone));
                    }
                    Gas::N2O(values) => {
                        let n2o_clone = values.clone();
                        self.measurement_gas_data.push(Gas::N2O(n2o_clone));
                    }
                }
            });
    }
    // unused as the current implementation
    pub fn _update_data(&mut self, dt_v: Vec<DateTime<Utc>>, gas_v: Vec<f64>) {
        self.dt_v = dt_v;
        // self.gas_v = gas_v;
        // println!("{:?}", self.dt_v);
        // println!("{:?}", self.gas_v);

        // Automatically run the calculation
        self.get_calc_data();
    }
    pub fn calculate_slope(&self) -> f64 {
        let num_ts: Vec<f64> = self
            .calc_dt_v
            .iter()
            .map(|dt| dt.timestamp() as f64)
            .collect();
        stats::LinReg::train(&num_ts, &self.calc_gas_v).slope
    }
    pub fn calculate_measurement_r(&mut self) {
        // Convert timestamps to `f64`
        let num_ts: Vec<f64> = self
            .measurement_dt_v
            .iter()
            .map(|dt| dt.timestamp() as f64)
            .collect();

        // Clear previous R values
        self.measurement_r_values.clear();

        // Compute Pearson correlation for each gas type
        for gas in &self.measurement_gas_data {
            let r_value = match gas {
                Gas::CH4(values) => {
                    // println!("{:?}", values);
                    stats::pearson_correlation(&num_ts, values).unwrap_or(0.0)
                }
                Gas::CO2(values) => stats::pearson_correlation(&num_ts, values).unwrap_or(0.0),
                Gas::H2O(values) => stats::pearson_correlation(&num_ts, values).unwrap_or(0.0),
                Gas::N2O(values) => stats::pearson_correlation(&num_ts, values).unwrap_or(0.0),
            };

            // Store the calculated R value
            self.measurement_r_values.push(match gas {
                Gas::CH4(_) => RValue::CH4(r_value),
                Gas::CO2(_) => RValue::CO2(r_value),
                Gas::H2O(_) => RValue::H2O(r_value),
                Gas::N2O(_) => RValue::N2O(r_value),
            });
        }
    }

    // pub fn calculate_measurement_r(&mut self) {
    //     // self.get_calc_data();
    //     let num_ts: Vec<f64> = self
    //         .measurement_dt_v
    //         .iter()
    //         .map(|dt| dt.timestamp() as f64)
    //         .collect();
    //     self.measurement_r =
    //         stats::pearson_correlation(&num_ts, &self.measurement_gas_v).unwrap_or(0.0);
    // }
    pub fn calculate_flux(&mut self) {
        let slope = self.calculate_slope();
        self.flux =
            slope / 1000000. * 1. * ((44. * (994. * 1000.)) / (8.314 * (10. + 273.15))) * 1000.;
    }
}

#[derive(Debug)]
pub struct GasData {
    pub header: StringRecord,
    pub datetime: Vec<DateTime<Utc>>,
    pub gas: Vec<Gas>,
    pub diag: Vec<i64>,
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
            datetime: Vec::new(),
            gas: Vec::new(),
            diag: Vec::new(),
        }
    }
    pub fn any_col_invalid(&self) -> bool {
        // Check if all gas values in each variant are invalid
        let gas_invalid = self.gas.iter().all(|gas| match gas {
            Gas::CH4(values) => values.iter().all(|&x| x == ERROR_FLOAT),
            Gas::CO2(values) => values.iter().all(|&x| x == ERROR_FLOAT),
            Gas::H2O(values) => values.iter().all(|&x| x == ERROR_FLOAT),
            Gas::N2O(values) => values.iter().all(|&x| x == ERROR_FLOAT),
        });

        // Check if all diagnostic values are invalid
        let diag_invalid = self.diag.iter().all(|&x| x == ERROR_INT);

        // If any column is fully invalid, return true
        gas_invalid || diag_invalid
    }

    pub fn summary(&self) {
        println!(
            "dt: {} len: {}, gas len: {}",
            self.datetime[0],
            self.diag.len(),
            self.gas.len()
        );
    }

    pub fn sort(&mut self) {
        let mut indices: Vec<usize> = (0..self.datetime.len()).collect();
        indices.sort_by(|&i, &j| self.datetime[i].cmp(&self.datetime[j]));

        self.datetime = indices.iter().map(|&i| self.datetime[i]).collect();
        self.diag = indices.iter().map(|&i| self.diag[i]).collect();

        self.gas = self
            .gas
            .iter()
            .map(|gas| match gas {
                Gas::CH4(values) => {
                    let sorted_values: Vec<f64> = indices
                        .iter()
                        .filter_map(|&i| values.get(i).copied()) // ✅ Avoid out-of-bounds error
                        .collect();
                    Gas::CH4(sorted_values)
                }
                Gas::CO2(values) => {
                    let sorted_values: Vec<f64> = indices
                        .iter()
                        .filter_map(|&i| values.get(i).copied())
                        .collect();
                    Gas::CO2(sorted_values)
                }
                Gas::H2O(values) => {
                    let sorted_values: Vec<f64> = indices
                        .iter()
                        .filter_map(|&i| values.get(i).copied())
                        .collect();
                    Gas::H2O(sorted_values)
                }
                Gas::N2O(values) => {
                    let sorted_values: Vec<f64> = indices
                        .iter()
                        .filter_map(|&i| values.get(i).copied())
                        .collect();
                    Gas::N2O(sorted_values)
                }
            })
            .collect();
    }

    // pub fn sort(&mut self) {
    //     let mut indices: Vec<usize> = (0..self.datetime.len()).collect();
    //     indices.sort_by(|&i, &j| self.datetime[i].cmp(&self.datetime[j]));
    //
    //     self.datetime = indices.iter().map(|&i| self.datetime[i]).collect();
    //     self.diag = indices.iter().map(|&i| self.diag[i]).collect();
    //     self.gas = self
    //         .gas
    //         .iter()
    //         .map(|gas| match gas {
    //             Gas::CH4(values) => Gas::CH4(indices.iter().map(|&i| values[i]).collect()),
    //             Gas::CO2(values) => Gas::CO2(indices.iter().map(|&i| values[i]).collect()),
    //             Gas::H2O(values) => Gas::H2O(indices.iter().map(|&i| values[i]).collect()),
    //             Gas::N2O(values) => Gas::N2O(indices.iter().map(|&i| values[i]).collect()),
    //         })
    //         .collect();
    // }
}

#[derive(Debug)]
pub struct TimeData {
    pub chamber_id: Vec<String>,
    pub start_time: Vec<DateTime<Utc>>,
    pub close_offset: Vec<i64>,
    pub open_offset: Vec<i64>,
    pub end_offset: Vec<i64>,
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
    pub fn iter(&self) -> impl Iterator<Item = (&String, &DateTime<Utc>, &i64, &i64, &i64)> {
        self.chamber_id
            .iter()
            .zip(&self.start_time)
            .zip(&self.close_offset)
            .zip(&self.open_offset)
            .zip(&self.end_offset)
            .map(|((((chamber, start), close), open), end)| (chamber, start, close, open, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn create_test_cycle() -> Cycle {
        let start_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap();
        let start_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap();
        let close_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 10, 0).unwrap();
        let close_offset = 20;
        let open_offset = 60;
        let end_offset = 100;
        let open_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 20, 0).unwrap();
        let end_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 30, 0).unwrap();
        let dt_v = (0..30)
            .map(|i| Utc.with_ymd_and_hms(2024, 2, 1, 10, i, 0).unwrap())
            .collect();
        let gas_v: Vec<f64> = (0..30).map(|i| i as f64 * 1.5 + 10.0).collect();
        let gas_data = Vec::new();
        let measurement_gas_data = Vec::new();
        let calc_gas_data = Vec::new();
        let r_values: Vec<f64> = Vec::new();

        Cycle {
            chamber_id: String::from("test_chamber"),
            start_time,
            close_time,
            open_time,
            end_time,
            close_offset,
            open_offset,
            end_offset,
            calc_range_end: close_time.timestamp() as f64,
            calc_range_start: open_time.timestamp() as f64,
            lag_s: 0.,
            max_idx: 0.,
            r: 0.0,
            r_values: Vec::new(),
            measurement_r_values: Vec::new(),
            calc_r: 0.0,
            measurement_r: 0.0,
            flux: 0.0,
            diag_v: Vec::new(),
            dt_v,
            gas_data,
            measurement_gas_data,
            calc_gas_data,
            calc_gas_v: Vec::new(),
            calc_dt_v: Vec::new(),
            measurement_gas_v: Vec::new(),
            measurement_dt_v: Vec::new(),
        }
    }

    #[test]
    fn test_get_calc_data() {
        let mut cycle = create_test_cycle();
        cycle.get_calc_data();
        assert!(
            !cycle.calc_dt_v.is_empty(),
            "Filtered timestamps should not be empty"
        );
        assert_eq!(
            cycle.calc_dt_v.len(),
            cycle.calc_gas_v.len(),
            "Timestamps and gas values lengths should match"
        );
    }

    #[test]
    fn test_calculate_slope() {
        let mut cycle = create_test_cycle();
        cycle.get_calc_data();
        let slope = cycle.calculate_slope();
        assert!(slope.is_finite(), "Slope should be a finite number");
    }

    #[test]
    fn test_calculate_r() {
        let mut cycle = create_test_cycle();
        cycle.get_calc_data();
        cycle.calculate_measurement_r();
        assert!(
            (0.0..=1.0).contains(&cycle.r),
            "Correlation coefficient should be between 0 and 1"
        );
    }

    #[test]
    fn test_calculate_flux() {
        let mut cycle = create_test_cycle();
        cycle.get_calc_data();
        cycle.calculate_flux();
        assert!(cycle.flux.is_finite(), "Flux should be a finite number");
    }

    #[ignore]
    #[test]
    fn test_update_data() {
        //
        // ignored since update_data is not being used
        //
        let mut cycle = create_test_cycle();
        let new_dt_v: Vec<DateTime<Utc>> = (0..10)
            .map(|i| Utc.with_ymd_and_hms(2024, 2, 2, 11, i, 0).unwrap())
            .collect();
        let new_gas_v: Vec<f64> = (0..10).map(|i| (i as f64) * 2.0 + 5.0).collect();
        cycle._update_data(new_dt_v.clone(), new_gas_v.clone());
        assert_eq!(
            cycle.dt_v, new_dt_v,
            "Updated timestamps should match the input"
        );
        // assert_eq!(
        //     cycle.gas_v, new_gas_v,
        //     "Updated gas values should match the input"
        // );
        assert!(
            !cycle.calc_dt_v.is_empty(),
            "Calculated timestamps should not be empty after update"
        );
    }
    #[test]
    fn test_find_highest_r_window_valid() {
        let mut cycle = create_test_cycle();
        cycle.calc_dt_v = cycle.dt_v.clone();
        // cycle.calc_gas_v = cycle.gas_v.clone();
        cycle.find_highest_r_window();
        assert!(cycle.r.is_finite(), "R value should be finite");
        assert_eq!(
            cycle.calc_dt_v.len(),
            cycle.calc_gas_v.len(),
            "Timestamps and gas values should match"
        );
        assert!(
            !cycle.calc_dt_v.is_empty(),
            "Filtered data should not be empty"
        );
    }
    #[test]
    fn test_adjust_open_time() {
        let mut cycle = create_test_cycle();
        let original_open_time = cycle.open_time;
        cycle.adjust_open_time();
        let expected_time = original_open_time + chrono::Duration::seconds(cycle.lag_s as i64);
        assert_eq!(
            cycle.open_time, expected_time,
            "Close time should be adjusted by lag_s seconds"
        );
    }

    #[test]
    fn test_adjust_open_time_zero_lag() {
        let mut cycle = create_test_cycle();
        cycle.lag_s = 0.;
        let original_open_time = cycle.open_time;
        cycle.adjust_open_time();
        assert_eq!(
            cycle.open_time, original_open_time,
            "Close time should remain unchanged when lag_s is zero"
        );
    }

    #[test]
    fn test_adjust_open_time_negative_lag() {
        let mut cycle = create_test_cycle();
        cycle.lag_s = -15.;
        let original_open_time = cycle.open_time;
        cycle.adjust_open_time();
        let expected_time = original_open_time - chrono::Duration::seconds(15);
        assert_eq!(
            cycle.open_time, expected_time,
            "Close time should be adjusted backwards by lag_s seconds"
        );
    }
}
