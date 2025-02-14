use chrono::prelude::DateTime;
use chrono::Utc;
use std::time::Duration;

use csv::StringRecord;

use crate::stats;

pub const ERROR_INT: i64 = -9999;
pub const ERROR_FLOAT: f64 = -9999.;
pub const MIN_WINDOW_SIZE: usize = 120;
pub const WINDOW_INCREMENT: usize = 10;

pub trait EqualLen {
    fn validate_lengths(&self) -> bool;
}

pub struct CycleBuilder {
    chamber_id: Option<String>,
    start_time: Option<DateTime<Utc>>,
    close_offset: Option<u64>,
    open_offset: Option<u64>,
    end_offset: Option<u64>,
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
    pub fn chamber_id(mut self, id: &str) -> Self {
        self.chamber_id = Some(id.to_string());
        self
    }

    /// Set the start time
    pub fn start_time(mut self, time: DateTime<Utc>) -> Self {
        self.start_time = Some(time);
        self
    }

    /// Set the close offset (seconds from start)
    pub fn close_offset(mut self, offset: u64) -> Self {
        self.close_offset = Some(offset);
        self
    }

    /// Set the open offset (seconds from start)
    pub fn open_offset(mut self, offset: u64) -> Self {
        self.open_offset = Some(offset);
        self
    }

    /// Set the end offset (seconds from start)
    pub fn end_offset(mut self, offset: u64) -> Self {
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
            close_time: start + Duration::from_secs(close),
            open_time: start + Duration::from_secs(open),
            end_time: start + Duration::from_secs(end),
            flux: 0.,
            r: 0.,
            dt_v: Vec::new(),
            gas_v: Vec::new(),
            calc_gas_v: Vec::new(),
            calc_dt_v: Vec::new(),
        })
    }
}
pub struct Cycle {
    pub chamber_id: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub close_time: chrono::DateTime<chrono::Utc>,
    pub open_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub r: f64,
    pub flux: f64,
    pub dt_v: Vec<chrono::DateTime<chrono::Utc>>,
    pub gas_v: Vec<f64>,
    pub calc_gas_v: Vec<f64>,
    pub calc_dt_v: Vec<chrono::DateTime<chrono::Utc>>,
}

impl Cycle {
    pub fn find_highest_r_window(&mut self) {
        if self.dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
            return;
        }

        let mut highest_r = f64::MIN;
        let mut step: usize = 0;

        // let mut cur_window = min_window_size;
        let data_len = self.calc_dt_v.len();
        let mut range_end = MIN_WINDOW_SIZE;
        let mut range_st = 0;
        let mut range_en = 0;

        while step + MIN_WINDOW_SIZE <= data_len {
            range_end += step;

            if range_end > data_len {
                range_end = data_len
            }
            let window_dt: Vec<f64> = self.calc_dt_v[step..range_end]
                .iter()
                .map(|dt| dt.timestamp() as f64)
                .collect();
            let window_gas = &self.gas_v[step..range_end];

            let r = stats::pearson_correlation(&window_dt, window_gas).unwrap_or(0.0);

            if r > highest_r {
                range_st = step;
                range_en = range_end;
                highest_r = r;
            }
            if step == 0 {
                step += WINDOW_INCREMENT;
            }
            if range_end >= data_len {
                range_end = MIN_WINDOW_SIZE;
                step += WINDOW_INCREMENT;
            }
        }

        self.r = highest_r;
        self.calc_dt_v = self.calc_dt_v[range_st..range_en].to_vec();
        self.calc_gas_v = self.calc_gas_v[range_st..range_en].to_vec();
    }

    pub fn get_calc_data(&mut self) {
        let s = self.close_time;
        let e = self.open_time;

        // Clear previous results
        self.calc_gas_v.clear();
        self.calc_dt_v.clear();

        // Filter and store results in separate vectors
        self.dt_v
            .iter()
            .zip(self.gas_v.iter()) // Pair timestamps with gas values
            .filter(|(t, _)| *t >= &s && *t <= &e) // Filter by time range
            .for_each(|(t, d)| {
                self.calc_dt_v.push(*t);
                self.calc_gas_v.push(*d);
            });
    }
    // unused as the current implementation
    pub fn _update_data(&mut self, dt_v: Vec<DateTime<Utc>>, gas_v: Vec<f64>) {
        self.dt_v = dt_v;
        self.gas_v = gas_v;
        println!("{:?}", self.dt_v);
        println!("{:?}", self.gas_v);

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
    pub fn calculate_r(&mut self) {
        let num_ts: Vec<f64> = self
            .calc_dt_v
            .iter()
            .map(|dt| dt.timestamp() as f64)
            .collect();
        let last = self.dt_v.last().unwrap();
        let first = self.dt_v[0];
        self.r = stats::pearson_correlation(&num_ts, &self.calc_gas_v).unwrap_or(0.0);
    }
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
    pub gas: Vec<f64>,
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

impl GasData {
    pub fn any_col_invalid(&self) -> bool {
        // create a list of booleans by checking all values in the vector, if all are equal to
        // error value, return true to the vector
        let invalids: [&bool; 2] = [
            &self.gas.iter().all(|&x| x == ERROR_FLOAT),
            &self.diag.iter().all(|&x| x == ERROR_INT),
        ];
        let check = invalids.iter().any(|&x| *x);
        check
    }

    pub fn summary(&self) {
        println!("dt: {} len: {}", self.datetime[0], self.diag.len());
    }

    pub fn sort(&mut self) {
        let mut indices: Vec<usize> = (0..self.datetime.len()).collect();
        indices.sort_by(|&i, &j| self.datetime[i].cmp(&self.datetime[j]));

        self.datetime = indices.iter().map(|&i| self.datetime[i]).collect();
        self.gas = indices.iter().map(|&i| self.gas[i]).collect();
        self.diag = indices.iter().map(|&i| self.diag[i]).collect();
    }
}

#[derive(Debug)]
pub struct TimeData {
    pub chamber_id: Vec<String>,
    pub start_time: Vec<DateTime<Utc>>,
    pub close_offset: Vec<u64>,
    pub open_offset: Vec<u64>,
    pub end_offset: Vec<u64>,
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
    pub fn iter(&self) -> impl Iterator<Item = (&String, &DateTime<Utc>, &u64, &u64, &u64)> {
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
        let open_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 20, 0).unwrap();
        let end_time = Utc.with_ymd_and_hms(2024, 2, 1, 10, 30, 0).unwrap();
        let dt_v = (0..30)
            .map(|i| Utc.with_ymd_and_hms(2024, 2, 1, 10, i, 0).unwrap())
            .collect();
        let gas_v = (0..30).map(|i| i as f64 * 1.5 + 10.0).collect();

        Cycle {
            chamber_id: "test_chamber".to_string(),
            start_time,
            close_time,
            open_time,
            end_time,
            r: 0.0,
            flux: 0.0,
            dt_v,
            gas_v,
            calc_gas_v: Vec::new(),
            calc_dt_v: Vec::new(),
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
        cycle.calculate_r();
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
        assert_eq!(
            cycle.gas_v, new_gas_v,
            "Updated gas values should match the input"
        );
        assert!(
            !cycle.calc_dt_v.is_empty(),
            "Calculated timestamps should not be empty after update"
        );
    }
}
