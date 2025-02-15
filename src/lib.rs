use csv::Writer;
use gas_plot::draw_gas_plot;
use std::fs::File;
use std::process;

use crate::structs::EqualLen;
use std::error::Error;
use std::path::PathBuf;

mod csv_parse;
mod gas_plot;
mod get_paths;
mod html_report;
mod stats;
mod structs;
use structs::GasData;

use std::collections::HashMap;
use std::io::Write; // Import Write trait

const R_LIM: f64 = 0.999;

pub struct Flux<'a> {
    datetime: Vec<chrono::DateTime<chrono::Utc>>,
    flux: Vec<f64>,
    r: Vec<f64>,
    chamber_id: Vec<&'a str>,
}

#[allow(clippy::needless_lifetimes)]
impl<'a> Default for Flux<'a> {
    fn default() -> Self {
        Flux::new()
    }
}
impl<'a> Flux<'a> {
    pub fn new() -> Flux<'a> {
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
                chamber_id.to_string(),
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
    pub fn build(mut args: impl Iterator<Item = String>) -> Result<Config, &'static str> {
        args.next();
        let gas_path: String = match args.next() {
            Some(str) => str.clone(),
            None => String::new(), // returning empty string will prompt get paths to to ask
        };

        let time_path: String = match args.next() {
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
        Ok(vec) => {
            if !vec.is_empty() {
                vec
            } else {
                println!("No gas data found.");
                process::exit(1)
            }
        }
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
    let mut all_gas = structs::GasData::new();
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
    let mut timev: Vec<structs::TimeData> = Vec::new();
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

    println!("Sorting");
    all_gas.sort(); // sort all_gas measurements by datetime
    println!("Sorted");
    println!("Grouping.");
    let sorted_data = group_gas_data_by_date(&all_gas);
    println!("Grouped.");

    // initiate variables
    let mut last_date = chrono::offset::Utc::now().format("%Y-%m-%d").to_string();
    let mut day = chrono::offset::Utc::now().format("%Y-%m-%d").to_string();
    let mut no_data_for_day = false;
    let mut calced = Flux::new();
    let first = all_gas.datetime[0];
    let last = all_gas.datetime.last().unwrap();
    let mut r_vec: Vec<f64> = Vec::new();
    let mut cycle_vec: Vec<structs::Cycle> = Vec::new();

    for time in &timev {
        for (chamber, start, close, open, end) in time.iter() {
            // skip loop if gas data doesnt cover the time data
            if start < &first || start > last {
                continue;
            }
            let mut cycle = structs::CycleBuilder::new()
                .chamber_id(chamber)
                .start_time(*start)
                .close_offset(*close) // 1 hour
                .open_offset(*open) // 30 minutes
                .end_offset(*end) // 2 hours
                .build()
                .expect("Failed to build Cycle");

            let st = cycle.start_time;
            let et = cycle.end_time;
            day = st.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD

            // skip measurements if there no data for that day
            if no_data_for_day && last_date == day {
                continue;
            } else {
                no_data_for_day = false;
            }
            last_date = day.clone();
            let start_younger_than_data = st < all_gas.datetime[0];
            let start_older_than_data = st > *all_gas.datetime.last().unwrap();
            if start_younger_than_data || start_older_than_data {
                continue;
            }

            // iterate over the data to create a single cycle struct
            if let Some(cur_data) = sorted_data.get(&day) {
                cur_data
                    .datetime
                    .iter()
                    .zip(cur_data.gas.iter()) // Combine times and data
                    .zip(cur_data.diag.iter()) // Combine times and data
                    .filter(|((t, _), _)| t >= &&st && t <= &&et) // Filter timestamps in range
                    .for_each(|((t, d), dg)| {
                        cycle.dt_v.push(*t); // save datetimes to cycle struct here
                        cycle.gas_v.push(*d); // save gas data to cycle struct here
                        cycle.diag_v.push(*dg); // save gas data to cycle struct here
                    }); // Convert datetime to seconds, collect f64
                cycle.get_calc_data();
                if cycle.calc_dt_v.is_empty() || cycle.calc_gas_v.is_empty() {
                    continue;
                }
                cycle.find_highest_r_window();
                r_vec.push(cycle.r);
                cycle.calculate_flux();
                if cycle.r > R_LIM {
                    calced.datetime.push(cycle.start_time);
                    calced.flux.push(cycle.flux);
                    calced.r.push(cycle.r);
                    calced.chamber_id.push(cycle.chamber_id);
                    // match draw_gas_plot(&cycle) {
                    //     Ok(val) => val,
                    //     Err(e) => {
                    //         println!("Failed to plot. Error {e}");
                    //         "images/".to_owned()
                    //     }
                    // };
                }
            } else {
                no_data_for_day = true;
                continue;
            }
            cycle_vec.push(cycle);
        }
    }

    println!("Calculated {} r values", r_vec.len());
    println!(
        "Calculated {}  flux values with r > {R_LIM}",
        calced.datetime.len()
    );
    match calced.write_to_csv("Testing.csv") {
        Ok(f) => f,
        Err(e) => println!("Problem writing file: {e}"),
    }
    // match calced.write_to_html_file("html_data.html") {
    //     Ok(f) => f,
    //     Err(e) => println!("Problem writing file: {e}"),
    // }
    match html_report::write_cycles_to_html(&cycle_vec) {
        Ok(val) => val,
        Err(e) => println!("Error {e}"),
    }
    Ok(())
}
