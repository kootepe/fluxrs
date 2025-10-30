use crate::instruments::InstrumentType;
use crate::processevent::{ProcessEvent, QueryEvent};
use crate::traits::EqualLen;
use csv::Writer;
use cycle::Cycle;
use rusqlite::{Connection, Result};
use std::error::Error;
use std::fs::File;
use std::process;
use tokio::sync::mpsc;
use ui::project_ui::Project;

pub mod cmd;
pub mod concentrationunit;
pub mod constants;
pub mod cycle;
pub mod cycle_navigator;
pub mod data_formats;
pub mod errorcode;
pub mod flux;
pub mod fluxes_schema;
mod gas_plot;
pub mod gaschannel;
pub mod gastype;
mod get_paths;
mod html_report;
mod index;
mod instruments;
mod keybinds;
pub mod utils;
// pub mod meteodata;
pub mod processevent;
mod stats;
pub mod traits;
pub mod ui;

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

// fn _group_gas_data_by_date(gas_data: &GasData) -> HashMap<String, GasData> {
//     let mut grouped_data: HashMap<String, GasData> = HashMap::new();
//
//     for (i, (dt, diag)) in gas_data.datetime.iter().zip(&gas_data.diag).enumerate() {
//         let date_key = dt.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
//
//         // Get or insert the daily GasData entry
//         let entry = grouped_data.entry(date_key).or_insert_with(|| GasData {
//             header: gas_data.header.clone(),
//             instrument_model: String::new(),
//             instrument_serial: String::new(),
//             datetime: Vec::new(),
//             gas: HashMap::new(),
//             diag: Vec::new(),
//         });
//
//         // Add values to the daily entry
//         entry.datetime.push(*dt);
//         entry.diag.push(*diag);
//
//         for (gas_type, values) in &gas_data.gas {
//             if let Some(value) = values.get(i) {
//                 entry.gas.entry(*gas_type).or_insert_with(Vec::new).push(*value);
//             }
//         }
//         // entry.gas.get(GasData::CH4).len();
//         // println!("{:?}", (entry.gas.get(GasType::CH4).len()));
//     }
//
//     grouped_data
// }

// fn _query_and_group_gas_data(
//     conn: &Connection,
//     start_timestamp: i64,
//     end_timestamp: i64,
// ) -> Result<HashMap<String, GasData>> {
//     let mut stmt = conn.prepare(
//         "SELECT datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model FROM measurements
//          WHERE datetime BETWEEN ?1 AND ?2
//          ORDER BY datetime",
//     )?;
//
//     let mut grouped_data: HashMap<String, GasData> = HashMap::new();
//
//     let rows = stmt.query_map(params![start_timestamp, end_timestamp], |row| {
//         let datetime_unix: i64 = row.get(0)?;
//         let ch4: Option<f64> = row.get(1)?; // Handle NULL values
//         let co2: Option<f64> = row.get(2)?;
//         let h2o: Option<f64> = row.get(3)?;
//         let n2o: Option<f64> = row.get(4)?;
//         let diag: i64 = row.get(5)?;
//         let serial: String = row.get(6)?;
//         let model: String = row.get(7)?;
//
//         let naive_datetime =
//             NaiveDateTime::from_timestamp_opt(datetime_unix, 0).expect("Invalid timestamp");
//         let datetime_utc = DateTime::<Utc>::from_utc(naive_datetime, Utc);
//
//         Ok((datetime_utc, ch4, co2, h2o, n2o, diag, serial, model))
//     })?;
//
//     for row in rows {
//         let (datetime, ch4, co2, h2o, n2o, diag, serial, model) = row?;
//
//         //   Extract YYYY-MM-DD for grouping
//         let date_key = datetime.format("%Y-%m-%d").to_string();
//
//         //   Get or create a new GasData entry
//         let entry = grouped_data.entry(date_key.clone()).or_insert_with(|| GasData {
//             header: StringRecord::new(),
//             instrument_model: String::new(),
//             instrument_serial: String::new(),
//             datetime: Vec::new(),
//             gas: HashMap::new(),
//             diag: Vec::new(),
//         });
//
//         //   Append values
//         entry.datetime.push(datetime);
//         entry.diag.push(diag);
//         entry.instrument_model = model;
//         entry.instrument_serial = serial.clone();
//         // entry.instrument_model.push(instrument_model);
//         // entry.instrument_serial.push(instrument_serial);
//
//         //   Store each gas type in the `HashMap`
//         entry.gas.entry((GasType::CH4, serial.clone())).or_default().push(ch4);
//         entry.gas.entry((GasType::CO2, serial.clone())).or_default().push(co2);
//         entry.gas.entry((GasType::H2O, serial.clone())).or_default().push(h2o);
//         entry.gas.entry((GasType::N2O, serial.clone())).or_default().push(n2o);
//     }
//
//     Ok(grouped_data)
// }

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

// fn get_gas_data(path: &str) -> Result<GasData, Box<dyn Error>> {
//     let gas_paths = get_paths::get_paths(path.to_owned(), "gas")?;
//     let mut all_gas = GasData::new();
//
//     for path in gas_paths {
//         let instrument = Li7810::default();
//         println!("{:?}", path);
//         let res = instrument.read_data_file(path)?;
//
//         if res.validate_lengths() && !res.any_col_invalid() {
//             all_gas.datetime.extend(res.datetime);
//             all_gas.diag.extend(res.diag);
//
//             // Merge gas values correctly
//             for (gas_type, values) in res.gas {
//                 all_gas.gas.entry(gas_type).or_insert_with(Vec::new).extend(values);
//             }
//         }
//     }
//
//     all_gas.sort();
//     Ok(all_gas)
// }

// fn _sort_and_group_gas(all_gas: &GasData) -> HashMap<String, GasData> {
//     _group_gas_data_by_date(all_gas)
// }
// pub fn init_from_db(
//     start: String,
//     end: String,
//     db: String,
// ) -> Result<Vec<Cycle>, Box<dyn Error>> {
//     Ok(Vec::from())
// }

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
