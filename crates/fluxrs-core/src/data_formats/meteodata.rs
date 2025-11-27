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
use std::fmt;
use std::path::Path;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task;

pub const DEFAULT_TEMP: f64 = 10.0;
pub const DEFAULT_PRESSURE: f64 = 980.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeteoSource {
    /// Directly read from CSV or DB
    Raw,
    /// Caller-supplied default value
    Default,
    /// No value is available
    Missing,
}

impl fmt::Display for MeteoSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeteoSource::Raw => write!(f, "Raw"),
            MeteoSource::Default => write!(f, "Default"),
            MeteoSource::Missing => write!(f, "Missing"),
        }
    }
}
impl MeteoSource {
    pub fn as_int(self) -> i32 {
        match self {
            MeteoSource::Raw => 0,
            MeteoSource::Default => 1,
            MeteoSource::Missing => 2,
        }
    }

    pub fn from_int(v: i32) -> Option<MeteoSource> {
        match v {
            0 => Some(MeteoSource::Raw),
            1 => Some(MeteoSource::Default),
            2 => Some(MeteoSource::Missing),
            _ => None, // safe fallback
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeteoPoint {
    pub value: Option<f64>,
    pub source: MeteoSource,
    /// Distance (in seconds) from the target timestamp used in nearest lookup.
    /// - For raw DB/CSV data: None
    /// - For results of get_nearest / get_nearest_meteo_data: Some(abs(dt - target))
    pub distance_from_target: Option<i64>,
}

impl fmt::Display for MeteoPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dist_text = match self.distance_from_target {
            None => String::new(), // don't show distance

            Some(0) => " — exact".to_string(),

            Some(sec_total_signed) => {
                let sign = if sec_total_signed < 0 { -1 } else { 1 };
                let sec_total = sec_total_signed.abs(); // work with positive for components

                let days = sec_total / 86_400;
                let hours = (sec_total % 86_400) / 3600;
                let minutes = (sec_total % 3600) / 60;
                let seconds = sec_total % 60;

                let mut parts = Vec::new();

                if days > 0 {
                    parts.push(format!("{} day{}", days, if days == 1 { "" } else { "s" }));
                }
                if hours > 0 {
                    parts.push(format!("{} h", hours));
                }
                if minutes > 0 {
                    parts.push(format!("{} min", minutes));
                }

                // Always show seconds if total < 1 minute or if it's the only component left
                if seconds > 0 || (days == 0 && hours == 0 && minutes == 0) {
                    parts.push(format!("{} sec", seconds));
                }

                let direction = if sign < 0 { "+" } else { "-" };

                format!(" — {}{}", direction, parts.join(" "))
            },
        };

        match self.value {
            Some(v) => write!(f, "{:.2} ({}){}", v, self.source, dist_text),
            None => write!(f, "None ({}){}", self.source, dist_text),
        }
    }
}

impl MeteoPoint {
    fn new(val: f64) -> Self {
        Self { value: Some(val), source: MeteoSource::Default, distance_from_target: None }
    }
    pub fn or_default(self, default: f64) -> MeteoPoint {
        match self.source {
            MeteoSource::Missing => MeteoPoint {
                value: Some(default),
                source: MeteoSource::Default,
                distance_from_target: None,
            },
            _ => self,
        }
    }
}

impl Default for MeteoPoint {
    fn default() -> Self {
        MeteoPoint { value: None, source: MeteoSource::Default, distance_from_target: None }
    }
}

#[derive(Debug, Default, Clone)]
pub struct MeteoData {
    pub datetime: Vec<i64>,
    pub temperature: Vec<MeteoPoint>,
    pub pressure: Vec<MeteoPoint>,
}

impl MeteoData {
    /// Returns the nearest meteo values within ±30 minutes of `target_timestamp`,
    /// independently for temperature and pressure.
    ///
    /// - If nothing usable is found for both, returns None.
    /// - Otherwise returns Some((temp_point, press_point)), where each point has
    ///   distance_from_target = Some(diff_seconds) if it was found by this lookup.
    pub fn get_nearest(&self, target_timestamp: i64) -> Option<(MeteoPoint, MeteoPoint)> {
        if self.datetime.is_empty() {
            return None;
        }

        // (index, diff)
        let mut best_temp: Option<(usize, i64)> = None;
        let mut best_press: Option<(usize, i64)> = None;

        for (idx, &ts) in self.datetime.iter().enumerate() {
            let signed_diff = ts - target_timestamp; // preserve sign
            let diff = signed_diff.abs(); // for nearest comparison

            // respect ±30 minutes window
            if diff > 1800 {
                continue;
            }

            // temperature
            let t = &self.temperature[idx];
            if t.value.is_some() {
                match best_temp {
                    None => best_temp = Some((idx, signed_diff)),
                    Some((_, best_diff)) if diff < best_diff.abs() => {
                        best_temp = Some((idx, signed_diff))
                    },
                    _ => {},
                }
            }

            // pressure
            let p = &self.pressure[idx];
            if p.value.is_some() {
                match best_press {
                    None => best_press = Some((idx, signed_diff)),
                    Some((_, best_diff)) if diff < best_diff.abs() => {
                        best_press = Some((idx, signed_diff))
                    },
                    _ => {},
                }
            }
        }

        match (best_temp, best_press) {
            (None, None) => None,
            (t_opt, p_opt) => {
                let temp_point = t_opt
                    .map(|(i, diff)| {
                        let base = &self.temperature[i];
                        MeteoPoint {
                            value: base.value,
                            source: base.source,
                            distance_from_target: Some(diff),
                        }
                    })
                    .unwrap_or(MeteoPoint {
                        value: None,
                        source: MeteoSource::Missing,
                        distance_from_target: None,
                    });

                let press_point = p_opt
                    .map(|(i, diff)| {
                        let base = &self.pressure[i];
                        MeteoPoint {
                            value: base.value,
                            source: base.source,
                            distance_from_target: Some(diff),
                        }
                    })
                    .unwrap_or(MeteoPoint {
                        value: None,
                        source: MeteoSource::Missing,
                        distance_from_target: None,
                    });

                Some((temp_point, press_point))
            },
        }
    }
    fn make_point_for_nearest(src: &MeteoPoint) -> MeteoPoint {
        // Always return the source unchanged.
        // All DB data stays Raw or Missing.
        MeteoPoint {
            value: src.value,
            source: src.source,
            distance_from_target: src.distance_from_target,
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
        return Err(rusqlite::Error::InvalidQuery); // length mismatch
    }

    let mut stmt = tx.prepare(
        "INSERT INTO meteo (project_link, datetime, temperature, pressure, file_link)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(datetime, project_link)
         DO UPDATE SET temperature = excluded.temperature,
                       pressure = excluded.pressure",
    )?;

    for i in 0..meteo_data.datetime.len() {
        let datetime = meteo_data.datetime[i];
        let temp_val = meteo_data.temperature[i].value;
        let press_val = meteo_data.pressure[i].value;

        inserted += 1;

        stmt.execute(params![
            project_id, datetime, temp_val, // Option<f64> -> NULL or REAL
            press_val, file_id
        ])?;
    }

    Ok(inserted)
}

pub fn get_nearest_meteo_data(
    conn: &Connection,
    project_id: i64,
    time: i64,
) -> Result<(MeteoPoint, MeteoPoint)> {
    let mut stmt = conn.prepare(
        "SELECT datetime, temperature, pressure
         FROM meteo
         WHERE project_link = ?1
         ORDER BY ABS(datetime - ?2)
         LIMIT 1",
    )?;

    let result = stmt.query_row(params![project_id, time], |row| {
        let dt: i64 = row.get(0)?;
        let t: Option<f64> = row.get(1)?;
        let p: Option<f64> = row.get(2)?;
        Ok((dt, t, p))
    });

    match result {
        Ok((dt, t, p)) => {
            let diff = (dt - time).abs();
            Ok((
                MeteoPoint {
                    value: t,
                    source: match t {
                        Some(_) => MeteoSource::Raw,
                        None => MeteoSource::Missing,
                    },
                    distance_from_target: Some(diff),
                },
                MeteoPoint {
                    value: p,
                    source: match p {
                        Some(_) => MeteoSource::Raw,
                        None => MeteoSource::Missing,
                    },
                    distance_from_target: Some(diff),
                },
            ))
        },
        Err(_) => Ok((
            MeteoPoint { value: None, source: MeteoSource::Missing, distance_from_target: None },
            MeteoPoint { value: None, source: MeteoSource::Missing, distance_from_target: None },
        )),
    }
}

pub fn query_meteo(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project_id: i64,
) -> Result<MeteoData> {
    println!("Querying meteo data");

    let mut stmt = conn.prepare(
        "SELECT datetime, temperature, pressure
         FROM meteo
         WHERE datetime BETWEEN ?1 AND ?2
           AND project_link = ?3
         ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp() - 86400, end.timestamp() + 86400, project_id],
        |row| {
            let datetime_unix: i64 = row.get(0)?;
            let temp: Option<f64> = row.get(1)?;
            let press: Option<f64> = row.get(2)?;
            Ok((datetime_unix, temp, press))
        },
    )?;

