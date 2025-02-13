use chrono::prelude::DateTime;
use chrono::Utc;
use std::time::Duration;

use csv::StringRecord;

use crate::stats;

pub const ERROR_INT: i64 = -9999;
pub const ERROR_FLOAT: f64 = -9999.;

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
            close_time: start + Duration::from_secs(close) + Duration::from_secs(120),
            open_time: start + Duration::from_secs(open) - Duration::from_secs(120),
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
    pub fn _update_data(&mut self, dt_v: Vec<DateTime<Utc>>, gas_v: Vec<f64>) {
        self.dt_v = dt_v;
        self.gas_v = gas_v;

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
        println!("f: {}", first);
        println!("l: {}", last);
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
