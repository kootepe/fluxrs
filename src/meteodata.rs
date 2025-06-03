use crate::project_app::Project;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result};
use std::cmp::Ordering;
use std::sync::{Arc, Mutex};
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
    conn: &mut Connection,
    project_id: &str,
    meteo_data: &MeteoData,
) -> Result<usize> {
    let mut inserted = 0;
    if meteo_data.datetime.len() != meteo_data.temperature.len()
        || meteo_data.datetime.len() != meteo_data.pressure.len()
    {
        return Err(rusqlite::Error::InvalidQuery); // Ensure all arrays have the same length
    }

    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO meteo (project_id, datetime, temperature, pressure)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(datetime, project_id)
             DO UPDATE SET temperature = excluded.temperature, pressure = excluded.pressure",
        )?;

        for i in 0..meteo_data.datetime.len() {
            let datetime = meteo_data.datetime[i];
            let temperature = meteo_data.temperature[i];
            let pressure = meteo_data.pressure[i];
            inserted += 1;

            stmt.execute(params![project_id, datetime, temperature, pressure])?;
        }
    }
    tx.commit()?;
    Ok(inserted)
}

pub fn get_nearest_meteo_data(conn: &Connection, project: String, time: i64) -> Result<(f64, f64)> {
    let mut stmt = conn.prepare(
        "SELECT temperature, pressure
             FROM meteo
             WHERE project_id = ?1
             ORDER BY ABS(datetime - ?2)
             LIMIT 1",
    )?;

    let result = stmt.query_row(params![&project, time], |row| Ok((row.get(0)?, row.get(1)?)));

    match result {
        Ok((temperature, pressure)) => Ok((temperature, pressure)),
        Err(_) => Ok((0.0, 0.0)), // Return defaults if no data is found
    }
}
pub fn query_meteo(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
) -> Result<MeteoData> {
    println!("Querying meteo data");
    // let mut data = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, temperature, pressure
             FROM meteo
             WHERE datetime BETWEEN ?1 AND ?2
             and project_id = ?3
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp() - 86400, end.timestamp() + 86400, project],
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
        query_meteo(&conn, start, end, project.name)
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
