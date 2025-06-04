use crate::gasdata::{query_gas, GasData};
use crate::instruments::InstrumentType;
use crate::meteodata::MeteoData;
use crate::processevent::{InsertEvent, ProcessEvent, ProgressEvent, QueryEvent, ReadEvent};
use crate::project_app::Project;
use crate::timedata::TimeData;
use crate::traits::EqualLen;
use crate::volumedata::VolumeData;
use chrono::{DateTime, NaiveDateTime, Utc};
use csv::StringRecord;
use csv::Writer;
use cycle::{Cycle, CycleBuilder};
use instruments::GasType;
use instruments::Li7810;
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::process;
use tokio::sync::mpsc;

pub mod app_plotting;
mod config;
pub mod constants;
mod csv_parse;
pub mod cycle;
pub mod cycle_navigator;
pub mod errorcode;
pub mod flux;
pub mod fluxes_schema;
mod gas_plot;
pub mod gasdata;
mod get_paths;
mod html_report;
mod index;
mod instruments;
mod keybinds;
pub mod meteodata;
pub mod myapp;
pub mod processevent;
pub mod project_app;
pub mod query;
mod stats;
pub mod timedata;
pub mod traits;
mod validation_app;
pub mod volumedata;

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
        Flux { datetime: Vec::new(), flux: Vec::new(), r: Vec::new(), chamber_id: Vec::new() }
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

fn _group_gas_data_by_date(gas_data: &GasData) -> HashMap<String, GasData> {
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    for (i, (dt, diag)) in gas_data.datetime.iter().zip(&gas_data.diag).enumerate() {
        let date_key = dt.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD

        // Get or insert the daily GasData entry
        let entry = grouped_data.entry(date_key).or_insert_with(|| GasData {
            header: gas_data.header.clone(),
            instrument_model: String::new(),
            instrument_serial: String::new(),
            datetime: Vec::new(),
            gas: HashMap::new(),
            diag: Vec::new(),
        });

        // Add values to the daily entry
        entry.datetime.push(*dt);
        entry.diag.push(*diag);

        for (gas_type, values) in &gas_data.gas {
            if let Some(value) = values.get(i) {
                entry.gas.entry(*gas_type).or_insert_with(Vec::new).push(*value);
            }
        }
        // entry.gas.get(GasData::CH4).len();
        // println!("{:?}", (entry.gas.get(GasType::CH4).len()));
    }

    grouped_data
}

pub struct Config {
    pub gas_path: String,
    pub time_path: String,
    pub db_path: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub R_LIM: f64,
}

impl Config {
    pub fn build(mut args: impl Iterator<Item = String>) -> Result<Config, &'static str> {
        const R_LIM: f64 = 0.999;
        args.next(); // Skip the first argument (program name)

        let mut gas_path = String::new();
        let mut time_path = String::new();
        let mut db_path: Option<String> = None;
        let mut start: Option<String> = None;
        let mut end: Option<String> = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-db" => {
                    db_path = args.next();
                },
                "-s" => {
                    start = args.next();
                },
                "-e" => {
                    end = args.next();
                },
                _ if gas_path.is_empty() => gas_path = arg.clone(),
                _ if time_path.is_empty() => time_path = arg.clone(),
                _ => {}, // Ignore unknown arguments
            }
        }

        Ok(Config { gas_path, time_path, db_path, start, end, R_LIM })
    }
}

