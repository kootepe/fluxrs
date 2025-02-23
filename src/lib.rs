use csv::Writer;
use std::cmp::Ordering;
// use gas_plot::draw_gas_plot;
use std::fs::File;
use std::process;

use crate::structs::EqualLen;
use std::error::Error;

mod csv_parse;
mod gas_plot;
mod get_paths;
mod html_report;
mod instruments;
pub mod myapp;
mod stats;
mod structs;
use instruments::Gas;
use instruments::Li7810;
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
pub fn group_gas_data_by_date(gas_data: &GasData) -> HashMap<String, GasData> {
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    for (dt, diag) in gas_data.datetime.iter().zip(&gas_data.diag) {
        let date_key = dt.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD

        // Get or insert the daily GasData entry
        let entry = grouped_data
            .entry(date_key.clone())
            .or_insert_with(|| GasData {
                header: gas_data.header.clone(),
                datetime: Vec::new(),
                gas: vec![
                    Gas::CH4(Vec::new()),
                    Gas::CO2(Vec::new()),
                    Gas::H2O(Vec::new()),
                    Gas::N2O(Vec::new()),
                ], // Initialize all gas types
                diag: Vec::new(),
            });

        // Add datetime & diag
        entry.datetime.push(*dt);
        entry.diag.push(*diag);

        // Iterate over `gas_data.gas` to find matching gas values for this timestamp index
        for gas in &gas_data.gas {
            match gas {
                Gas::CH4(values) => {
                    if let Gas::CH4(existing) = &mut entry.gas[0] {
                        existing.push(values[entry.datetime.len() - 1]);
                    }
                }
                Gas::CO2(values) => {
                    if let Gas::CO2(existing) = &mut entry.gas[1] {
                        existing.push(values[entry.datetime.len() - 1]);
                    }
                }
                Gas::H2O(values) => {
                    if let Gas::H2O(existing) = &mut entry.gas[2] {
                        existing.push(values[entry.datetime.len() - 1]);
                    }
                }
                Gas::N2O(values) => {
                    if let Gas::N2O(existing) = &mut entry.gas[3] {
                        existing.push(values[entry.datetime.len() - 1]);
                    }
                }
            }
        }
    }

    grouped_data
}

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
//         let entry = grouped_data
//             .entry(date_key.clone())
//             .or_insert_with(|| GasData {
//                 header: gas_data.header.clone(),
//                 datetime: Vec::new(),
//                 gas: vec![
//                     Gas::CH4(Vec::new()),
//                     Gas::CO2(Vec::new()),
//                     Gas::H2O(Vec::new()),
//                     Gas::N2O(Vec::new()),
//                 ], // Initialize with empty variants
//                 diag: Vec::new(),
//             });
//
//         // Add datetime & diag
//         println!("{}", gas_data.datetime[0]);
//         entry.datetime.push(*dt);
//         entry.diag.push(*diag);
//
//         // Match and append to the correct enum variant
//         for stored_gas in &mut entry.gas {
//             match (stored_gas, gas) {
//                 (Gas::CH4(existing), Gas::CH4(new)) => existing.extend(new),
//                 (Gas::CO2(existing), Gas::CO2(new)) => existing.extend(new),
//                 (Gas::H2O(existing), Gas::H2O(new)) => existing.extend(new),
//                 (Gas::N2O(existing), Gas::N2O(new)) => existing.extend(new),
//                 _ => {} // Ignore mismatches
//             }
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
//             gas: vec![
//                 Gas::CH4(Vec::new()),
//                 Gas::CO2(Vec::new()),
//                 Gas::H2O(Vec::new()),
//                 Gas::N2O(Vec::new()),
//             ], // ✅ Initialize each gas type with empty vectors
//             diag: Vec::new(),
//         });
//
//         // Add datetime & diag
//         entry.datetime.push(*dt);
//         entry.diag.push(*diag);
//
//         // Append gas values to the correct variant
//         match gas {
//             Gas::CH4(values) => {
//                 if let Gas::CH4(existing) = &mut entry.gas[0] {
//                     existing.extend(values);
//                 }
//             }
//             Gas::CO2(values) => {
//                 if let Gas::CO2(existing) = &mut entry.gas[1] {
//                     existing.extend(values);
//                 }
//             }
//             Gas::H2O(values) => {
//                 if let Gas::H2O(existing) = &mut entry.gas[2] {
//                     existing.extend(values);
//                 }
//             }
//             Gas::N2O(values) => {
//                 if let Gas::N2O(existing) = &mut entry.gas[3] {
//                     existing.extend(values);
//                 }
//             }
//         }
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
    // for gas in &all_gas.gas {
    //     match gas {
    //         Gas::CH4(values) => {
    //             println!("{:?}", values.len())
    //         }
    //         Gas::CO2(values) => {
    //             println!("{:?}", values.len())
    //         }
    //         Gas::H2O(values) => {
    //             println!("{:?}", values.len())
    //         }
    //         Gas::N2O(values) => {
    //             println!("{:?}", values.len())
    //         }
    //     }
    // }

    println!("Sorting and grouping data...");
    let sorted_data = sort_and_group_gas(&all_gas);
    println!("{:?}", sorted_data.len());

    let cycle_vec = process_cycles(&timev, &all_gas, &sorted_data)?;

    println!("Calculated {} cycles.", cycle_vec.len());

    let xy = prepare_plot_data(&cycle_vec[0]);

    Ok(cycle_vec)
}

