use csv::Writer;
// use gas_plot::draw_gas_plot;
use std::fs::File;
use std::process;

use instruments::Li7810;
use std::error::Error;
use structs::EqualLen;

mod csv_parse;
mod gas_plot;
mod get_paths;
mod html_report;
mod instruments;
pub mod myapp;
mod stats;
mod structs;
use instruments::GasType;
use structs::GasData;
// mod app;
// mod plot_demo;

use std::collections::HashMap;
use std::io::Write; // Import Write trait

pub struct Flux {
    datetime: Vec<chrono::DateTime<chrono::Utc>>,
    flux: Vec<f64>,
    r: Vec<f64>,
    chamber_id: Vec<String>,
}

#[allow(clippy::needless_lifetimes)]
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
                chamber_id.to_string(),
            ])?;
        }

        // Flush and Finish
        wtr.flush()?;
        println!("Data successfully written to {}", filename);
        Ok(())
    }
}

fn group_gas_data_by_date(gas_data: &GasData) -> HashMap<String, GasData> {
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    for (i, (dt, diag)) in gas_data.datetime.iter().zip(&gas_data.diag).enumerate() {
        let date_key = dt.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD

        // Get or insert the daily GasData entry
        let entry = grouped_data.entry(date_key).or_insert_with(|| GasData {
            header: gas_data.header.clone(),
            datetime: Vec::new(),
            gas: HashMap::new(),
            diag: Vec::new(),
        });

        // Add values to the daily entry
        entry.datetime.push(*dt);
        entry.diag.push(*diag);

        for (gas_type, values) in &gas_data.gas {
            if let Some(value) = values.get(i) {
                entry
                    .gas
                    .entry(*gas_type)
                    .or_insert_with(Vec::new)
                    .push(*value);
            }
        }
        // entry.gas.get(GasData::CH4).len();
        // println!("{:?}", (entry.gas.get(GasType::CH4).len()));
    }

    grouped_data
}

// pub fn group_gas_data_by_date(gas_data: &GasData) -> HashMap<String, GasData> {
//     let mut grouped_data: HashMap<String, GasData> = HashMap::new();
//
//     for (dt, diag) in gas_data.datetime.iter().zip(&gas_data.diag) {
//         let date_key = dt.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
//
//         // Get or insert the daily GasData entry
//         let entry = grouped_data.entry(date_key).or_insert_with(|| GasData {
//             header: gas_data.header.clone(),
//             datetime: Vec::new(),
//             gas: HashMap::new(),
//             diag: Vec::new(),
//         });
//
//         // Add values to the daily entry
//         // println!("{}", dt.len());
//         entry.datetime.push(*dt);
//         entry.diag.push(*diag);
//
//         for (gas_type, values) in &gas_data.gas {
//             println!("pushing: {}", values.len());
//             entry
//                 .gas
//                 .entry(*gas_type)
//                 .or_insert_with(Vec::new)
//                 .extend(values.iter().copied());
//         }
//     }
//
//     grouped_data
// }

// pub fn group_gas_data_by_date(gas_data: &GasData) -> HashMap<String, GasData> {
//     let mut grouped_data: HashMap<String, GasData> = HashMap::new();
//
//     for (dt, gas, diag) in gas_data
//         .datetime
//         .iter()
//         .zip(&gas_data.gas)
//         .zip(&gas_data.diag)
//         .map(|((dt, gas), diag)| (dt, gas, diag))
//     {
//         let date_key = dt.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
//
//         // Get or insert the daily GasData entry
//         let entry = grouped_data.entry(date_key).or_insert_with(|| GasData {
//             header: gas_data.header.clone(),
//             datetime: Vec::new(),
//             gas: Vec::new(),
//             diag: Vec::new(),
//         });
//
//         // Add values to the daily entry
//         entry.datetime.push(*dt);
//         entry.gas.push(*gas);
//         entry.diag.push(*diag);
//     }
//
//     grouped_data
// }

pub struct Config {
    pub gas_path: String,
    pub time_path: String,
    pub R_LIM: f64,
}

impl Config {
    pub fn build(mut args: impl Iterator<Item = String>) -> Result<Config, &'static str> {
        const R_LIM: f64 = 0.999;
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
            R_LIM,
        })
    }
}

pub fn run(config: Config) -> Result<Vec<structs::Cycle>, Box<dyn Error>> {
    let all_gas = get_gas_data(&config.gas_path)?;
    let timev = get_time_data(&config.time_path)?;
    if let Some(gas_values) = all_gas.gas.get(&GasType::CH4) {
        println!("{:?}", gas_values.len());
    } else {
        println!("CH4 gas data not found");
    }

    println!("Sorting and grouping data...");
    // let sorted_data = sort_and_group_gas(&all_gas);
    let sorted_data = group_gas_data_by_date(&all_gas);
    println!("{}", sorted_data.len());

    println!("Processing cycles");
    let cycle_vec = process_cycles(&timev, &all_gas, &sorted_data)?;

    println!("Calculated {} cycles.", cycle_vec.len());

    // prepare_plot_data(&cycle_vec[0], GasType::CH4);

    Ok(cycle_vec)
}
fn get_gas_data(path: &str) -> Result<structs::GasData, Box<dyn Error>> {
    let gas_paths = get_paths::get_paths(path.to_owned(), "gas")?;
    let mut all_gas = structs::GasData::new();

    for path in gas_paths {
        let instrument = Li7810::default();
        let res = instrument.read_data_file(&path.to_owned())?;

        if res.validate_lengths() && !res.any_col_invalid() {
            all_gas.datetime.extend(res.datetime);
            all_gas.diag.extend(res.diag);

            // Merge gas values correctly
            for (gas_type, values) in res.gas {
                all_gas
                    .gas
                    .entry(gas_type)
                    .or_insert_with(Vec::new)
                    .extend(values);
            }
        }
    }

    all_gas.sort();
    Ok(all_gas)
}

