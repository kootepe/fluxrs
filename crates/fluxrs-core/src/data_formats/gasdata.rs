use crate::constants::ERROR_INT;
use crate::cycle::gaskey::GasKey;
use crate::instruments::instruments::get_or_insert_instrument;
use crate::instruments::instruments::{Instrument, InstrumentMeasurement, InstrumentType};
use crate::project::Project;
use crate::traits::EqualLen;
use crate::types::FastMap;

use chrono::prelude::DateTime;
use chrono::Utc;
use csv::StringRecord;
use rusqlite::{params, params_from_iter, Connection};
use tokio::task;

use std::collections::HashMap;
use std::collections::HashSet;
use std::process;
use std::sync::{Arc, Mutex};

use rusqlite;
use std::fmt;

#[derive(Debug)]
pub enum QueryError {
    Db(rusqlite::Error),
    MissingInstrumentId,
    SelectedInstrumentNotFound { instrument: Instrument },
    JoinError(String),
    // add other variants as needed
}

impl From<rusqlite::Error> for QueryError {
    fn from(err: rusqlite::Error) -> Self {
        QueryError::Db(err)
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::Db(e) => write!(f, "Database error: {e}"),
            QueryError::MissingInstrumentId => write!(f, "Instrument ID was NULL in database row"),
            QueryError::SelectedInstrumentNotFound { instrument } => {
                write!(f, "No data found for selected instrument {instrument}")
            },
            QueryError::JoinError(s) => write!(f, "Join error: {}", s),
        }
    }
}

impl std::error::Error for QueryError {}

pub type Result<T> = std::result::Result<T, QueryError>;
#[derive(Clone, Debug)]
pub struct GasData {
    pub header: StringRecord,
    // NOTE: Change to InstrumentType
    pub instruments: HashSet<Instrument>,
    pub model_key: FastMap<i64, InstrumentType>,
    pub datetime: FastMap<i64, Vec<i64>>,
    pub gas: FastMap<GasKey, Vec<Option<f64>>>,
    pub diag: FastMap<i64, Vec<i64>>,
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
            instruments: HashSet::new(),
            model_key: FastMap::default(),
            datetime: FastMap::default(),
            gas: FastMap::default(),
            diag: FastMap::default(),
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
) -> Result<HashMap<String, Arc<GasData>>> {
    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_gas2(&conn, start.timestamp(), end.timestamp(), project)
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
    project: Project,
) -> Result<HashMap<String, Arc<GasData>>> {
    println!("Querying gas data");
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    // Static columns must match dynamic index logic below
    let mut stmt = conn.prepare(
        "SELECT m.datetime,
                    m.co2, m.ch4,
                    m.h2o, m.n2o,
                    m.diag,
                    i.instrument_serial,
                    i.instrument_model,
                    i.id AS instrument_id
         FROM measurements m
         LEFT JOIN instruments i ON m.instrument_link = i.id
         WHERE m.datetime BETWEEN ?1 AND ?2
           AND m.project_link = ?3
         ORDER BY i.instrument_serial, m.datetime",
    )?;

    let rows = stmt.query_map(params![start, end, project.id.unwrap()], |row| {
        let datetime_unix: i64 = row.get(0)?;
        let diag: i64 = row.get(5)?;
        let instrument_serial: Option<String> = row.get(6)?;
        let instrument_model: Option<String> = row.get(7)?;
        let instrument_id: Option<i64> = row.get(8)?;

        // Collect gas values into a Vec<Option<f64>>
        let gases = vec![
            row.get::<_, Option<f64>>(1)?,
            row.get::<_, Option<f64>>(2)?,
            row.get::<_, Option<f64>>(3)?,
            row.get::<_, Option<f64>>(4)?,
        ];

        Ok((datetime_unix, gases, diag, instrument_serial, instrument_model, instrument_id))
    })?;

    for row in rows {
        let (dt_unix, gas_values, diag, instrument_serial, instrument_model, instrument_id) = row?;

        // Skip rows lacking serial/model (you can choose to error instead)
        let (serial, model) = match (instrument_serial, instrument_model) {
            (Some(s), Some(m)) => (s, m),
            _ => continue,
        };

        let dt_utc: DateTime<Utc> = chrono::DateTime::from_timestamp(dt_unix, 0).unwrap();
        let date_key = dt_utc.format("%Y-%m-%d").to_string();

        let instrument_type = match model.parse::<InstrumentType>() {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Unexpected invalid instrument type from DB: '{}'", model);
                process::exit(1);
            },
        };
        let available_gases = instrument_type.available_gases();

        let entry = grouped_data.entry(date_key).or_insert_with(|| GasData {
            header: StringRecord::new(),
            instruments: HashSet::new(),
            model_key: FastMap::default(),
            datetime: FastMap::default(),
            gas: FastMap::default(),
            diag: FastMap::default(),
        });

        // Keep these up-to-date if multiple instruments share a day
        entry.instruments.insert(Instrument {
            model: instrument_type,
            serial: serial.clone(),
            id: instrument_id,
        });
        entry.model_key.insert(instrument_id.unwrap(), instrument_type);

        // Time + diag
        entry.datetime.entry(instrument_id.unwrap()).or_default().push(dt_unix);
        entry.diag.entry(instrument_id.unwrap()).or_default().push(diag);

        // Gas vectors
        for gas in &available_gases {
            let idx = gas.as_int();
            let gas_key = GasKey::from((gas, &instrument_id.unwrap()));
            let gas_vec = entry.gas.entry(gas_key).or_default();

            if let Some(gas_val) = gas_values.get(idx).copied().flatten() {
                gas_vec.push(Some(gas_val));
            } else {
                gas_vec.push(None);
            }
        }
    }

    // Convert to Arc-wrapped values without cloning GasData
    let grouped_arc: HashMap<String, Arc<GasData>> =
        grouped_data.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();

    Ok(grouped_arc)
}

