use crate::datatype::DataType;
use crate::processevent::{InsertEvent, ProcessEvent, ReadEvent};
use crate::project::Project;
use crate::utils::get_or_insert_data_file;
use crate::utils::{ensure_utf8, parse_datetime};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rusqlite::{params, Connection, Result};
use std::cmp::Ordering;
use std::error::Error;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task;

#[derive(Debug, Default, Clone)]
pub struct MeteoData {
    pub datetime: Vec<i64>,
    pub temperature: Vec<f64>,
    pub pressure: Vec<f64>,
}
impl MeteoData {
    pub fn get_nearest(&self, target_timestamp: i64) -> Option<(f64, f64)> {
        if self.datetime.is_empty() {
            return None; // No data available
        }

        let mut left = 0;
        let mut right = self.datetime.len() - 1;

        while left < right {
            let mid = (left + right) / 2;
            match self.datetime[mid].cmp(&target_timestamp) {
                Ordering::Less => left = mid + 1,
                Ordering::Greater => {
                    if mid > 0 {
                        right = mid - 1;
                    } else {
                        break;
                    }
                },
                Ordering::Equal => return Some((self.temperature[mid], self.pressure[mid])),
            }
        }

        match left {
            0 => {
                let diff = (self.datetime[0] - target_timestamp).abs();
                if diff <= 1800 {
                    Some((self.temperature[0], self.pressure[0]))
                } else {
                    None
                }
            },
            _ if left >= self.datetime.len() => {
                let diff = (self.datetime[right] - target_timestamp).abs();
                if diff <= 1800 {
                    Some((self.temperature[right], self.pressure[right]))
                } else {
                    None
                }
            },
            _ => {
                let prev_idx = left - 1;
                let next_idx = left;

                let prev_diff = (self.datetime[prev_idx] - target_timestamp).abs();
                let next_diff = (self.datetime[next_idx] - target_timestamp).abs();

                let (nearest_idx, nearest_diff) = if prev_diff <= next_diff {
                    (prev_idx, prev_diff)
                } else {
                    (next_idx, next_diff)
                };

                if nearest_diff <= 1800 {
                    Some((self.temperature[nearest_idx], self.pressure[nearest_idx]))
                } else {
                    None // No valid data within 30 min
                }
            },
        }
    }
}
pub fn insert_meteo_data(
    tx: &Connection,
    file_id: &i64,
    project_id: &i64,
    meteo_data: &MeteoData,
) -> Result<usize> {
    let mut inserted = 0;
    if meteo_data.datetime.len() != meteo_data.temperature.len()
        || meteo_data.datetime.len() != meteo_data.pressure.len()
    {
        return Err(rusqlite::Error::InvalidQuery); // Ensure all arrays have the same length
    }

    {
        // BUG: BAD SQL
        let mut stmt = tx.prepare(
            "INSERT INTO meteo (project_link, datetime, temperature, pressure, file_link)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(datetime, project_link)
             DO UPDATE SET temperature = excluded.temperature, pressure = excluded.pressure",
        )?;

        for i in 0..meteo_data.datetime.len() {
            let datetime = meteo_data.datetime[i];
            let temperature = meteo_data.temperature[i];
            let pressure = meteo_data.pressure[i];
            inserted += 1;

            stmt.execute(params![project_id, datetime, temperature, pressure, file_id])?;
        }
    }
    Ok(inserted)
}

