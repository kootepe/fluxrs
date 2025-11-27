use crate::datatype::DataType;
use crate::processevent::{InsertEvent, ProcessEvent, ReadEvent};
use crate::project::Project;
use crate::utils::get_or_insert_data_file;
use crate::utils::{ensure_utf8, parse_datetime};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rusqlite::{params, Connection, Result};
use std::error::Error;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task;

#[derive(Debug, Default, Clone)]
pub struct HeightData {
    pub datetime: Vec<i64>,
    pub chamber_id: Vec<String>,
    pub height: Vec<f64>,
}

impl HeightData {
    /// the the nearest height to a given timestamp
    pub fn get_nearest_previous_height(
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

        nearest_index.map(|i| self.height[i])
    }
}

pub fn insert_height_data(
    tx: &Connection,
    height_data: &HeightData,
    file_id: &i64,
    project_id: &i64,
) -> Result<u64> {
    let mut inserted: u64 = 0;
    if height_data.datetime.len() != height_data.chamber_id.len()
        || height_data.datetime.len() != height_data.height.len()
    {
        return Err(rusqlite::Error::InvalidQuery); // Ensure all vectors have the same length
    }

    {
        // BUG: BAD SQL
        let mut stmt = tx.prepare(
            "INSERT INTO height (chamber_id, project_link, datetime, height, file_link)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(chamber_id, project_link, datetime)
             DO UPDATE SET height = excluded.height",
        )?;

        for i in 0..height_data.datetime.len() {
            stmt.execute(params![
                &height_data.chamber_id[i],
                project_id,
                height_data.datetime[i],
                height_data.height[i],
                file_id,
            ])?;
            // inserted += 1;
            inserted += tx.changes();
        }
    }
    Ok(inserted)
}

pub fn get_previous_height(
    conn: &Connection,
    project_id: i64,
    chamber_id: String,
    time: i64,
) -> Result<f64> {
    let mut stmt = conn.prepare(
        "SELECT height
             FROM height
             WHERE project_link = ?1
             AND datetime - ?3 < 0
             AND chamber_id = ?2
             ORDER BY datetime - ?3
             LIMIT 1",
    )?;

    let result = stmt.query_row(params![&project_id, &chamber_id, time], |row| (row.get(0)));

    match result {
        Ok(height) => Ok(height),
        Err(_) => Ok(1.0), // Return defaults if no data is found
    }
}

pub fn query_height(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project_id: i64,
) -> Result<HeightData> {
    println!("Querying height data");
    // let mut data = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, height, chamber_id
             FROM height
             WHERE datetime BETWEEN ?1 AND ?2
             and project_link = ?3
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp() - (86400 * 365), end.timestamp() + (86400 * 365), project_id],
        |row| {
            let datetime_unix: i64 = row.get(0)?;
            let height: f64 = row.get(1)?;
            let chamber_id: String = row.get(2)?;

            Ok((datetime_unix, height, chamber_id))
        },
    )?;

    let mut heights = HeightData::default();
    for row in rows {
        let (time, height, chamber_id) = row?;
        heights.datetime.push(time);
        heights.chamber_id.push(chamber_id);
        heights.height.push(height);
    }
    Ok(heights)
}
pub async fn query_height_async(
    conn: Arc<Mutex<Connection>>, // Arc<Mutex> for shared async access
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: Project,
) -> Result<HeightData> {
    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_height(&conn, start, end, project.id.unwrap())
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

pub fn read_height_csv<P: AsRef<Path>>(file_path: P, tz: Tz) -> Result<HeightData, Box<dyn Error>> {
    let content = ensure_utf8(&file_path)?;
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(content.as_bytes());
    let mut datetime = Vec::new();
    let mut chamber_id = Vec::new();
    let mut height = Vec::new();

    for (i, result) in rdr.records().enumerate() {
        let record = result.map_err(|e| format!("CSV read error at row {}: {}", i + 2, e))?;

        let datetime_str =
            record.get(0).ok_or_else(|| format!("Missing datetime at row {}", i + 2))?.trim();
        let ch =
            record.get(1).ok_or_else(|| format!("Missing chamber_id at row {}", i + 2))?.trim();
        let h: f64 = record
            .get(2)
            .ok_or_else(|| format!("Missing height at row {}", i + 2))?
            .trim()
            .parse()
            .map_err(|e| format!("Invalid height at row {}: {}", i + 2, e))?;

        let timestamp = parse_datetime(datetime_str, tz)
            .map_err(|e| format!("Datetime parse error at row {}: {}", i + 2, e))?;

        datetime.push(timestamp);
        chamber_id.push(ch.to_owned());
        height.push(h);
    }

    Ok(HeightData { datetime, chamber_id, height })
}
pub fn upload_height_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    let mut heights = HeightData::default();
    for path in &selected_paths {
        let project_id = project.id.unwrap();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                // Optionally notify UI:
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::height_fail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                continue;
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
        let file_id = match get_or_insert_data_file(&tx, DataType::Height, file_name, project_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                // Optionally notify UI
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue;
            },
        };
        match read_height_csv(path, tz) {
            Ok(res) => match insert_height_data(&tx, &res, &file_id, &project.id.unwrap()) {
                Ok(row_count) => {
                    let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Ok(
                        " of height data inserted.".to_owned(),
                        row_count,
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
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::height_fail(
                    path.to_string_lossy().to_string(),
                    e.to_string(),
                )));
            },
        }
        let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
    }
}
