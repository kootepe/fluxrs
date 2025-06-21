use crate::constants::ERROR_INT;
use crate::gastype::GasType;
use crate::project_app::Project;
use crate::validation_app::GasKey;
use crate::EqualLen;
use chrono::prelude::DateTime;
use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::task;

use csv::StringRecord;

use crate::instruments::InstrumentType;

#[derive(Clone, Debug)]
pub struct GasData {
    pub header: StringRecord,
    pub instrument_model: String,
    pub instrument_serial: String,
    pub model_key: HashMap<String, InstrumentType>,
    pub datetime: HashMap<String, Vec<DateTime<Utc>>>,
    pub gas: HashMap<GasKey, Vec<Option<f64>>>,
    pub diag: HashMap<String, Vec<i64>>,
}
// impl fmt::Debug for GasData {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         writeln!(
//             f,
//             "start: {}, \nstart_len: {} diag_len: {}",
//             self.datetime[0],
//             self.datetime.len(),
//             self.diag.len(),
//         )?;
//
//         for (gas_type, values) in &self.gas {
//             writeln!(f, "{:?}: {:?}", gas_type, values.len())?;
//         }
//
//         Ok(())
//     }
// }

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
            model_key: HashMap::new(),
            datetime: HashMap::new(),
            gas: HashMap::new(),
            diag: HashMap::new(),
        }
    }
    pub fn any_col_invalid(&self) -> bool {
        // Check if any gas vector has all values as None
        let gas_invalid = self.gas.values().any(|v| v.iter().all(|&x| x.is_none()));

        // Check if any diag vector has all values equal to ERROR_INT
        let diag_invalid = self.diag.values().any(|v| v.iter().all(|&x| x == ERROR_INT));

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

    // pub fn summary(&self) {
    //     println!("dt: {} len: {}", self.datetime[0], self.diag.len());
    // }
    // pub fn sort(&mut self) {
    //     let mut indices: Vec<usize> = (0..self.datetime.len()).collect();
    //     indices.sort_by(|&i, &j| self.datetime[i].cmp(&self.datetime[j]));
    //
    //     self.datetime = indices.iter().map(|&i| self.datetime[i]).collect();
    //     self.diag = indices.iter().map(|&i| self.diag[i]).collect();
    //
    //     // Sort each gas type in the HashMap safely
    //     for values in self.gas.values_mut() {
    //         if values.len() == self.datetime.len() {
    //             // Ensure lengths match before sorting
    //             *values = indices.iter().filter_map(|&i| values.get(i).copied()).collect();
    //         } else {
    //             eprintln!("Warning: Mismatched lengths during sorting, skipping gas type sorting.");
    //         }
    //     }
    // }
}
pub async fn query_gas_async(
    conn: Arc<Mutex<Connection>>, // Arc<Mutex> for shared async access
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: Project,
) -> Result<HashMap<(String), GasData>> {
    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_gas2(&conn, start.timestamp(), end.timestamp(), project.name)
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
    println!("Querying gas data");
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    // Static columns must match dynamic index logic below
    let mut stmt = conn.prepare(
        "SELECT datetime, co2, ch4, h2o, n2o, diag, instrument_serial, instrument_model
         FROM measurements
         WHERE datetime BETWEEN ?1 AND ?2
         AND project_id = ?3
         ORDER BY instrument_serial, datetime",
    )?;

    let rows = stmt.query_map(params![start, end, project], |row| {
        let datetime_unix: i64 = row.get(0)?;
        let diag: i64 = row.get(5)?;
        let instrument_serial: Option<String> = row.get(6)?;
        let instrument_model: Option<String> = row.get(7)?;

        let utc_datetime: DateTime<Utc> =
            chrono::DateTime::from_timestamp(datetime_unix, 0).unwrap();

        // Collect gas values into a Vec<Option<f64>>
        let gases = vec![
            row.get::<_, Option<f64>>(1)?,
            row.get::<_, Option<f64>>(2)?,
            row.get::<_, Option<f64>>(3)?,
            row.get::<_, Option<f64>>(4)?,
        ];

        Ok((utc_datetime, gases, diag, instrument_serial, instrument_model))
    })?;

    for row in rows {
        let (datetime, gas_values, diag, instrument_serial, instrument_model) = row?;
        let date_key = datetime.format("%Y-%m-%d").to_string();

        let serial = instrument_serial.clone().unwrap();
        let model = instrument_model.clone().unwrap();
        let instrument_type = InstrumentType::from_str(&model);
        let available_gases = instrument_type.available_gases();

        let entry = grouped_data.entry(date_key.clone()).or_insert_with(|| GasData {
            header: StringRecord::new(),
            instrument_model: model.clone(),
            instrument_serial: serial.clone(),
            model_key: HashMap::new(),
            datetime: HashMap::new(),
            gas: HashMap::new(),
            diag: HashMap::new(),
        });

        entry.instrument_model = model.clone();
        entry.instrument_serial = serial.clone();
        entry.model_key.insert(serial.clone(), instrument_type);

        // Dynamically assign each gas value

        entry.datetime.entry(serial.clone()).or_default().push(datetime);
        entry.diag.entry(serial.clone()).or_default().push(diag);
        for gas in &available_gases {
            let idx = gas.as_int();
            let gas_key = GasKey::from((gas, serial.as_str()));

            // Ensure a vector exists for this gas_key
            let gas_vec = entry.gas.entry(gas_key).or_default();

            // Push the value or None
            if let Some(gas_val) = gas_values.get(idx).copied().flatten() {
                gas_vec.push(Some(gas_val));
            } else {
                gas_vec.push(None);
            }
        }
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
    println!("Querying gas data");
    // let mut grouped_data: HashMap<(String, String), GasData> = HashMap::new();
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, co2, ch4, h2o, n2o, diag, instrument_serial, instrument_model
         FROM measurements
         WHERE datetime BETWEEN ?1 AND ?2
         AND project_id = ?3
         ORDER BY instrument_serial, datetime",
    )?;

    let rows = stmt.query_map(params![start.timestamp(), end.timestamp(), project], |row| {
        let datetime_unix: i64 = row.get(0)?;
        let diag: i64 = row.get(5)?;
        let instrument_serial: Option<String> = row.get(6)?;
        let instrument_model: Option<String> = row.get(7)?;

        // Read gas values dynamically in order
        let gas_values = vec![
            row.get::<_, Option<f64>>(1)?, // ch4
            row.get::<_, Option<f64>>(2)?, // co2
            row.get::<_, Option<f64>>(3)?, // h2o
            row.get::<_, Option<f64>>(4)?, // n2o
        ];

        let utc_datetime = chrono::DateTime::from_timestamp(datetime_unix, 0).unwrap();

        Ok((utc_datetime, gas_values, diag, instrument_serial, instrument_model))
    })?;

    for row in rows {
        let (datetime, gas_values, diag, instrument_serial, instrument_model) = row?;
        let date_key = datetime.format("%Y-%m-%d").to_string();
        let serial = instrument_serial.clone().unwrap();
        let model = instrument_model.clone().unwrap();
        let instrument_type = InstrumentType::from_str(&model);
        let available_gases = instrument_type.available_gases();

        let entry = grouped_data.entry(date_key.clone()).or_insert_with(|| GasData {
            header: StringRecord::new(),
            instrument_model: model.clone(), // optional if multiple models
            instrument_serial: serial.clone(), // optional if multiple instruments
            model_key: HashMap::new(),
            datetime: HashMap::new(),
            gas: HashMap::new(),
            diag: HashMap::new(),
        });
        // let entry =
        //     grouped_data.entry((date_key.clone(), serial.clone())).or_insert_with(|| GasData {
        //         header: StringRecord::new(),
        //         instrument_model: model.clone(),
        //         instrument_serial: serial.clone(),
        //         model_key: HashMap::new(),
        //         datetime: HashMap::new(),
        //         gas: HashMap::new(),
        //         diag: HashMap::new(),
        //     });

        entry.instrument_model = model.clone();
        entry.instrument_serial = serial.clone();
        entry.model_key.insert(serial.clone(), instrument_type);

        entry.datetime.entry(serial.clone()).or_default().push(datetime);
        entry.diag.entry(serial.clone()).or_default().push(diag);

        for gas in &available_gases {
            let idx = gas.as_int();
            let gas_key = GasKey::from((gas, serial.as_str()));

            // Ensure a vector exists for this gas_key
            let gas_vec = entry.gas.entry(gas_key).or_default();

            // Push the value or None
            if let Some(gas_val) = gas_values.get(idx).copied().flatten() {
                gas_vec.push(Some(gas_val));
            } else {
                gas_vec.push(None);
            }
        }
    }

    Ok(grouped_data)
}