// fn get_gas_data(path: &str) -> Result<structs::GasData, Box<dyn Error>> {
//     let gas_paths = get_paths::get_paths(path.to_owned(), "gas")?;
//     let mut all_gas = structs::GasData::new();
//     for path in gas_paths {
//         let instrument = Li7810::default();
//         let res = instrument.read_data_file(&path.to_owned())?;
//         // let res = csv_parse::read_gas_csv(&path.to_owned())?;
//         if res.validate_lengths() && !res.any_col_invalid() {
//             all_gas.datetime.extend(res.datetime);
//             all_gas.gas.extend(res.gas);
//             all_gas.diag.extend(res.diag);
//         }
//     }
//     all_gas.sort();
//     Ok(all_gas)
// }

fn get_time_data(path: &str) -> Result<Vec<structs::TimeData>, Box<dyn Error>> {
    let time_paths = get_paths::get_paths(path.to_owned(), "time")?;
    let mut timev = Vec::new();
    for path in time_paths {
        let res = csv_parse::read_time_csv(&path)?;
        if res.validate_lengths() {
            timev.push(res);
        }
    }
    Ok(timev)
}

fn sort_and_group_gas(all_gas: &structs::GasData) -> HashMap<String, structs::GasData> {
    group_gas_data_by_date(all_gas)
}
fn process_cycles(
    timev: &[structs::TimeData],
    all_gas: &structs::GasData,
    sorted_data: &HashMap<String, structs::GasData>,
) -> Result<Vec<structs::Cycle>, Box<dyn Error>> {
    let mut cycle_vec = Vec::new();
    let mut no_data_for_day = false;
    let mut last_date = chrono::offset::Utc::now().format("%Y-%m-%d").to_string();
    for time in timev {
        for (chamber, start, close, open, end) in time.iter() {
            // if start < &all_gas.datetime[0] || start > all_gas.datetime.last().unwrap() {
            //     continue;
            // }
            let mut cycle = structs::CycleBuilder::new()
                .chamber_id(chamber.to_owned())
                .start_time(*start)
                .close_offset(*close)
                .open_offset(*open)
                .end_offset(*end)
                .build()?;
            let st = cycle.start_time;
            let day = st.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
            if no_data_for_day && last_date == day {
                continue;
            } else {
                no_data_for_day = false;
            }
            last_date = day.clone();

            if let Some(cur_data) = sorted_data.get(&start.format("%Y-%m-%d").to_string()) {
                cur_data.datetime.iter().enumerate().for_each(|(i, t)| {
                    if t >= &cycle.start_time && t <= &cycle.end_time {
                        // println!("Time within {} and {}", cycle.start_time, cycle.end_time);
                        cycle.dt_v.push(*t);
                        for (gas_type, gas_values) in &cur_data.gas {
                            // println!("{:?}", gas_values.len());
                            if let Some(value) = gas_values.get(i) {
                                cycle
                                    .gas_v
                                    .entry(*gas_type)
                                    .or_insert_with(Vec::new)
                                    .push(*value);
                            }
                        }
                        if let Some(value) = cur_data.diag.get(i) {
                            cycle.diag_v.push(*value);
                        }
                    }
                    // } else {
                    //     println!(
                    //         "Time not within {} and {}",
                    //         cycle.start_time, cycle.end_time
                    //     );
                    // }
                });
                // println!("totdt: {:?}", cur_data.datetime.len());

                for gas_type in cur_data.gas.keys() {
                    if *gas_type == GasType::CH4 {
                        cycle.get_peak_datetime(*gas_type);
                    }
                    cycle.get_measurement_data(*gas_type);
                    // println!("mesdtv: {:?}", cycle.measurement_dt_v.len());
                    if let Some(measurement_gas_v) = cycle.measurement_gas_v.get(gas_type) {
                        // println!("mesgas: {:?}", measurement_gas_v.len());
                        if !cycle.measurement_dt_v.is_empty() && !measurement_gas_v.is_empty() {
                            cycle.calculate_measurement_r(*gas_type);
                            cycle.find_highest_r_window_disp(*gas_type);
                            cycle.calculate_flux(*gas_type);
                            cycle.calculate_min_y();
                            cycle.calculate_max_y();
                            cycle.prepare_plot_data();
                        }
                    }
                }
                cycle_vec.push(cycle);
            } else {
                no_data_for_day = true;
                continue;
            }
        }
    }
    Ok(cycle_vec)
}
// fn process_cycles(
//     timev: &[structs::TimeData],
//     all_gas: &structs::GasData,
//     sorted_data: &HashMap<String, structs::GasData>,
// ) -> Result<Vec<structs::Cycle>, Box<dyn Error>> {
//     let mut cycle_vec = Vec::new();
//     for time in timev {
//         for (chamber, start, close, open, end) in time.iter() {
//             if start < &all_gas.datetime[0] || start > all_gas.datetime.last().unwrap() {
//                 continue;
//             }
//             let mut cycle = structs::CycleBuilder::new()
//                 .chamber_id(chamber.to_owned())
//                 .start_time(*start)
//                 .close_offset(*close)
//                 .open_offset(*open)
//                 .end_offset(*end)
//                 .build()?;
//
//             if let Some(cur_data) = sorted_data.get(&start.format("%Y-%m-%d").to_string()) {
//                 cur_data.datetime.iter().enumerate().for_each(|(i, t)| {
//                     if t >= &cycle.start_time && t <= &cycle.end_time {
//                         cycle.dt_v.push(*t);
//                         for (gas_type, gas_values) in &cur_data.gas {
//                             cycle
//                                 .gas_v
//                                 .entry(*gas_type)
//                                 .or_insert_with(Vec::new)
//                                 .push(gas_values[i]);
//                         }
//                         cycle.diag_v.push(cur_data.diag[i]);
//                     }
//                 });
//
//                 for gas_type in cur_data.gas.keys() {
//                     cycle.get_peak_datetime(*gas_type);
//                     cycle.get_measurement_data(*gas_type);
//                     if let Some(measurement_gas_v) = cycle.measurement_gas_v.get(gas_type) {
//                         if !cycle.measurement_dt_v.is_empty() && !measurement_gas_v.is_empty() {
//                             cycle.calculate_measurement_r(*gas_type);
//                             cycle.find_highest_r_window_disp(*gas_type);
//                             cycle.calculate_flux(*gas_type);
//                         }
//                     }
//                 }
//                 cycle_vec.push(cycle);
//             }
//         }
//     }
//     Ok(cycle_vec)
// }