fn get_gas_data(path: &str) -> Result<structs::GasData, Box<dyn Error>> {
    let gas_paths = get_paths::get_paths(path.to_owned(), "gas")?;
    let mut all_gas = structs::GasData::new();
    for path in gas_paths {
        // let res = csv_parse::read_gas_csv(&path.to_owned())?;
        let instrument = Li7810::default();
        let res = instrument.read_csv(&path.to_owned())?;
        if res.validate_lengths() && !res.any_col_invalid() {
            all_gas.datetime.extend(res.datetime);
            all_gas.diag.extend(res.diag);
            for gas in &res.gas {
                match gas {
                    Gas::CH4(values) => {
                        // println!("{:?}", values);
                        if let Some(Gas::CH4(existing)) =
                            all_gas.gas.iter_mut().find(|g| matches!(g, Gas::CH4(_)))
                        {
                            existing.extend(values.clone());
                        } else {
                            all_gas.gas.push(Gas::CH4(values.clone()));
                        }
                    }
                    Gas::CO2(values) => {
                        if let Some(Gas::CO2(existing)) =
                            all_gas.gas.iter_mut().find(|g| matches!(g, Gas::CO2(_)))
                        {
                            existing.extend(values.clone());
                        } else {
                            all_gas.gas.push(Gas::CO2(values.clone()));
                        }
                    }
                    Gas::H2O(values) => {
                        if let Some(Gas::H2O(existing)) =
                            all_gas.gas.iter_mut().find(|g| matches!(g, Gas::H2O(_)))
                        {
                            existing.extend(values.clone());
                        } else {
                            all_gas.gas.push(Gas::H2O(values.clone()));
                        }
                    }
                    Gas::N2O(values) => {
                        if let Some(Gas::N2O(existing)) =
                            all_gas.gas.iter_mut().find(|g| matches!(g, Gas::N2O(_)))
                        {
                            existing.extend(values.clone());
                        } else {
                            all_gas.gas.push(Gas::N2O(values.clone()));
                        }
                    }
                }
            }
        }
    }

    all_gas.sort();
    Ok(all_gas)
}

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

    for time in timev {
        for (chamber, start, close, open, end) in time.iter() {
            if start < &all_gas.datetime[0] || start > all_gas.datetime.last().unwrap() {
                continue;
            }

            let mut cycle = structs::CycleBuilder::new()
                .chamber_id(chamber.to_owned())
                .start_time(*start)
                .close_offset(*close)
                .open_offset(*open)
                .end_offset(*end)
                .build()?;

            if let Some(cur_data) = sorted_data.get(&start.format("%Y-%m-%d").to_string()) {
                let start_idx = match cur_data.datetime.binary_search_by(|t| {
                    if *t < cycle.start_time {
                        Ordering::Less
                    } else if *t > cycle.start_time {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                }) {
                    Ok(idx) => idx,                                   // Exact match found
                    Err(idx) => idx.min(cur_data.datetime.len() - 1), // Nearest match
                };

                // Find the index of `cycle.end_time`
                let end_idx = match cur_data.datetime.binary_search_by(|t| {
                    if *t < cycle.end_time {
                        Ordering::Less
                    } else if *t > cycle.end_time {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                }) {
                    Ok(idx) => idx,                    // Exact match found
                    Err(idx) => idx.saturating_sub(1), // Nearest match
                };

                // println!("Start Index: {}, End Index: {}", start_idx, end_idx);
                cycle.dt_v = cur_data.datetime[start_idx..end_idx].to_vec();
                cycle.diag_v = cur_data.diag[start_idx..end_idx].to_vec();
                for gas in &cur_data.gas {
                    match gas {
                        Gas::CH4(values) => {
                            cycle
                                .gas_data
                                .push(Gas::CH4(values[start_idx..end_idx].to_vec()));
                        }
                        Gas::CO2(values) => {
                            cycle
                                .gas_data
                                .push(Gas::CO2(values[start_idx..end_idx].to_vec()));
                        }
                        Gas::H2O(values) => {
                            cycle
                                .gas_data
                                .push(Gas::H2O(values[start_idx..end_idx].to_vec()));
                        }
                        // Gas::N2O(values) => {
                        //     cycle
                        //         .gas_data
                        //         .push(Gas::N2O(values[start_idx..end_idx].to_vec()));
                        // }
                        _ => {} // Skip if index is out of range
                    }
                }
                // cur_data
                //     .datetime
                //     .iter()
                //     .enumerate() // Get index to match gas values
                //     .filter(|(i, t)| *t >= &cycle.start_time && *t <= &cycle.end_time)
                //     .for_each(|(i, t)| {
                //         // println!("asd");
                //         cycle.dt_v.push(*t);
                //         cycle.diag_v.push(cur_data.diag[i]); // Use index `i` to fetch diag data
                //                                              // println!("{}", i);
                //
                //         // Extract gas values corresponding to the current timestamp index `i`
                //         let mut gas_entry = Vec::new();
                //
                //         // Ensure we extract data from each gas type correctly
                //         if let Some(Gas::CH4(values)) =
                //             cur_data.gas.iter().find(|g| matches!(g, Gas::CH4(_)))
                //         {
                //             if i < values.len() {
                //                 gas_entry.push(Gas::CH4(vec![values[i]]));
                //                 // println!("{:?}", values[i])
                //             }
                //         }
                //
                //         if let Some(Gas::CO2(values)) =
                //             cur_data.gas.iter().find(|g| matches!(g, Gas::CO2(_)))
                //         {
                //             if i < values.len() {
                //                 gas_entry.push(Gas::CO2(vec![values[i]]));
                //             }
                //         }
                //
                //         if let Some(Gas::H2O(values)) =
                //             cur_data.gas.iter().find(|g| matches!(g, Gas::H2O(_)))
                //         {
                //             if i < values.len() {
                //                 gas_entry.push(Gas::H2O(vec![values[i]]));
                //             }
                //         }
                //
                //         if let Some(Gas::N2O(values)) =
                //             cur_data.gas.iter().find(|g| matches!(g, Gas::N2O(_)))
                //         {
                //             if i < values.len() {
                //                 gas_entry.push(Gas::N2O(vec![values[i]]));
                //             }
                //         }
                //
                //         // Store the gas entry
                //         cycle.gas_data.extend(gas_entry);
                //         // Iterate through `Gas` enum and extract values for the corresponding index
                //         // for gas in &cur_data.gas {
                //         //     match gas {
                //         //         Gas::CH4(values) if i < values.len() => {
                //         //             cycle.gas_data.push(Gas::CH4(vec![values[i]]));
                //         //         }
                //         //         Gas::CO2(values) if i < values.len() => {
                //         //             cycle.gas_data.push(Gas::CO2(vec![values[i]]));
                //         //         }
                //         //         Gas::H2O(values) if i < values.len() => {
                //         //             cycle.gas_data.push(Gas::H2O(vec![values[i]]));
                //         //         }
                //         //         Gas::N2O(values) if i < values.len() => {
                //         //             cycle.gas_data.push(Gas::N2O(vec![values[i]]));
                //         //         }
                //         //         _ => {} // Skip if index is out of range
                //         //     }
                //         // }
                //     });

                // cur_data
                //     .datetime
                //     .iter()
                //     .zip(cur_data.gas.iter())
                //     .zip(cur_data.diag.iter())
                //     .filter(|((t, _), _)| t >= &&cycle.start_time && t <= &&cycle.end_time)
                //     .for_each(|((t, gas), dg)| {
                //         cycle.dt_v.push(*t);
                //         cycle.diag_v.push(*dg);

                //         match gas {
                //             Gas::CH4(values) => {
                //                 println!("{:?}", values);
                //                 let ch4_clone = values.clone();
                //                 cycle.gas_data.push(Gas::CH4(ch4_clone));
                //             }
                //             Gas::CO2(values) => {
                //                 println!("{:?}", values);
                //                 let co2_clone = values.clone();
                //                 cycle.gas_data.push(Gas::CO2(co2_clone));
                //             }
                //             Gas::H2O(values) => {
                //                 println!("{:?}", values);
                //                 let h2o_clone = values.clone();
                //                 cycle.gas_data.push(Gas::H2O(h2o_clone));
                //             }
                //             Gas::N2O(values) => {
                //                 println!("{:?}", values);
                //                 let n2o_clone = values.clone();
                //                 cycle.gas_data.push(Gas::N2O(n2o_clone));
                //             }
                //         }
                //     });
                // for gas in &cycle.gas_data {
                //     match gas {
                //         Gas::CH4(values) => {
                //             println!("{:?}", values.len())
                //         }
                //         Gas::CO2(values) => {
                //             println!("{:?}", values.len())
                //         }
                //         Gas::H2O(values) => {
                //             println!("{:?}", values.len())
                //         }
                //         Gas::N2O(values) => {
                //             println!("{:?}", values.len())
                //         }
                //     }
                // }
                cycle.get_peak_datetime();
                cycle.get_measurement_data();

                // NOTE: measurement_gas_data.is_empty() check is not valid
                // println!("Checking");
                if !cycle.measurement_dt_v.is_empty() && !cycle.measurement_gas_data.is_empty() {
                    for gas in &cycle.gas_data {
                        match gas {
                            Gas::CH4(values) => {
                                println!("{:?}", values.len())
                            }
                            Gas::CO2(values) => {
                                // println!("{:?}", values.len())
                            }
                            Gas::H2O(values) => {
                                // println!("{:?}", values.len())
                            }
                            Gas::N2O(values) => {
                                // println!("{:?}", values.len())
                            }
                        }
                    }
                    // println!("Valid");
                    cycle.calculate_measurement_r();
                    cycle.find_highest_r_window();
                    // cycle.calculate_flux();
                    cycle.summary();
                    cycle_vec.push(cycle);
                }
            }
        }
    }

    Ok(cycle_vec)
}

fn prepare_plot_data(cycle: &structs::Cycle) -> Vec<[f64; 2]> {
    cycle
        .dt_v_as_float()
        .iter()
        .copied()
        .zip(cycle.gas_data.iter().flat_map(|gas| match gas {
            Gas::CH4(values) => values.clone(),
            Gas::CO2(values) => values.clone(),
            Gas::H2O(values) => values.clone(),
            Gas::N2O(values) => values.clone(),
        }))
        .map(|(x, y)| [x, y])
        .collect()
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