fn insert_cycles(
    conn: &mut Connection,
    cycles: &TimeData,
    project: String,
) -> Result<(usize, usize)> {
    let close_vec = &cycles.close_offset;
    let open_vec = &cycles.open_offset;
    let end_vec = &cycles.end_offset;
    let chamber_vec = &cycles.chamber_id;
    let start_vec = cycles.start_time.iter().map(|dt| dt.timestamp()).collect::<Vec<i64>>();

    let tx = conn.transaction()?;
    let mut duplicates = 0;
    let mut inserted = 0;
    // let site_id = "oulanka_fen"; // Example site

    //   Prepare the statements **before** the loop
    let mut insert_stmt = tx.prepare(
        "INSERT INTO cycles (start_time, close_offset, open_offset, end_offset, chamber_id, project_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;

    let mut check_stmt = tx.prepare(
        "SELECT 1 FROM cycles WHERE start_time = ?1 AND chamber_id = ?2 AND project_id = ?3",
    )?;

    println!("Pushing data!");
    for i in 0..start_vec.len() {
        // Check for duplicates first
        let mut rows = check_stmt.query(params![start_vec[i], chamber_vec[i], project])?;
        if rows.next()?.is_some() {
            //   Duplicate found, skip insert
            duplicates += 1;
            println!(
                "Warning: Duplicate record found for start_time: {}, chamber_id: {}, site: {}",
                start_vec[i], chamber_vec[i], project
            );
        } else {
            //   Insert new record
            insert_stmt.execute(params![
                start_vec[i],
                close_vec[i],
                open_vec[i],
                end_vec[i],
                chamber_vec[i],
                project,
            ])?;
            inserted += 1;
        }
    }

    drop(insert_stmt);
    drop(check_stmt);

    tx.commit()?;
    println!("Inserted {} rows into cycles, {} duplicates skipped.", inserted, duplicates);

    Ok((inserted, duplicates))
}

fn _query_and_group_gas_data(
    conn: &Connection,
    start_timestamp: i64,
    end_timestamp: i64,
) -> Result<HashMap<String, GasData>> {
    let mut stmt = conn.prepare(
        "SELECT datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model FROM measurements
         WHERE datetime BETWEEN ?1 AND ?2
         ORDER BY datetime",
    )?;

    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    let rows = stmt.query_map(params![start_timestamp, end_timestamp], |row| {
        let datetime_unix: i64 = row.get(0)?;
        let ch4: Option<f64> = row.get(1)?; // Handle NULL values
        let co2: Option<f64> = row.get(2)?;
        let h2o: Option<f64> = row.get(3)?;
        let n2o: Option<f64> = row.get(4)?;
        let diag: i64 = row.get(5)?;
        let serial: String = row.get(6)?;
        let model: String = row.get(7)?;

        let naive_datetime =
            NaiveDateTime::from_timestamp_opt(datetime_unix, 0).expect("Invalid timestamp");
        let datetime_utc = DateTime::<Utc>::from_utc(naive_datetime, Utc);

        Ok((datetime_utc, ch4, co2, h2o, n2o, diag, serial, model))
    })?;

    for row in rows {
        let (datetime, ch4, co2, h2o, n2o, diag, serial, model) = row?;

        //   Extract YYYY-MM-DD for grouping
        let date_key = datetime.format("%Y-%m-%d").to_string();

        //   Get or create a new GasData entry
        let entry = grouped_data.entry(date_key.clone()).or_insert_with(|| GasData {
            header: StringRecord::new(),
            instrument_model: String::new(),
            instrument_serial: String::new(),
            datetime: Vec::new(),
            gas: HashMap::new(),
            diag: Vec::new(),
        });

        //   Append values
        entry.datetime.push(datetime);
        entry.diag.push(diag);
        entry.instrument_model = model;
        entry.instrument_serial = serial;
        // entry.instrument_model.push(instrument_model);
        // entry.instrument_serial.push(instrument_serial);

        //   Store each gas type in the `HashMap`
        entry.gas.entry(GasType::CH4).or_default().push(ch4);
        entry.gas.entry(GasType::CO2).or_default().push(co2);
        entry.gas.entry(GasType::H2O).or_default().push(h2o);
        entry.gas.entry(GasType::N2O).or_default().push(n2o);
    }

    Ok(grouped_data)
}

pub fn initiate_db() -> Result<(), Box<dyn std::error::Error>> {
    println!("Initiating db.");
    let conn = Connection::open("fluxrs.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cycles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            chamber_id TEXT NOT NULL,
            start_time integer NOT NULL,
            close_offset integer NOT NULL,
            open_offset integer NOT NULL,
            end_offset integer NOT NULL,
            site TEXT NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS measurements (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            datetime integer NOT NULL,
            ch4 float,
            co2 float,
            h2o float,
            n2o float,
            diag integer
        )",
        [],
    )?;
    // insert_measurements(&mut conn, gases)?;
    // insert_cycles(&mut conn, times)?;
    Ok(())
}

// pub fn run(config: Config) -> Result<Vec<Cycle>, Box<dyn Error>> {
//     let gases = get_gas_data(&config.gas_path)?;
//     let times = get_time_data(&config.time_path)?;
//     initiate_db(&gases, &times)?;
//     println!("Sorting and grouping data...");
//     // let sorted_data = sort_and_group_gas(&all_gas);
//     // let sorted_data = group_gas_data_by_date(&gases);
//
//     println!("Processing cycles");
//
//     let st = match config.start {
//         None => Utc::now(),
//         Some(s) => {
//             let naive_datetime = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d%H%M%S").unwrap();
//             println!("{:?}", naive_datetime);
//             DateTime::<Utc>::from_utc(naive_datetime, Utc)
//         },
//     };
//     let en = match config.end {
//         None => Utc::now(),
//         Some(s) => {
//             let naive_datetime = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d%H%M%S")
//                 .expect("Failed to parse NaiveDateTime");
//             println!("{:?}", naive_datetime);
//             DateTime::<Utc>::from_utc(naive_datetime, Utc)
//         },
//     };
//     let conn = Connection::open("fluxrs.db")?;
//     let times = query_cycles(&conn, st, en)?;
//     let gas_data = query_gas(&conn, st, en)?;
//     let cycle_vec = process_cycles(&times, &gas_data)?;
//
//     println!("Calculated {} cycles.", cycle_vec.len());
//
//     Ok(cycle_vec)
// }

fn get_gas_data(path: &str) -> Result<GasData, Box<dyn Error>> {
    let gas_paths = get_paths::get_paths(path.to_owned(), "gas")?;
    let mut all_gas = GasData::new();

    for path in gas_paths {
        let instrument = Li7810::default();
        println!("{:?}", path);
        let res = instrument.read_data_file(path)?;

        if res.validate_lengths() && !res.any_col_invalid() {
            all_gas.datetime.extend(res.datetime);
            all_gas.diag.extend(res.diag);

            // Merge gas values correctly
            for (gas_type, values) in res.gas {
                all_gas.gas.entry(gas_type).or_insert_with(Vec::new).extend(values);
            }
        }
    }

    all_gas.sort();
    Ok(all_gas)
}

fn get_time_data(path: &str) -> Result<TimeData, Box<dyn Error>> {
    let time_paths = get_paths::get_paths(path.to_owned(), "time")?;
    let mut all_times = TimeData::new();
    for path in time_paths {
        let res = csv_parse::read_time_csv(&path)?;
        if res.validate_lengths() {
            all_times.chamber_id.extend(res.chamber_id);
            all_times.start_time.extend(res.start_time);
            all_times.close_offset.extend(res.close_offset);
            all_times.open_offset.extend(res.open_offset);
            all_times.end_offset.extend(res.end_offset);
            // timev.push(res);
        }
    }
    Ok(all_times)
}

fn _sort_and_group_gas(all_gas: &GasData) -> HashMap<String, GasData> {
    _group_gas_data_by_date(all_gas)
}
// pub fn init_from_db(
//     start: String,
//     end: String,
//     db: String,
// ) -> Result<Vec<Cycle>, Box<dyn Error>> {
//     Ok(Vec::from())
// }
fn query_cycles_within_timerange(
    conn: &Connection,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<Vec<Cycle>, rusqlite::Error> {
    // pub fn _to_html_row(&self) -> Result<String, Box<dyn Error>> {
    let start_timestamp = start_time.timestamp(); // Convert to i64 (UNIX time)
    let end_timestamp = end_time.timestamp();

    let mut stmt = conn.prepare(
        "SELECT chamber_id, start_time, close_offset, open_offset, end_offset, site
         FROM cycles
         WHERE start_time BETWEEN ?1 AND ?2",
    )?;

    let cycle_iter = stmt.query_map(params![start_timestamp, end_timestamp], |row| {
        CycleBuilder::new()
            .chamber_id(row.get(0)?) // chamber_id as String
            .start_time(DateTime::from_timestamp(row.get(1)?, 0).unwrap()) // start_time as i64 (UNIX timestamp)
            .close_offset(row.get(2)?) // close_offset as i32
            .open_offset(row.get(3)?) // open_offset as i32
            .end_offset(row.get(4)?) // end_offset as i32
            .build_db()
    })?;

    cycle_iter.collect::<Result<Vec<_>, _>>()
}

fn process_cycles(
    timev: &TimeData,
    sorted_data: &HashMap<String, GasData>,
    meteo_data: &MeteoData,
    volume_data: &VolumeData,
    project: Project,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) -> Result<Vec<Option<Cycle>>, Box<dyn Error + Send + Sync>> {
    // println!("Processing cycles");
    let mut cycle_vec = Vec::new();
    let mut no_data_for_day = false;
    let mut last_date = chrono::offset::Utc::now().format("%Y-%m-%d").to_string();
    let mut processed_cycles: u32 = 0;
    for (chamber, start, close, open, end, _) in timev.iter() {
        let mut cycle = CycleBuilder::new()
            .chamber_id(chamber.to_owned())
            .start_time(*start)
            .close_offset(*close)
            .open_offset(*open)
            .end_offset(*end)
            .project_name(project.name.to_owned())
            .min_calc_len(project.min_calc_len)
            .build()?;
        let st = cycle.start_time;
        let day = st.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
        if no_data_for_day && last_date == day {
            continue;
        } else {
            no_data_for_day = false;
        }

        // if day != last_date {
        //     println!("Processed {}/{} cycles from {}", processed_cycles, total_cycles, day);
        // }
        last_date = day.clone();

        if let Some(cur_data) = sorted_data.get(&start.format("%Y-%m-%d").to_string()) {
            processed_cycles += 1;
            // cur_data is ordered, so we can check last and first timestamp to skip cycles
            // with no data
            if start < &cur_data.datetime[0] || start > cur_data.datetime.last().unwrap() {
                continue;
            }
            let end_time = DateTime::from_timestamp(cycle.get_end() as i64, 0).unwrap();
            cur_data.datetime.iter().enumerate().for_each(|(i, t)| {
                // println!("{}", t);
                if t >= &cycle.start_time && t <= &end_time {
                    cycle.dt_v.push(t.timestamp() as f64);
                    for (gas_type, gas_values) in &cur_data.gas {
                        if let Some(value) = gas_values.get(i) {
                            cycle.gas_v.entry(*gas_type).or_insert_with(Vec::new).push(*value);
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

            let gases: Vec<_> = cur_data.gas.keys().cloned().collect(); // Collect first
            cycle.gases = gases.clone();
            cycle.instrument_model = InstrumentType::from_str(&cur_data.instrument_model.clone());
            cycle.instrument_serial = cur_data.instrument_serial.clone();

            let target =
                (cycle.start_time + chrono::TimeDelta::seconds(cycle.close_offset)).timestamp();
            let (temp, pressure) = match meteo_data.get_nearest(target) {
                Some((temp, pressure)) => (temp, pressure),
                None => (10.0, 1000.0),
            };
            cycle.air_temperature = temp;
            cycle.air_pressure = pressure;

            let volume =
                volume_data.get_nearest_previous_volume(target, &cycle.chamber_id).unwrap_or(1.0);
            cycle.chamber_volume = volume;
            for gas in gases {
                cycle.deadbands.insert(gas, project.deadband);
            }
            // cycle.deadbands = project_deadband;
            cycle.reset();
            if cycle.dt_v.is_empty() {
                let _ = progress_sender.send(ProcessEvent::Query(QueryEvent::NoGasData(format!(
                    "{}",
                    cycle.start_time
                ))));
                // progress_sender.send(ProcessEvent::NoGasData(format!("{}", cycle.start_time)));
                cycle_vec.push(None);
            } else {
                cycle_vec.push(Some(cycle));
            }
        } else {
            no_data_for_day = true;
            progress_sender.send(ProcessEvent::Query(QueryEvent::NoGasDataDay(format!(
                "{}",
                &start.format("%Y-%m-%d").to_string()
            ))))?;
            continue;
        }
    }
    Ok(cycle_vec)
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