// fn process_cycles(
//     timev: &[structs::TimeData],
//     all_gas: &structs::GasData,
//     sorted_data: &HashMap<String, structs::GasData>,
// ) -> Result<Vec<structs::Cycle>, Box<dyn Error>> {
//     let mut cycle_vec = Vec::new();
//     for time in timev {
//         for (chamber, start, close, open, end) in time.iter() {
//             if start < &all_gas.datetime[0] || start > all_gas.datetime.last().unwrap() {
//                 continue;
//             }
//             let mut cycle = structs::CycleBuilder::new()
//                 .chamber_id(chamber.to_owned())
//                 .start_time(*start)
//                 .close_offset(*close)
//                 .open_offset(*open)
//                 .end_offset(*end)
//                 .build()?;
//
//             if let Some(cur_data) = sorted_data.get(&start.format("%Y-%m-%d").to_string()) {
//                 cur_data
//                     .datetime
//                     .iter()
//                     .zip(cur_data.gas.iter())
//                     .zip(cur_data.diag.iter())
//                     .filter(|((t, _), _)| t >= &&cycle.start_time && t <= &&cycle.end_time)
//                     .for_each(|((t, d), dg)| {
//                         cycle.dt_v.push(*t);
//                         cycle.gas_v.push(*d);
//                         cycle.diag_v.push(*dg);
//                     });
//                 cycle.get_peak_datetime();
//                 cycle.get_measurement_data();
//                 if !cycle.measurement_dt_v.is_empty() && !cycle.measurement_gas_v.is_empty() {
//                     cycle.calculate_measurement_r();
//                     cycle.find_highest_r_window_disp();
//                     cycle.calculate_flux();
//                     cycle_vec.push(cycle);
//                 }
//             }
//         }
//     }
//     Ok(cycle_vec)
// }

// fn prepare_plot_data(cycle: &structs::Cycle) -> Vec<[f64; 2]> {
//     cycle
//         .dt_v_as_float()
//         .iter()
//         .copied()
//         .zip(cycle.gas_v.iter().copied())
//         .map(|(x, y)| [x, y])
//         .collect()
// }
fn prepare_plot_data(cycle: &structs::Cycle, gas_type: GasType) -> Vec<[f64; 2]> {
    if let Some(gas_v) = cycle.gas_v.get(&gas_type) {
        cycle
            .dt_v_as_float()
            .iter()
            .copied()
            .zip(gas_v.iter().copied())
            .map(|(x, y)| [x, y])
            .collect()
    } else {
        Vec::new()
    }
}

pub fn exit_with_help() {
    let help = String::from(
        r#"Usage, remember quotes:
    fluxrs "<gas path glob>" "<time path glob>"
Example:
    fluxrs "data/24*.DAT" "time_data/24*""#,
    );
    println!("{help}");
    process::exit(1)
}