pub fn query_gas_all(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
) -> Result<GasData> {
    println!("Querying gas data");

    let mut stmt = conn.prepare(
        "SELECT datetime, co2, ch4, h2o, n2o, diag, instrument_serial, instrument_model
         FROM measurements
         WHERE datetime BETWEEN ?1 AND ?2
         AND project_id = ?3
         ORDER BY instrument_serial, datetime",
    )?;

    let rows = stmt.query_map(params![start.timestamp(), end.timestamp(), project], |row| {
        let datetime_unix: i64 = row.get(0)?;
        let diag: i64 = row.get(5)?;
        let instrument_serial: Option<String> = row.get(6)?;
        let instrument_model: Option<String> = row.get(7)?;

        let utc_datetime: DateTime<Utc> =
            chrono::DateTime::from_timestamp(datetime_unix, 0).unwrap().to_utc();
        // Collect gas values into a vector in matching order
        let gas_values = vec![
            row.get::<_, Option<f64>>(1)?, // co2
            row.get::<_, Option<f64>>(2)?, // ch4
            row.get::<_, Option<f64>>(3)?, // h2o
            row.get::<_, Option<f64>>(4)?, // n2o
        ];

        Ok((utc_datetime, gas_values, diag, instrument_serial, instrument_model))
    })?;

    let mut entry = GasData {
        header: StringRecord::new(),
        instrument_model: String::new(),
        instrument_serial: String::new(),
        model_key: HashMap::new(),
        datetime: HashMap::new(),
        gas: HashMap::new(),
        diag: HashMap::new(),
    };

    for row in rows {
        let (datetime, gas_values, diag, instrument_serial, instrument_model) = row?;
        let serial = instrument_serial.clone().unwrap();
        let model = instrument_model.clone().unwrap();
        let instrument_type = InstrumentType::from_str(&model);
        let available_gases = instrument_type.available_gases();

        entry.instrument_model = model.clone();
        entry.instrument_serial = serial.clone();
        entry.model_key.insert(serial.clone(), instrument_type);

        // Append dynamic gas values
        //
        for gas in &available_gases {
            let idx = gas.as_int();
            if let Some(gas_val) = gas_values.get(idx).copied().flatten() {
                entry
                    .gas
                    .entry(GasKey::from((gas, serial.as_str())))
                    .or_default()
                    .push(Some(gas_val));
            }
        }

        // entry.datetime.push(datetime);
        entry.datetime.entry(serial.clone()).or_default().push(datetime);
        entry.diag.entry(serial.clone()).or_default().push(diag);
    }

    Ok(entry)
}

