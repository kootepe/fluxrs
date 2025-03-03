use chrono::prelude::DateTime;
use chrono::Utc;
use rusqlite::Error;
use std::collections::HashMap;
use std::fmt;
use std::{thread, time};

use csv::StringRecord;
// use std::error::Error;

use crate::gas_plot;
use crate::instruments::GasType;
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
    pub fn build_db(self) -> Result<Cycle, Error> {
        let start = self.start_time.ok_or(Error::InvalidColumnName(
            "Start time is required".to_owned(),
        ))?;
        let chamber = self.chamber_id.ok_or(Error::InvalidColumnName(
            "Chamber ID is required".to_owned(),
        ))?;
        let close = self.close_offset.ok_or(Error::InvalidColumnName(
            "Close offset is required".to_owned(),
        ))?;
        let open = self.open_offset.ok_or(Error::InvalidColumnName(
            "Open offset is required".to_owned(),
        ))?;
        let end = self.end_offset.ok_or(Error::InvalidColumnName(
            "End offset is required".to_owned(),
        ))?;

        Ok(Cycle {
            chamber_id: chamber,
            start_time: start,
            close_time: start + chrono::Duration::seconds(close),
            open_time: start + chrono::Duration::seconds(open),
            end_time: start + chrono::Duration::seconds(end),
            close_offset: close,
            open_offset: open,
            end_offset: end,
            main_gas: GasType::CH4,
            has_errors: false,
            // calc_range_end: (start + chrono::Duration::seconds(open)).timestamp() as f64,
            // calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            gas_plot: HashMap::new(),
            calc_range_end: HashMap::new(),
            calc_range_start: HashMap::new(),
            min_y: HashMap::new(),
            max_y: HashMap::new(),
            // calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            lag_s: 0.,
            max_idx: 0.,
            flux: HashMap::new(),
            r: HashMap::new(),
            calc_r: HashMap::new(),
            measurement_r: HashMap::new(),
            diag_v: vec![],
            dt_v: vec![],
            gas_v: HashMap::new(),
            calc_gas_v: HashMap::new(),
            calc_dt_v: HashMap::new(),
            measurement_gas_v: HashMap::new(),
            measurement_dt_v: vec![],
            gases: vec![],
            air_pressure: 1000.,
            air_temperature: 10.,
        })
    }
    pub fn build(self) -> Result<Cycle, Box<dyn std::error::Error>> {
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
            has_errors: false,
            main_gas: GasType::CH4,
            // calc_range_end: (start + chrono::Duration::seconds(open)).timestamp() as f64,
            // calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            gas_plot: HashMap::new(),
            calc_range_end: HashMap::new(),
            calc_range_start: HashMap::new(),
            min_y: HashMap::new(),
            max_y: HashMap::new(),
            // calc_range_start: (start + chrono::Duration::seconds(close)).timestamp() as f64,
            lag_s: 0.,
            max_idx: 0.,
            flux: HashMap::new(),
            r: HashMap::new(),
            calc_r: HashMap::new(),
            measurement_r: HashMap::new(),
            diag_v: vec![],
            dt_v: vec![],
            gas_v: HashMap::new(),
            calc_gas_v: HashMap::new(),
            calc_dt_v: HashMap::new(),
            measurement_gas_v: HashMap::new(),
            measurement_dt_v: vec![],
            gases: vec![],
            air_pressure: 1000.,
            air_temperature: 10.,
        })
    }
}
#[derive(PartialEq)]
pub struct Cycle {
    pub chamber_id: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub close_time: chrono::DateTime<chrono::Utc>,
    pub open_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub air_temperature: f64,
    pub air_pressure: f64,
    pub has_errors: bool,
    pub main_gas: GasType,
    pub close_offset: i64,
    pub open_offset: i64,
    pub end_offset: i64,
    pub lag_s: f64,
    pub max_idx: f64,
    pub gases: Vec<GasType>,
    pub calc_range_start: HashMap<GasType, f64>,
    pub calc_range_end: HashMap<GasType, f64>,
    pub min_y: HashMap<GasType, f64>,
    pub max_y: HashMap<GasType, f64>,
    pub gas_plot: HashMap<GasType, Vec<[f64; 2]>>,