    let mut meteos = MeteoData::default();

    for row in rows {
        let (time, temp, press) = row?;
        meteos.datetime.push(time);
        meteos.temperature.push(MeteoPoint {
            value: temp,
            source: match temp {
                Some(_) => MeteoSource::Raw,
                None => MeteoSource::Missing,
            },
            distance_from_target: None,
        });
        meteos.pressure.push(MeteoPoint {
            value: press,
            source: match press {
                Some(_) => MeteoSource::Raw,
                None => MeteoSource::Missing,
            },
            distance_from_target: None,
        });
    }

    Ok(meteos)
}

pub async fn query_meteo_async(
    conn: Arc<Mutex<Connection>>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: Project,
) -> Result<MeteoData> {
    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_meteo(&conn, start, end, project.id.unwrap())
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(rusqlite::Error::ExecuteReturnedResults),
    }
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

        let temp_point = match record.get(1) {
            Some(s) if !s.trim().is_empty() => MeteoPoint {
                value: Some(s.parse()?),
                source: MeteoSource::Raw,
                distance_from_target: None,
            },
            _ => {
                MeteoPoint { value: None, source: MeteoSource::Missing, distance_from_target: None }
            },
        };

        let press_point = match record.get(2) {
            Some(s) if !s.trim().is_empty() => MeteoPoint {
                value: Some(s.parse()?),
                source: MeteoSource::Raw,
                distance_from_target: None,
            },
            _ => {
                MeteoPoint { value: None, source: MeteoSource::Missing, distance_from_target: None }
            },
        };

        datetime.push(timestamp);
        temperature.push(temp_point);
        pressure.push(press_point);
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
    for path in &selected_paths {
        let project_id = project.id.unwrap();

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                return;
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
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue;
            },
        };

        match read_meteo_csv(path, tz) {
            Ok(res) => match insert_meteo_data(&tx, &file_id, &project.id.unwrap(), &res) {
                Ok(row_count) => {
                    let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Ok(
                        " rows of meteo data inserted.".to_owned(),
                        row_count as u64,
                    )));
                    if let Err(e) = tx.commit() {
                        eprintln!("Failed to commit transaction for '{}': {}", file_name, e);
                        let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(
                            format!("Commit failed for file '{}': {}", file_name, e),
                        )));
                        continue;
                    }
                },
                Err(e) => {
                    let msg = format!("Failed to insert meteo data to db. Error {}", e);
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