pub fn query_gas_all(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project_id: i64,
) -> Result<GasData> {
    println!("Querying gas data");

    let mut stmt = conn.prepare(
        "SELECT m.datetime, m.co2, m.ch4, m.h2o, m.n2o, m.diag, i.instrument_serial, i.instrument_model, i.id
         FROM measurements m
         LEFT JOIN instruments i ON m.instrument_link = i.id
         WHERE m.datetime BETWEEN ?1 AND ?2
         AND m.project_link = ?3
         ORDER BY i.instrument_serial, m.datetime",
    )?;

    let rows = stmt.query_map(params![start.timestamp(), end.timestamp(), project_id], |row| {
        let datetime_unix: i64 = row.get(0)?;
        let diag: i64 = row.get(5)?;
        let instrument_serial: String = row.get(6)?;
        let instrument_model: String = row.get(7)?;
        let instrument_id: i64 = row.get(8)?;

        // Collect gas values into a vector in matching order
        let gas_values = vec![
            row.get::<_, Option<f64>>(1)?, // co2
            row.get::<_, Option<f64>>(2)?, // ch4
            row.get::<_, Option<f64>>(3)?, // h2o
            row.get::<_, Option<f64>>(4)?, // n2o
        ];
        let instrument_type =
            instrument_model.parse::<InstrumentType>().expect("Invalid instrument type in DB");

        let instrument = Instrument {
            model: instrument_type,
            serial: instrument_serial.clone(),
            id: Some(instrument_id),
        };

        Ok((datetime_unix, gas_values, diag, instrument))
    })?;

    let mut entry = GasData {
        header: StringRecord::new(),
        instruments: HashSet::new(),
        model_key: FastMap::default(),
        datetime: FastMap::default(),
        gas: FastMap::default(),
        diag: FastMap::default(),
    };

    for row in rows {
        let (dt_unix, gas_values, diag, instrument) = row?;
        let serial = instrument.serial.clone();
        let model = instrument.model.clone();

        let available_gases = instrument.model.available_gases();

        entry.model_key.insert(instrument.id.unwrap(), instrument.model);

        // Append dynamic gas values
        //
        for gas in &available_gases {
            let idx = gas.as_int();
            if let Some(gas_val) = gas_values.get(idx).copied().flatten() {
                entry
                    .gas
                    .entry(GasKey::from((gas, &instrument.id.unwrap())))
                    .or_default()
                    .push(Some(gas_val));
            }
        }

        // entry.datetime.push(datetime);
        entry.datetime.entry(instrument.id.unwrap()).or_default().push(dt_unix);
        entry.diag.entry(instrument.id.unwrap()).or_default().push(diag);
    }

    Ok(entry)
}

pub fn insert_measurements(
    tx: &Connection,
    all_gas: &InstrumentMeasurement,
    project: &Project,
    file_id: &i64,
) -> Result<(usize, usize)> {
    let diag_vec = &all_gas.diag;

    let data_len = all_gas.datetime.len();

    let mut duplicates = 0;
    let mut inserted = 0;

    let instrument_id = get_or_insert_instrument(&tx, &all_gas.instrument, project.id.unwrap())?;

    // Dynamically build SQL
    let mut columns = vec!["datetime".to_string()];
    let mut placeholders = vec!["?1".to_string()]; // datetime is always first

    let available_gases = &all_gas.instrument.model.available_gases();
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
        "instrument_link".to_owned(),
        "project_link".to_owned(),
        "file_link".to_owned(),
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
        // If no duplicate, insert the new record
        let mut values: Vec<&dyn rusqlite::ToSql> = Vec::new();
        let id = project.id.unwrap();
        let ins_id = &instrument_id;
        let dt = &all_gas.datetime[i] as &dyn rusqlite::ToSql;
        let diag = &all_gas.diag[i] as &dyn rusqlite::ToSql;

        values.push(dt);

        for gas in &gas_keys {
            let g = all_gas.gas.get(gas).unwrap()[i].as_ref().unwrap() as &dyn rusqlite::ToSql;
            values.push(g);
        }

        values.push(diag);
        values.push(&ins_id);
        values.push(&id);
        values.push(&file_id);

        let affected = stmt.execute(params_from_iter(values))?;
        if affected > 0 {
            inserted += 1;
        } else {
            duplicates += 1; // affected == 0 → ignored
        }
    }

    drop(stmt);

    // Print how many rows were inserted and how many were duplicates
    println!("Inserted {} rows into measurements, {} duplicates skipped.", inserted, duplicates);

    Ok((inserted, duplicates))
}