pub fn get_nearest_meteo_data(conn: &Connection, project_id: i64, time: i64) -> Result<(f64, f64)> {
    let mut stmt = conn.prepare(
        "SELECT temperature, pressure
             FROM meteo
             WHERE project_link = ?1
             ORDER BY ABS(datetime - ?2)
             LIMIT 1",
    )?;

    let result = stmt.query_row(params![&project_id, time], |row| Ok((row.get(0)?, row.get(1)?)));

    match result {
        Ok((temperature, pressure)) => Ok((temperature, pressure)),
        Err(_) => Ok((0.0, 0.0)), // Return defaults if no data is found
    }
}
pub fn query_meteo(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project_id: i64,
) -> Result<MeteoData> {
    println!("Querying meteo data");
    // let mut data = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, temperature, pressure
             FROM meteo m
             WHERE datetime BETWEEN ?1 AND ?2
             and project_link = ?3
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp() - 86400, end.timestamp() + 86400, project_id],
        |row| {
            let datetime_unix: i64 = row.get(0)?;
            let temp: f64 = row.get(1)?;
            let press: f64 = row.get(2)?;

            Ok((datetime_unix, temp, press))
        },
    )?;

    let mut meteos = MeteoData::default();
    for row in rows {
        let (time, temp, press) = row?;
        meteos.datetime.push(time);
        meteos.temperature.push(temp);
        meteos.pressure.push(press);
    }
    Ok(meteos)
}
pub async fn query_meteo_async(
    conn: Arc<Mutex<Connection>>, // Arc<Mutex> for shared async access
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: Project,
) -> Result<MeteoData> {
    // let start_ts = start.timestamp();
    // let end_ts = end.timestamp();

    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_meteo(&conn, start, end, project.id.unwrap())
    })
    .await;
    match result {
        Ok(inner) => inner,
        Err(_) => {
            // Convert JoinError into rusqlite::Error::ExecuteReturnedResults or custom error
            Err(rusqlite::Error::ExecuteReturnedResults) // or log `e` if needed
        },
    }
    // match result {
    //     Ok(inst) => inst,
    //     Err(_) => Ok(MeteoData::default()),
    // }
}
pub fn read_meteo_csv<P: AsRef<Path>>(file_path: P, tz: Tz) -> Result<MeteoData, Box<dyn Error>> {
    let content = ensure_utf8(&file_path)?;
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(content.as_bytes());

    let mut datetime = Vec::new();
    let mut temperature = Vec::new();
    let mut pressure = Vec::new();

    for result in rdr.records() {
        let record = result?;
        let datetime_str = record.get(0).ok_or("Missing datetime field")?;
        let timestamp = parse_datetime(datetime_str, tz)?;
        let temp: f64 = record.get(1).ok_or("Missing temperature field")?.parse()?;
        let press: f64 = record.get(2).ok_or("Missing pressure field")?.parse()?;

        datetime.push(timestamp);
        temperature.push(temp);
        pressure.push(press);
    }

    Ok(MeteoData { datetime, temperature, pressure })
}
pub fn upload_meteo_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    let mut meteos = MeteoData::default();
    for path in &selected_paths {
        let project_id = project.id.unwrap();

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                // Optionally notify UI:
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                return (); // or `continue` if this is in a loop
            },
        };
        let tx = match conn.transaction() {
            Ok(tx) => tx,
            Err(e) => {
                eprintln!("Failed to start transaction: {}", e);
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "Could not start transaction for '{}': {}",
                    file_name, e
                ))));
                continue;
            },
        };
        let file_id = match get_or_insert_data_file(&tx, DataType::Meteo, file_name, project_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                // Optionally notify UI
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue; // or return if not inside a loop
            },
        };
        match read_meteo_csv(path, tz) {
            //   Pass `path` directly
            Ok(res) => match insert_meteo_data(&tx, &file_id, &project.id.unwrap(), &res) {
                Ok(row_count) => {
                    let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Ok(
                        " of meteo data inserted.".to_owned(),
                        row_count as u64,
                    )));
                    if let Err(e) = tx.commit() {
                        eprintln!("Failed to commit transaction for '{}': {}", file_name, e);
                        let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(
                            format!("Commit failed for file '{}': {}", file_name, e),
                        )));
                        // no commit = rollback
                        continue;
                    }
                },
                Err(e) => {
                    let msg = format!("Failed to insert cycle data to db.Error {}", e);
                    let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(msg)));
                },
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::MeteoFail(
                    path.to_string_lossy().to_string(),
                    e.to_string(),
                )));
            },
        }
        let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
    }
}
