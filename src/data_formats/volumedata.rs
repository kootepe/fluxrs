use crate::ui::project_ui::Project;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use rusqlite::{params, Connection, Result};
use std::error::Error;
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::task;

#[derive(Debug, Default, Clone)]
pub struct VolumeData {
    pub datetime: Vec<i64>,
    pub chamber_id: Vec<String>,
    pub volume: Vec<f64>,
}

impl VolumeData {
    /// the the nearest volume to a given timestamp
    pub fn get_nearest_previous_volume(
        &self,
        target_datetime: i64,
        target_chamber_id: &str,
    ) -> Option<f64> {
        let mut nearest_index: Option<usize> = None;
        let mut smallest_time_diff = i64::MAX;

        for (i, (dt, chamber)) in self.datetime.iter().zip(&self.chamber_id).enumerate() {
            if chamber == target_chamber_id && *dt <= target_datetime {
                let time_diff = target_datetime - dt; // guaranteed to be >= 0
                if time_diff < smallest_time_diff {
                    smallest_time_diff = time_diff;
                    nearest_index = Some(i);
                }
            }
        }

        nearest_index.map(|i| self.volume[i])
    }
}

pub fn insert_volume_data(
    conn: &mut Connection,
    project_id: &str,
    volume_data: &VolumeData,
) -> Result<usize> {
    let mut inserted: usize = 0;
    if volume_data.datetime.len() != volume_data.chamber_id.len()
        || volume_data.datetime.len() != volume_data.volume.len()
    {
        return Err(rusqlite::Error::InvalidQuery); // Ensure all vectors have the same length
    }

    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO height (chamber_id, project_id, datetime, volume)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(chamber_id, project_id, datetime)
             DO UPDATE SET volume = excluded.volume",
        )?;

        for i in 0..volume_data.datetime.len() {
            stmt.execute(params![
                &volume_data.chamber_id[i],
                project_id,
                volume_data.datetime[i],
                volume_data.volume[i]
            ])?;
            inserted += 1;
        }
    }
    tx.commit()?;
    Ok(inserted)
}

pub fn get_previous_volume(
    conn: &Connection,
    project: String,
    chamber_id: String,
    time: i64,
) -> Result<f64> {
    let mut stmt = conn.prepare(
        "SELECT height
             FROM height
             WHERE project_id = ?1
             AND datetime - ?3 < 0
             AND chamber_id = ?2
             ORDER BY datetime - ?3
             LIMIT 1",
    )?;

    let result = stmt.query_row(params![&project, &chamber_id, time], |row| (row.get(0)));

    match result {
        Ok(volume) => Ok(volume),
        Err(_) => Ok(1.0), // Return defaults if no data is found
    }
}

pub fn query_volume(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
) -> Result<VolumeData> {
    println!("Querying volume data");
    // let mut data = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, height, chamber_id
             FROM height
             WHERE datetime BETWEEN ?1 AND ?2
             and project_id = ?3
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp() - (86400 * 365), end.timestamp() + (86400 * 365), project],
        |row| {
            let datetime_unix: i64 = row.get(0)?;
            let volume: f64 = row.get(1)?;
            let chamber_id: String = row.get(2)?;

            Ok((datetime_unix, volume, chamber_id))
        },
    )?;

    let mut volumes = VolumeData::default();
    for row in rows {
        let (time, volume, chamber_id) = row?;
        volumes.datetime.push(time);
        volumes.chamber_id.push(chamber_id);
        volumes.volume.push(volume);
    }
    Ok(volumes)
}
pub async fn query_volume_async(
    conn: Arc<Mutex<Connection>>, // Arc<Mutex> for shared async access
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: Project,
) -> Result<VolumeData> {
    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_volume(&conn, start, end, project.name)
    })
    .await;
    match result {
        Ok(inner) => inner,
        Err(_) => {
            // Convert JoinError into rusqlite::Error::ExecuteReturnedResults or custom error
            Err(rusqlite::Error::ExecuteReturnedResults) // or log `e` if needed
        },
    }
}
pub fn read_volume_csv<P: AsRef<Path>>(file_path: P) -> Result<VolumeData, Box<dyn Error>> {
    let file = File::open(file_path)?;

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true) //   Ensure headers are read
        .from_reader(file);

    let mut datetime = Vec::new();
    let mut chamber_id = Vec::new();
    let mut volume = Vec::new();

    for result in rdr.records() {
        let record = result?;

        let datetime_str = &record[0]; // Read datetime column
        let ch = &record[1]; // Read datetime column
        let vol: f64 = record[2].parse()?; // Read air_pressure column

        // Convert datetime string to Unix timestamp
        let dt = NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S")?;
        let timestamp = Utc.from_utc_datetime(&dt).timestamp();

        // Store values
        datetime.push(timestamp);
        chamber_id.push(ch.to_owned());
        volume.push(vol);
    }

    Ok(VolumeData { datetime, chamber_id, volume })
}
