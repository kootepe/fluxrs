use csv::Writer;
use std::fs::File;
use std::time::Duration;

use crate::structs::EqualLen;
use std::error::Error;
use std::path::PathBuf;

mod csv_parse;
mod get_paths;
mod stats;
mod structs;

use structs::GasData;

use std::collections::HashMap;

pub struct Flux {
    datetime: Vec<chrono::DateTime<chrono::Utc>>,
    flux: Vec<f64>,
    r: Vec<f64>,
    chamber_id: Vec<String>,
}

impl Default for Flux {
    fn default() -> Self {
        Flux::new()
    }
}
impl Flux {
    pub fn new() -> Flux {
        Flux {
            datetime: Vec::new(),
            flux: Vec::new(),
            r: Vec::new(),
            chamber_id: Vec::new(),
        }
    }

    pub fn write_to_csv(&self, filename: &str) -> Result<(), Box<dyn Error>> {
        // Ensure all vectors have the same length
        assert!(
            self.datetime.len() == self.flux.len()
                && self.flux.len() == self.r.len()
                && self.r.len() == self.chamber_id.len(),
            "All vectors in Flux struct must have the same length"
        );

        let mut wtr = Writer::from_writer(File::create(filename)?);

        // Write CSV Header
        wtr.write_record(["datetime", "flux", "r", "chamber_id"])?;

        // Write CSV Rows
        for ((time, (flux, r)), chamber_id) in self
            .datetime
            .iter()
            .zip(self.flux.iter().zip(self.r.iter()))
            .zip(self.chamber_id.iter())
        {
            wtr.write_record(&[
                time.to_rfc3339(),
                flux.to_string(),
                r.to_string(),
                chamber_id.clone(),
            ])?;
        }

        // Flush and Finish
        wtr.flush()?;
        println!("Data successfully written to {}", filename);
        Ok(())
    }
}

pub fn group_gas_data_by_date(gas_data: &GasData) -> HashMap<String, GasData> {
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    for (dt, gas, diag) in gas_data
        .datetime
        .iter()
        .zip(&gas_data.gas)
        .zip(&gas_data.diag)
        .map(|((dt, gas), diag)| (dt, gas, diag))
    {
        let date_key = dt.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD

        // Get or insert the daily GasData entry
        let entry = grouped_data.entry(date_key).or_insert_with(|| GasData {
            header: gas_data.header.clone(),
            datetime: Vec::new(),
            gas: Vec::new(),
            diag: Vec::new(),
        });

        // Add values to the daily entry
        entry.datetime.push(*dt);
        entry.gas.push(*gas);
        entry.diag.push(*diag);
    }

    grouped_data
}

pub struct Config {
    pub gas_path: String,
    pub time_path: String,
}

impl Config {
    pub fn build(args: &[String]) -> Result<Config, &'static str> {
        let gas_path: String = match args.get(1) {
            Some(str) => str.clone(),
            None => String::new(), // returning empty string will prompt get paths to to ask
        };

        let time_path: String = match args.get(2) {
            Some(str) => str.clone(),
            None => String::new(), // returning empty string will prompt get paths to to ask
        };

        Ok(Config {
            gas_path,
            time_path,
        })
    }
}

pub fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let str: &str = "gas";
    let gaspaths: Vec<PathBuf> = match get_paths::get_paths(config.gas_path, str) {
        Ok(vec) => vec,
        Err(e) => {
            println!("{}", e);
            return Err(e);
        }
    };
    let str: &str = "time";
    let timepaths: Vec<PathBuf> = match get_paths::get_paths(config.time_path, str) {
        Ok(vec) => vec,
        Err(e) => {
            println!("{}", e);
            return Err(e);
        }
    };
        header: csv::StringRecord::new(),
        datetime: Vec::new(),
        gas: Vec::new(),
        diag: Vec::new(),
    };
    println!("Processing {} files.", gaspaths.len());
    for path in gaspaths {
        match csv_parse::read_gas_csv(&path) {
            Ok(res) => {
                if res.validate_lengths() && !res.any_col_invalid() {
                    all_gas.datetime.extend(res.datetime);
                    all_gas.gas.extend(res.gas);
                    all_gas.diag.extend(res.diag);
                }
            }
            Err(err) => {
                println!("Crashed with: {}, {:?}", err, &path);
                return Err(err);
            }
        };
    }
    let mut timev: Vec<csv_parse::TimeData> = Vec::new();
    for path in timepaths {
        match csv_parse::read_time_csv(&path) {
            Ok(res) => {
                if res.validate_lengths() {
                    timev.push(res);
                }
            }
            Err(err) => {
                println!("Crashed with: {}, {:?}", err, &path);
                return Err(err);
            }
        };
    }
    let mut cycles: Vec<csv_parse::Cycle> = Vec::new();

    let mut r_vec: Vec<f64> = Vec::new();
    println!("Sorting");
    all_gas.sort(); // sort all_gas measurements by datetime
    println!("Sorted");
    for time in &timev {
        for (chamber, start, close, open, end) in time.iter() {
            println!("Processing cycle {}", &start);
            let mut cycle = csv_parse::Cycle {
                chamber_id: chamber.clone(),
                start_time: *start,
                close_time: *start + Duration::from_secs(*close) + Duration::from_secs(120),
                open_time: *start + Duration::from_secs(*open) - Duration::from_secs(120),
                end_time: *start + Duration::from_secs(*end),
                datetime_v: Vec::new(),
                gas_v: Vec::new(),
            };
            let st = &cycle.close_time;
            let et = &cycle.open_time;
            let start_younger_than_data = st < &all_gas.datetime[0];
            let start_older_than_data = st > all_gas.datetime.last().unwrap();
            if start_younger_than_data || start_older_than_data {
                println!("Skipped");
                continue;
            }
            let data: Vec<(f64, f64)> = all_gas
                .datetime
                .iter()
                .zip(all_gas.gas.iter()) // Combine times and data
                .filter(|(t, _)| *t >= st && *t <= et) // Filter timestamps in range
                .map(|(t, d)| {
                    cycle.datetime_v.push(*t); // save datetimes to cycle struct here
                    cycle.gas_v.push(*d); // save gas data to cycle struct here
                    (t.timestamp() as f64, *d)
                }) // Convert datetime to seconds, collect f64
                .collect();
            let (x, y): (Vec<f64>, Vec<f64>) = data.into_iter().unzip();
            if x.is_empty() || y.is_empty() {
                continue;
            } else {
                let r = stats::pearson_correlation(&x, &cycle.gas_v).unwrap_or(0.0);
                cycles.push(cycle);
                // println!("{}", r);
                r_vec.push(r)
            }
        }
    }

    println!("Calculated {} r values", r_vec.len());
    Ok(())
}
