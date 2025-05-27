use crate::constants::{ERROR_FLOAT, ERROR_INT};
use crate::EqualLen;
use chrono::prelude::DateTime;
use chrono::{NaiveDateTime, Utc};
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use tokio::task;

use csv::StringRecord;

use crate::instruments::GasType;

#[derive(Clone)]
pub struct GasData {
    pub header: StringRecord,
    pub instrument_model: String,
    pub instrument_serial: String,
    pub datetime: Vec<DateTime<Utc>>,
    // pub gas: HashMapnew(),
    pub gas: HashMap<GasType, Vec<Option<f64>>>,
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
            instrument_model: String::new(),
            instrument_serial: String::new(),
            datetime: Vec::new(),
            gas: HashMap::new(),
            diag: Vec::new(),
        }
    }
    pub fn any_col_invalid(&self) -> bool {
        // Check if all values in any vector are equal to the error value
        let gas_invalid = self.gas.values().any(|v| v.iter().all(|&x| x.is_none()));
        let diag_invalid = self.diag.iter().all(|&x| x == ERROR_INT);

        gas_invalid || diag_invalid
    }

    pub fn print_gasdata_lengths(&self) {
        println!("datetime length: {}", self.datetime.len());
        println!("diag length: {}", self.diag.len());

        for (gas_type, values) in &self.gas {
            println!("{:?} gas length: {}", gas_type, values.len());
        }
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
                *values = indices.iter().filter_map(|&i| values.get(i).copied()).collect();
            } else {
                eprintln!("Warning: Mismatched lengths during sorting, skipping gas type sorting.");
            }
        }
    }
}
pub async fn query_gas_async(
    conn: Arc<Mutex<Connection>>, // Arc<Mutex> for shared async access
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
    instrument_serial: String,
) -> Result<HashMap<String, GasData>> {
    // let start_ts = start.timestamp();
    // let end_ts = end.timestamp();

    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_gas(&conn, start, end, project, instrument_serial)
    })
    .await;
    match result {
        Ok(inner) => inner,
        Err(e) => {
            // Convert JoinError into rusqlite::Error::ExecuteReturnedResults or custom error
            Err(rusqlite::Error::ExecuteReturnedResults) // or log `e` if needed
        },
    }
}
pub fn query_gas2(
    conn: &Connection,
    start: i64,
    end: i64,
    project: String,
) -> Result<HashMap<String, GasData>> {
    // let mut data = HashMap::new();
    println!("Querying gas data");
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model
             FROM measurements
             WHERE datetime BETWEEN ?1 AND ?2
             and project_id = ?3
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(params![start, end, project], |row| {
        let datetime_unix: i64 = row.get(0)?;
        let ch4: Option<f64> = row.get(1)?; // Handle NULL values
        let co2: Option<f64> = row.get(2)?;
        let h2o: Option<f64> = row.get(3)?;
        let n2o: Option<f64> = row.get(4)?;
        let diag: i64 = row.get(5)?;
        let instrument_serial: Option<String> = row.get(6)?;
        let instrument_model: Option<String> = row.get(7)?;

        // Convert UNIX timestamp to DateTime<Utc>

        let utc_datetime: DateTime<Utc> =
            chrono::DateTime::from_timestamp(datetime_unix, 0).unwrap();

        Ok((utc_datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model))
    })?;

    for row in rows {
        let (datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model) = row?;

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

        entry.gas.entry(GasType::CH4).or_default().push(ch4);
        entry.gas.entry(GasType::CO2).or_default().push(co2);
        entry.gas.entry(GasType::H2O).or_default().push(h2o);
        entry.gas.entry(GasType::N2O).or_default().push(n2o);

        //   Append values
        entry.datetime.push(datetime);
        entry.diag.push(diag);
        entry.instrument_model = instrument_model.unwrap();
        entry.instrument_serial = instrument_serial.unwrap();

        //   Store each gas type in the `HashMap`
    }
    Ok(grouped_data)
}
pub fn query_gas(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
    instrument_serial: String,
) -> Result<HashMap<String, GasData>> {
    // let mut data = HashMap::new();
    println!("Querying gas data");
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model
             FROM measurements
             WHERE datetime BETWEEN ?1 AND ?2
             and project_id = ?3 AND instrument_serial = ?4
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp(), end.timestamp(), project, instrument_serial],
        |row| {
            let datetime_unix: i64 = row.get(0)?;
            let ch4: Option<f64> = row.get(1)?; // Handle NULL values
            let co2: Option<f64> = row.get(2)?;
            let h2o: Option<f64> = row.get(3)?;
            let n2o: Option<f64> = row.get(4)?;
            let diag: i64 = row.get(5)?;
            let instrument_serial: Option<String> = row.get(6)?;
            let instrument_model: Option<String> = row.get(7)?;

            // Convert UNIX timestamp to DateTime<Utc>

            let utc_datetime: DateTime<Utc> =
                chrono::DateTime::from_timestamp(datetime_unix, 0).unwrap();

            Ok((utc_datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model))
        },
    )?;

    for row in rows {
        let (datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model) = row?;

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

        entry.gas.entry(GasType::CH4).or_default().push(ch4);
        entry.gas.entry(GasType::CO2).or_default().push(co2);
        entry.gas.entry(GasType::H2O).or_default().push(h2o);
        entry.gas.entry(GasType::N2O).or_default().push(n2o);

        //   Append values
        entry.datetime.push(datetime);
        entry.diag.push(diag);
        entry.instrument_model = instrument_model.unwrap();
        entry.instrument_serial = instrument_serial.unwrap();

        //   Store each gas type in the `HashMap`
    }
    Ok(grouped_data)
}
pub fn query_gas_all(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
    instrument_serial: String,
) -> Result<GasData> {
    // let mut data = HashMap::new();
    println!("Querying gas data");

    let mut stmt = conn.prepare(
        "SELECT datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model
             FROM measurements
             WHERE datetime BETWEEN ?1 AND ?2
             and project_id = ?3 AND instrument_serial = ?4
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp(), end.timestamp(), project, instrument_serial],
        |row| {
            let datetime_unix: i64 = row.get(0)?;
            let ch4: Option<f64> = row.get(1)?; // Handle NULL values
            let co2: Option<f64> = row.get(2)?;
            let h2o: Option<f64> = row.get(3)?;
            let n2o: Option<f64> = row.get(4)?;
            let diag: i64 = row.get(5)?;
            let instrument_serial: Option<String> = row.get(6)?;
            let instrument_model: Option<String> = row.get(7)?;

            // Convert UNIX timestamp to DateTime<Utc>
            let utc_datetime = chrono::DateTime::from_timestamp(datetime_unix, 0).unwrap().to_utc();
            Ok((utc_datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model))
        },
    )?;
    let mut entry = GasData {
        header: StringRecord::new(),
        instrument_model: String::new(),
        instrument_serial: String::new(),
        datetime: Vec::new(),
        gas: HashMap::new(),
        diag: Vec::new(),
    };
    for row in rows {
        let (datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model) = row?;

        //   Append values
        entry.datetime.push(datetime);
        entry.diag.push(diag);
        entry.instrument_model = instrument_model.unwrap();
        entry.instrument_serial = instrument_serial.unwrap();

        entry.gas.entry(GasType::CH4).or_default().push(ch4);
        entry.gas.entry(GasType::CO2).or_default().push(co2);
        entry.gas.entry(GasType::H2O).or_default().push(h2o);
        entry.gas.entry(GasType::N2O).or_default().push(n2o);
    }
    Ok(entry)
}
pub fn insert_measurements(
    conn: &mut Connection,
    all_gas: &GasData,
    project: String,
) -> Result<(usize, usize)> {
    let diag_vec = &all_gas.diag;
    let datetime_vec = all_gas.datetime.iter().map(|dt| dt.timestamp()).collect::<Vec<i64>>();

    let ch4_vec = all_gas.gas.get(&GasType::CH4).unwrap();
    let co2_vec = all_gas.gas.get(&GasType::CO2).unwrap();
    let h2o_vec = all_gas.gas.get(&GasType::H2O).unwrap();

    if datetime_vec.len() != ch4_vec.len()
        || datetime_vec.len() != co2_vec.len()
        || datetime_vec.len() != h2o_vec.len()
    {
        println!("Error: Mismatched data lengths");
        return Err(rusqlite::Error::InvalidQuery); // Ensure equal-length data
    }

    let tx = conn.transaction()?;
    let mut duplicates = 0;
    let mut inserted = 0;

    // Prepare the statement for insertion
    let mut stmt = tx.prepare(
        "INSERT OR IGNORE INTO measurements (datetime, ch4, co2, h2o, diag, instrument_serial, instrument_model, project_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;

    println!("Pushing data!");
    for i in 0..datetime_vec.len() {
        // Check for duplicates first
        // let mut check_stmt = tx
        //     .prepare("SELECT 1 FROM measurements WHERE datetime = ?1 AND instrument_serial = ?2 AND project_id = ?3")?;
        // let mut rows =
        //     check_stmt.query(params![datetime_vec[i], all_gas.instrument_serial, project])?;
        stmt.execute(params![
            datetime_vec[i],           //   Individual timestamp
            ch4_vec[i],                //   Individual CH4 value
            co2_vec[i],                //   Individual CO2 value
            h2o_vec[i],                //   Individual H2O value
            diag_vec[i],               //   Individual diag value
            all_gas.instrument_serial, // Example: Serial number (Replace with actual value)
            all_gas.instrument_model,  // Example: Instrument model
            project
        ])?;
        inserted += 1;
        // if rows.next()?.is_some() {
        //     // If a duplicate exists, log it
        //     duplicates += 1;
        //     // println!(
        //     //     "Warning: Duplicate record found for datetime: {} and instrument_serial: {}",
        //     //     datetime_vec[i], all_gas.instrument_serial
        //     // );
        // } else {
        //     // If no duplicate, insert the new record
        //     stmt.execute(params![
        //         datetime_vec[i],           //   Individual timestamp
        //         ch4_vec[i],                //   Individual CH4 value
        //         co2_vec[i],                //   Individual CO2 value
        //         h2o_vec[i],                //   Individual H2O value
        //         diag_vec[i],               //   Individual diag value
        //         all_gas.instrument_serial, // Example: Serial number (Replace with actual value)
        //         all_gas.instrument_model,  // Example: Instrument model
        //         project
        //     ])?;
        //     inserted += 1;
        // }
    }

    drop(stmt);
    tx.commit()?;

    // Print how many rows were inserted and how many were duplicates
    println!("Inserted {} rows into measurements, {} duplicates skipped.", inserted, duplicates);

    Ok((inserted, duplicates))
}