    pub flux: HashMap<GasType, f64>,
    pub r: HashMap<GasType, f64>,
    pub measurement_r: HashMap<GasType, f64>,
    pub calc_r: HashMap<GasType, f64>,

    // datetime vectors
    pub dt_v: Vec<chrono::DateTime<chrono::Utc>>,
    pub calc_dt_v: HashMap<GasType, Vec<chrono::DateTime<chrono::Utc>>>,
    pub measurement_dt_v: Vec<chrono::DateTime<chrono::Utc>>,

    // gas vectors
    pub gas_v: HashMap<GasType, Vec<f64>>,
    pub calc_gas_v: HashMap<GasType, Vec<f64>>,
    pub measurement_gas_v: HashMap<GasType, Vec<f64>>,

    pub diag_v: Vec<i64>,
}
impl fmt::Debug for Cycle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ms = self.measurement_dt_v[0];
        let me = self.measurement_dt_v.last().unwrap();
        let mr = ms.timestamp() as f64 - me.timestamp() as f64;
        let cs = self.calc_dt_v.get(&GasType::CO2).unwrap()[0];
        let ce = self.calc_dt_v.get(&GasType::CO2).unwrap().last().unwrap();
        let cr = ce.timestamp() as f64 - cs.timestamp() as f64;
        write!(
            f,
            "Cycle id: {}, \nlag: {}, \nstart: {}, \nmeas_s: {}, \nmeas_e: {}, \nmeas_r: {}, \ncalc_e: {}, \ncalc_e: {}, \ncalc_r: {}",
            self.chamber_id,
            self.lag_s,
            self.start_time,
            ms.format("%H:%M:%S"),
            me.format("%H:%M:%S"),
            mr,
            cs.format("%H:%M:%S"),
            ce.format("%H:%M:%S"),
            cr,
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
    pub fn dt_v_as_float(&self) -> Vec<f64> {
        self.dt_v.iter().map(|x| x.timestamp() as f64).collect()
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
                    let max_idxx = (self.start_time + chrono::TimeDelta::seconds(idx as i64))
                        .timestamp() as f64;
                    self.lag_s = max_idxx - (self.open_time.timestamp() as f64);
                    // println!("{}", self.lag_s);
                    return Some(peak_time);
                }
            }
        }
        self.get_measurement_datas();
        None
    }

    // pub fn get_peak_datetime(&mut self) -> Option<DateTime<Utc>> {
    //     // Find the index of the highest gas value in the last 120 elements
    //     let len = self.gas_v.len();
    //     if len < 120 {
    //         return None; // Return None if there aren't 120 elements
    //     }
    //
    //     // NOTE: maybe look around the lag adjusted open time?
    //     // right now just looks for max in the last 240 secs
    //     let start_index = len.saturating_sub(240); // Get the start index for the last 240 elements
    //
    //     let max_idx = self.gas_v[start_index..] // Take the last x elements
    //         .iter()
    //         .enumerate()
    //         .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
    //         .map(|(idx, _)| start_index + idx); // Adjust index to original vector
    //
    //     if let Some(idx) = max_idx {
    //         if let Some(peak_time) = self.dt_v.get(idx).cloned() {
    //             let max_idxx =
    //                 (self.start_time + chrono::TimeDelta::seconds(idx as i64)).timestamp() as f64;
    //             self.lag_s = max_idxx - (self.open_time.timestamp() as f64);
    //             // self.open_time = self.start_time
    //             //     + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
    //             return Some(peak_time);
    //         }
    //     }
    //
    //     None
    // }

    pub fn check_diag(&mut self) -> bool {
        self.diag_v.iter().sum::<i64>() != 0
    }

    pub fn adjust_open_time(&mut self) {
        self.open_time = self.start_time
            + chrono::TimeDelta::seconds(self.open_offset)
            + chrono::TimeDelta::seconds(self.lag_s as i64)
    }
    pub fn calculate_max_y(&mut self) {
        // let cycle = &self.cycles[self.index.count];
        // self.min_y.clear(); // Clear previous data

        for (gas_type, gas_v) in &self.gas_v {
            let min_value = gas_v
                .iter()
                .copied()
                .filter(|v| !v.is_nan())
                .fold(f64::NEG_INFINITY, f64::max);

            self.max_y.insert(*gas_type, min_value);
        }
    }
    pub fn calculate_min_y(&mut self) {
        // let cycle = &self.cycles[self.index.count];
        // self.min_y.clear(); // Clear previous data

        for (gas_type, gas_v) in &self.gas_v {
            let min_value = gas_v
                .iter()
                .copied()
                .filter(|v| !v.is_nan())
                .fold(f64::INFINITY, f64::min);

            self.min_y.insert(*gas_type, min_value);
        }
    }
    pub fn calculate_measurement_r(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
            let dt_vv: Vec<f64> = self
                .measurement_dt_v
                .iter()
                .map(|x| x.timestamp() as f64)
                .collect();
            // self.measurement_r = stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0);
            self.measurement_r.insert(
                gas_type,
                stats::pearson_correlation(&dt_vv, gas_v).unwrap_or(0.0),
            );
        }
    }
    pub fn prepare_plot_data(&mut self) {
        // let cycle = &self.cycles[self.index.count];
        self.gas_plot.clear(); // Clear existing data before recalculating

        for (gas_type, gas_v) in &self.gas_v {
            let data: Vec<[f64; 2]> = self
                .dt_v_as_float()
                .iter()
                .copied()
                .zip(gas_v.iter().copied())
                .map(|(x, y)| [x, y])
                .collect();

            self.gas_plot.insert(*gas_type, data);
        }
    }
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
            self.calc_r.insert(
                gas_type,
                stats::pearson_correlation(&dt_vv, &filtered_gas_v).unwrap_or(0.0),
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
            let dt_v: Vec<f64> = self
                .measurement_dt_v
                .iter()
                .map(|dt| dt.timestamp() as f64)
                .collect();
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

            self.calc_r.insert(gas_type, max_r);
            self.calc_range_start.insert(
                gas_type,
                self.measurement_dt_v[start_idx].timestamp() as f64,
            );
            self.calc_range_end.insert(
                gas_type,
                self.measurement_dt_v[end_idx - 1].timestamp() as f64,
            );
            self.calc_dt_v
                .insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
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
                println!("no measurement data for {}", gas_type);
            }
        }
    }

    pub fn calculate_measurement_rs(&mut self) {
        for &gas_type in &self.gases {
            if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
                let dt_vv: Vec<f64> = self
                    .measurement_dt_v
                    .iter()
                    .map(|x| x.timestamp() as f64)
                    .collect();

                self.measurement_r.insert(
                    gas_type,
                    stats::pearson_correlation(&dt_vv, gas_v).unwrap_or(0.0),
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
                let dt_v: Vec<f64> = self
                    .measurement_dt_v
                    .iter()
                    .map(|dt| dt.timestamp() as f64)
                    .collect();
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
                        let r = stats::pearson_correlation(x_win, y_win).unwrap_or(0.0);

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
                self.calc_r.insert(gas_type, max_r);
                self.calc_range_start.insert(
                    gas_type,
                    self.measurement_dt_v
                        .get(start_idx)
                        .map_or(0.0, |dt| dt.timestamp() as f64),
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
    // pub fn find_highest_r_windows(&mut self) {
    //     for &gas_type in &self.gases {
    //         if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
    //             if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
    //                 println!("Short data for {:?}", gas_type);
    //                 continue;
    //             }
    //
    //             let max_window = gas_v.len();
    //             let mut max_r = f64::MIN;
    //             let mut start_idx = 0;
    //             let mut end_idx = 0;
    //             let dt_v: Vec<f64> = self
    //                 .measurement_dt_v
    //                 .iter()
    //                 .map(|dt| dt.timestamp() as f64)
    //                 .collect();
    //             let mut best_window_y = Vec::new();
    //
    //             for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
    //                 for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
    //                     // let end = start + win_size;
    //                     let end = (start + win_size).min(max_window); // 🔹 Ensure `end` does not exceed `max_window`
    //
    //                     // Extract the window
    //                     let x_win = &dt_v[start..end];
    //                     let y_win = &gas_v[start..end];
    //
    //                     // 🔹 Check for missing timestamps
    //                     let has_missing_time = x_win
    //                         .windows(2) // Pairwise check for consecutive elements
    //                         .any(|pair| (pair[1] - pair[0]).abs() > 1.0); // Difference > 1 second means gap
    //
    //                     // 🔹 Skip calculation if there are missing timestamps
    //                     if has_missing_time {
    //                         continue;
    //                     }
    //
    //                     // Compute Pearson correlation only for valid continuous data
    //                     let r = stats::pearson_correlation(x_win, y_win).unwrap_or(0.0);
    //
    //                     if r > max_r {
    //                         max_r = r;
    //                         start_idx = start;
    //                         end_idx = end;
    //                         best_window_y = y_win.to_vec();
    //                     }
    //                 }
    //             }
    //
    //             // 🔹 Store results
    //             self.calc_r.insert(gas_type, max_r);
    //             self.calc_range_start.insert(
    //                 gas_type,
    //                 self.measurement_dt_v[start_idx].timestamp() as f64,
    //             );
    //             println!("{}", end_idx);
    //             self.calc_range_end.insert(
    //                 gas_type,
    //                 self.measurement_dt_v[end_idx - 1].timestamp() as f64,
    //             );
    //             self.calc_dt_v
    //                 .insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
    //             self.calc_gas_v.insert(gas_type, best_window_y);
    //         }
    //     }
    // }
    // pub fn find_highest_r_windows(&mut self) {
    //     for &gas_type in &self.gases {
    //         if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
    //             if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
    //                 println!("Short data for {:?}", gas_type);
    //                 continue;
    //             }
    //
    //             let max_window = gas_v.len();
    //             let mut max_r = f64::MIN;
    //             let mut start_idx = 0;
    //             let mut end_idx = 0;
    //             let dt_v: Vec<f64> = self
    //                 .measurement_dt_v
    //                 .iter()
    //                 .map(|dt| dt.timestamp() as f64)
    //                 .collect();
    //             let mut best_window_y = Vec::new();
    //
    //             for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
    //                 for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
    //                     let end = start + win_size;
    //
    //                     // Extract the window
    //                     let x_win = &dt_v[start..end];
    //                     let y_win = &gas_v[start..end];
    //
    //                     // 🔹 Skip calculation if any NaN is found in the window
    //                     if x_win.iter().any(|&x| !x.is_finite())
    //                         || y_win.iter().any(|&y| !y.is_finite())
    //                     {
    //                         continue;
    //                     }
    //
    //                     // Compute Pearson correlation only for valid data
    //                     let r = stats::pearson_correlation(x_win, y_win).unwrap_or(0.0);
    //
    //                     if r > max_r {
    //                         max_r = r;
    //                         start_idx = start;
    //                         end_idx = end;
    //                         best_window_y = y_win.to_vec();
    //                     }
    //                 }
    //             }
    //
    //             // 🔹 Store results
    //             self.calc_r.insert(gas_type, max_r);
    //             self.calc_range_start.insert(
    //                 gas_type,
    //                 self.measurement_dt_v[start_idx].timestamp() as f64,
    //             );
    //             self.calc_range_end.insert(
    //                 gas_type,
    //                 self.measurement_dt_v[end_idx - 1].timestamp() as f64,
    //             );
    //             self.calc_dt_v
    //                 .insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
    //             self.calc_gas_v.insert(gas_type, best_window_y);
    //         }
    //     }
    // }
    // pub fn find_highest_r_windows(&mut self) {
    //     for &gas_type in &self.gases {
    //         if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
    //             if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
    //                 println!("Short data for {:?}", gas_type);
    //                 continue;
    //             }
    //
    //             let max_window = gas_v.len();
    //             let mut max_r = f64::MIN;
    //             let mut start_idx = 0;
    //             let mut end_idx = 0;
    //             let dt_v: Vec<f64> = self
    //                 .measurement_dt_v
    //                 .iter()
    //                 .map(|dt| dt.timestamp() as f64)
    //                 .collect();
    //             let mut best_window_y = Vec::new();
    //
    //             for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
    //                 for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
    //                     let end = start + win_size;
    //
    //                     let x_win: Vec<f64> = dt_v[start..end]
    //                         .iter()
    //                         .cloned()
    //                         .filter(|&x| x.is_finite())
    //                         .collect();
    //                     let y_win: Vec<f64> = gas_v[start..end]
    //                         .iter()
    //                         .cloned()
    //                         .filter(|&y| y.is_finite())
    //                         .collect();
    //
    //                     // 🔹 Ensure valid values before calculating Pearson correlation
    //                     let r = if x_win.len() > 1 && y_win.len() > 1 {
    //                         stats::pearson_correlation(&x_win, &y_win).unwrap_or(0.0)
    //                     } else {
    //                         0.0 // Fallback if insufficient valid data
    //                     };
    //
    //                     if r > max_r {
    //                         max_r = r;
    //                         start_idx = start;
    //                         end_idx = end;
    //                         best_window_y = y_win;
    //                     }
    //                 }
    //             }
    //
    //             // 🔹 Store results
    //             self.calc_r.insert(gas_type, max_r);
    //             self.calc_range_start.insert(
    //                 gas_type,
    //                 self.measurement_dt_v[start_idx].timestamp() as f64,
    //             );
    //             self.calc_range_end.insert(
    //                 gas_type,
    //                 self.measurement_dt_v[end_idx - 1].timestamp() as f64,
    //             );
    //             self.calc_dt_v
    //                 .insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
    //             self.calc_gas_v.insert(gas_type, best_window_y);
    //         }
    //     }
    // }
    // pub fn find_highest_r_windows(&mut self) {
    //     for &gas_type in &self.gases {
    //         if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
    //             if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
    //                 println!("Short data for {:?}", gas_type);
    //                 continue;
    //             }
    //
    //             let max_window = gas_v.len();
    //             let mut max_r = f64::MIN;
    //             let mut start_idx = 0;
    //             let mut end_idx = 0;
    //             let dt_v: Vec<f64> = self
    //                 .measurement_dt_v
    //                 .iter()
    //                 .map(|dt| dt.timestamp() as f64)
    //                 .collect();
    //             let mut best_window_y = Vec::new();
    //
    //             for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
    //                 for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
    //                     let end = start + win_size;
    //                     let x_win = &dt_v[start..end];
    //                     let y_win = &gas_v[start..end];
    //                     let r = stats::pearson_correlation(x_win, y_win).unwrap_or(0.0);
    //                     if r > max_r {
    //                         max_r = r;
    //                         start_idx = start;
    //                         end_idx = end;
    //                         best_window_y = y_win.to_vec();
    //                     }
    //                 }
    //             }
    //
    //             self.calc_r.insert(gas_type, max_r);
    //             self.calc_range_start.insert(
    //                 gas_type,
    //                 self.measurement_dt_v[start_idx].timestamp() as f64,
    //             );
    //             self.calc_range_end.insert(
    //                 gas_type,
    //                 self.measurement_dt_v[end_idx - 1].timestamp() as f64,
    //             );
    //             self.calc_dt_v
    //                 .insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
    //             self.calc_gas_v.insert(gas_type, best_window_y);
    //         }
    //     }
    // }
    pub fn reset(&mut self) {
        self.get_peak_datetime(self.main_gas);
        self.get_measurement_datas();
        self.calculate_measurement_rs();
        if self.check_diag() {
            self.lag_s = 0.;
            self.has_errors = true;
            return;
        }
        if (self.dt_v.len() as f64) < self.end_offset as f64 * 0.9 {
            self.lag_s = 0.;
        }
        self.find_highest_r_windows();
        self.calculate_fluxes();
        self.calculate_max_y();
        self.calculate_min_y();
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

    // pub fn find_highest_r_window(&mut self) {
    //     println!("Finding highest r");
    //     if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
    //         return;
    //     }
    //
    //     let mut max_r = f64::MIN;
    //     let mut step: usize = 0;
    //
    //     // let mut cur_window = min_window_size;
    //     let data_len = self.measurement_dt_v.len();
    //     let mut range_end = MIN_WINDOW_SIZE;
    //     let mut range_st = 0;
    //     let mut range_en = 0;
    //     // let mut window_dt: Vec<f64> = Vec::new();
    //
    //     while step + MIN_WINDOW_SIZE <= data_len {
    //         range_end += step;
    //
    //         if range_end > data_len {
    //             range_end = data_len
    //         }
    //         println!("stp: {}", step);
    //         println!("len: {}", (range_end - step));
    //         let window_dt: Vec<f64> = self.measurement_dt_v[step..range_end]
    //             .iter()
    //             .map(|dt| dt.timestamp() as f64)
    //             .collect();
    //         if (range_end - step) > window_dt.len() {
    //             // missing data points
    //             continue;
    //         }
    //         let window_gas = &self.measurement_gas_v[step..range_end];
    //
    //         let r = stats::pearson_correlation(&window_dt, window_gas).unwrap_or(0.0);
    //         if range_end == data_len {
    //             // calculate total_r from the whole length of the measurement
    //             self.measurement_r = r;
    //         }
    //
    //         if r > max_r {
    //             range_st = step;
    //             range_en = range_end;
    //             max_r = r;
    //         }
    //         if step == 0 {
    //             step += WINDOW_INCREMENT;
    //         }
    //         if range_end >= data_len {
    //             range_end = MIN_WINDOW_SIZE;
    //             step += WINDOW_INCREMENT;
    //         }
    //     }
    //
    //     self.r = max_r;
    //     self.calc_range_start = self.calc_dt_v[range_st].timestamp() as f64;
    //     self.calc_range_end = self.calc_dt_v[range_en - 1].timestamp() as f64;
    //     self.calc_dt_v = self.calc_dt_v[range_st..range_en].to_vec();
    //     self.calc_gas_v = self.calc_gas_v[range_st..range_en].to_vec();
    // }

    // pub fn get_calc_data(&mut self, gas_type: GasType) {
    //     // self.close_time =
    //     //     self.start_time + chrono::TimeDelta::seconds(self.close_offset + self.lag_s as i64);
    //     // self.open_time =
    //     //     self.start_time + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
    //     let s = self.calc_range_start;
    //     let e = self.calc_range_end;
    //
    //     // Clear previous results
    //     self.calc_gas_v.clear();
    //     self.calc_dt_v.clear();
    //
    //     // Filter and store results in separate vectors
    //     self.dt_v
    //         .iter()
    //         .zip(self.gas_v.iter()) // Pair timestamps with gas values
    //         .filter(|(t, _)| (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e) // Filter by time range
    //         .for_each(|(t, d)| {
    //             self.calc_dt_v.push(*t);
    //             self.calc_gas_v.push(*d);
    //         });
    // }
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
    // unused as the current implementation
    // pub fn _update_data(&mut self, dt_v: Vec<DateTime<Utc>>, gas_v: Vec<f64>) {
    //     self.dt_v = dt_v;
    //     self.gas_v = gas_v;
    //     println!("{:?}", self.dt_v);
    //     println!("{:?}", self.gas_v);
    //
    //     // Automatically run the calculation
    //     self.get_calc_data();
    // }
    pub fn calculate_slope(&self, gas_type: GasType) -> f64 {
        if let Some(gas_v) = self.calc_gas_v.get(&gas_type) {
            let num_ts: Vec<f64> = self
                .calc_dt_v
                .get(&gas_type)
                .unwrap()
                .iter()
                .map(|dt| dt.timestamp() as f64)
                .collect();
            stats::LinReg::train(&num_ts, gas_v).slope
        } else {
            0.0
        }
    }

    // pub fn calculate_slope(&self) -> f64 {
    //     let num_ts: Vec<f64> = self
    //         .calc_dt_v
    //         .iter()
    //         .map(|dt| dt.timestamp() as f64)
    //         .collect();
    //     stats::LinReg::train(&num_ts, &self.calc_gas_v).slope
    // }
    // pub fn calculate_measurement_r(&mut self, gas_type: GasType) {
    //     // self.get_calc_data();
    //     if let Some(gas_v) = self.calc_gas_v.get(&gas_type) {
    //         let num_ts: Vec<f64> = self
    //             .measurement_dt_v
    //             .iter()
    //             .map(|dt| dt.timestamp() as f64)
    //             .collect();
    //         self.measurement_r =
    //             stats::pearson_correlation(&num_ts, &self.measurement_gas_v).unwrap_or(0.0);
    //     }
    // }
    pub fn calculate_fluxes(&mut self) {
        for &gas in &self.gases.clone() {
            self.calculate_flux(gas);
        }
    }
    pub fn calculate_flux(&mut self, gas_type: GasType) {
        let slope = self.calculate_slope(gas_type);
        let mol_mass = match gas_type {
            GasType::CO2 => 44.0,
            GasType::CH4 => 16.0,
            GasType::H2O => 18.0,
            GasType::N2O => 44.0,
        };
        self.flux.insert(
            gas_type,
            slope / 1_000_000.0
                * 1.0
                * ((mol_mass * (self.air_pressure * 1000.0))
                    / (8.314 * (self.air_temperature + 273.15)))
                * 1000.0,
        );
    }

    // pub fn calculate_flux(&mut self) {
    //     let slope = self.calculate_slope();
    //     self.flux =
    //         slope / 1000000. * 1. * ((44. * (994. * 1000.)) / (8.314 * (10. + 273.15))) * 1000.;
    // }
}

pub struct GasData {
    pub header: StringRecord,
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
            datetime: Vec::new(),
            gas: HashMap::new(),
            diag: Vec::new(),
        }
    }
    pub fn any_col_invalid(&self) -> bool {
        // Check if all values in any vector are equal to the error value
        let gas_invalid = self
            .gas
            .values()
            .any(|v| v.iter().all(|&x| x == ERROR_FLOAT));
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
                *values = indices
                    .iter()
                    .filter_map(|&i| values.get(i).copied())
                    .collect();
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
        }
    }
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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use chrono::TimeZone;
//
//     fn create_test_cycle() -> Cycle {
//         let start_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap();
//         let start_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap();
//         let close_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 10, 0).unwrap();
//         let close_offset = 20;
//         let open_offset = 60;
//         let end_offset = 100;
//         let open_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 20, 0).unwrap();
//         let end_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 30, 0).unwrap();
//         let dt_v = (0..30)
//             .map(|i| Utc.with_ymd_and_hms(2024, 2, 1, 10, i, 0).unwrap())
//             .collect();
//         let gas_v = (0..30).map(|i| i as f64 * 1.5 + 10.0).collect();
//
//         Cycle {
//             chamber_id: String::from("test_chamber"),
//             start_time,
//             close_time,
//             open_time,
//             end_time,
//             close_offset,
//             open_offset,
//             end_offset,
//             calc_range_end: close_time.timestamp() as f64,
//             calc_range_start: open_time.timestamp() as f64,
//             lag_s: 0.,
//             max_idx: 0.,
//             r: HashMap::new(),
//             calc_r: HashMap::new(),
//             measurement_r: HashMap::new(),
//             flux: HashMap::new(),
//             diag_v: Vec::new(),
//             dt_v,
//             gas_v,
//             calc_gas_v: HashMap::new(),
//             calc_dt_v: Vec::new(),
//             measurement_gas_v: HashMap::new(),
//             measurement_dt_v: Vec::new(),
//         }
//     }
//
//     #[test]
//     fn test_get_calc_data() {
//         let mut cycle = create_test_cycle();
//         cycle.get_calc_data();
//         assert!(
//             !cycle.calc_dt_v.is_empty(),
//             "Filtered timestamps should not be empty"
//         );
//         assert_eq!(
//             cycle.calc_dt_v.len(),
//             cycle.calc_gas_v.len(),
//             "Timestamps and gas values lengths should match"
//         );
//     }
//
//     #[test]
//     fn test_calculate_slope() {
//         let mut cycle = create_test_cycle();
//         cycle.get_calc_data();
//         let slope = cycle.calculate_slope();
//         assert!(slope.is_finite(), "Slope should be a finite number");
//     }
//
//     #[test]
//     fn test_calculate_r() {
//         let mut cycle = create_test_cycle();
//         cycle.get_calc_data();
//         cycle.calculate_measurement_r();
//         assert!(
//             (0.0..=1.0).contains(&cycle.r),
//             "Correlation coefficient should be between 0 and 1"
//         );
//     }
//
//     // #[test]
//     // fn test_calculate_flux() {
//     //     let mut cycle = create_test_cycle();
//     //     cycle.get_calc_data();
//     //     cycle.calculate_flux();
//     //     assert!(cycle.flux.is_finite(), "Flux should be a finite number");
//     // }
//     #[ignore]
//     #[test]
//     fn test_update_data() {
//         //
//         // ignored since update_data is not being used
//         //
//         let mut cycle = create_test_cycle();
//         let new_dt_v: Vec<DateTime<Utc>> = (0..10)
//             .map(|i| Utc.with_ymd_and_hms(2024, 2, 2, 11, i, 0).unwrap())
//             .collect();
//         let new_gas_v: Vec<f64> = (0..10).map(|i| (i as f64) * 2.0 + 5.0).collect();
//         cycle._update_data(new_dt_v.clone(), new_gas_v.clone());
//         assert_eq!(
//             cycle.dt_v, new_dt_v,
//             "Updated timestamps should match the input"
//         );
//         // assert_eq!(
//         //     cycle.gas_v, new_gas_v,
//         //     "Updated gas values should match the input"
//         // );
//         assert!(
//             !cycle.calc_dt_v.is_empty(),
//             "Calculated timestamps should not be empty after update"
//         );
//     }
//     #[test]
//     fn test_find_highest_r_window_valid() {
//         let mut cycle = create_test_cycle();
//         cycle.calc_dt_v = cycle.dt_v.clone();
//         cycle.calc_gas_v = cycle.gas_v.clone();
//         cycle.find_highest_r_window();
//         assert!(cycle.r.is_finite(), "R value should be finite");
//         assert_eq!(
//             cycle.calc_dt_v.len(),
//             cycle.calc_gas_v.len(),
//             "Timestamps and gas values should match"
//         );
//         assert!(
//             !cycle.calc_dt_v.is_empty(),
//             "Filtered data should not be empty"
//         );
//     }
//     #[test]
//     fn test_adjust_open_time() {
//         let mut cycle = create_test_cycle();
//         let original_open_time = cycle.open_time;
//         cycle.adjust_open_time();
//         let expected_time = original_open_time + chrono::Duration::seconds(cycle.lag_s as i64);
//         assert_eq!(
//             cycle.open_time, expected_time,
//             "Close time should be adjusted by lag_s seconds"
//         );
//     }
//
//     #[test]
//     fn test_adjust_open_time_zero_lag() {
//         let mut cycle = create_test_cycle();
//         cycle.lag_s = 0.;
//         let original_open_time = cycle.open_time;
//         cycle.adjust_open_time();
//         assert_eq!(
//             cycle.open_time, original_open_time,
//             "Close time should remain unchanged when lag_s is zero"
//         );
//     }
//
//     #[test]
//     fn test_adjust_open_time_negative_lag() {
//         let mut cycle = create_test_cycle();
//         cycle.lag_s = -15.;
//         let original_open_time = cycle.open_time;
//         cycle.adjust_open_time();
//         let expected_time = original_open_time - chrono::Duration::seconds(15);
//         assert_eq!(
//             cycle.open_time, expected_time,
//             "Close time should be adjusted backwards by lag_s seconds"
//         );
//     }
// }