pub fn insert_measurements(
    conn: &mut Connection,
    all_gas: &GasData,
    project: &Project,
) -> Result<(usize, usize)> {
    let diag_vec = &all_gas.diag;
    // let datetime_vec = all_gas.datetime.iter().map(|dt| dt.timestamp()).collect::<Vec<i64>>();

    let datetime_vec: HashMap<String, Vec<i64>> = all_gas
        .datetime
        .iter()
        .map(|(serial, dt_list)| {
            let timestamps = dt_list.iter().map(|dt| dt.timestamp()).collect::<Vec<i64>>();
            (serial.clone(), timestamps)
        })
        .collect();
    let instrument = InstrumentType::from_str(&all_gas.instrument_model);
    // gas_vectors
    let mut gas_map: HashMap<GasType, &Vec<Option<f64>>> = HashMap::new();

    for &gas in instrument.available_gases().iter() {
        if let Some(vec) =
            all_gas.gas.get(&GasKey::from((&gas, all_gas.instrument_serial.as_str())))
        {
            gas_map.insert(gas, vec);
        } else {
            println!("Warning: Missing data for gas {:?}", gas);
        }
    }
    let data_len = datetime_vec.get(&all_gas.instrument_serial.clone()).unwrap().len();

    let tx = conn.transaction()?;
    let mut duplicates = 0;
    let mut inserted = 0;

    // Dynamically build SQL
    let mut columns = vec!["datetime".to_string()];
    let mut placeholders = vec!["?1".to_string()]; // datetime is always first

    let available_gases = instrument.available_gases();
    let mut gas_keys = vec![];
    for (idx, gas) in available_gases.iter().enumerate() {
        columns.push(format!("{}", gas).to_lowercase()); // assumes gas types match DB column names
        placeholders.push(format!("?{}", idx + 2)); // next placeholders
        gas_keys.push(gas); // to get values during loop
    }

    // Add fixed fields
    let base_idx = placeholders.len();
    columns.extend([
        "diag".to_owned(),
        "instrument_serial".to_owned(),
        "instrument_model".to_owned(),
        "project_id".to_owned(),
    ]);
    for i in 0..4 {
        placeholders.push(format!("?{}", base_idx + i + 1));
    }

    let sql = format!(
        "INSERT OR IGNORE INTO measurements ({}) VALUES ({})",
        columns.join(", "),
        placeholders.join(", ")
    );
    let mut stmt = tx.prepare(&sql)?;

    println!("Pushing data!");
    for i in 0..data_len {
        let mut check_stmt = tx
        .prepare("SELECT 1 FROM measurements WHERE datetime = ?1 AND instrument_serial = ?2 AND project_id = ?3")?;
        let mut rows = check_stmt.query(params![
            datetime_vec.get(&all_gas.instrument_serial).unwrap()[i],
            all_gas.instrument_serial,
            project.name
        ])?;

        if rows.next()?.is_some() {
            // If a duplicate exists, log it
            duplicates += 1;
        } else {
            // If no duplicate, insert the new record
            let mut values: Vec<&dyn rusqlite::ToSql> = Vec::new();
            values.push(
                &datetime_vec.get(&all_gas.instrument_serial).unwrap()[i] as &dyn rusqlite::ToSql,
            );
            for gas in &gas_keys {
                values.push(gas_map.get(gas).unwrap()[i].as_ref().unwrap() as &dyn rusqlite::ToSql);
            }
            values.push(
                &diag_vec.get(&all_gas.instrument_serial).unwrap()[i] as &dyn rusqlite::ToSql,
            );

            // Add fixed fields
            values.push(&all_gas.instrument_serial);
            values.push(&all_gas.instrument_model);
            values.push(&project.name);

            stmt.execute(params_from_iter(values))?;
            inserted += 1;
        }
    }

    drop(stmt);
    tx.commit()?;

    // Print how many rows were inserted and how many were duplicates
    println!("Inserted {} rows into measurements, {} duplicates skipped.", inserted, duplicates);

    Ok((inserted, duplicates))
}
