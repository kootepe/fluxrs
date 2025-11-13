use crate::datatype::DataType;
use chrono::{LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{params, Connection, Result};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::str;

pub fn ensure_utf8<P: AsRef<Path>>(path: P) -> Result<String, Box<dyn Error>> {
    let bytes = fs::read(&path)?;
    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => {
            Err(format!("Input file '{}' is not valid UTF-8: {}", path.as_ref().display(), e)
                .into())
        },
    }
}

pub fn parse_datetime(s: &str, tz: Tz) -> Result<i64, Box<dyn Error>> {
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
        "%d-%m-%Y %H:%M:%S",
        "%d/%m/%Y %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.fZ",
    ];

    for fmt in &formats {
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(s, fmt) {
            let dt_utc = match tz.from_local_datetime(&naive_dt) {
                LocalResult::Single(dt) => dt.with_timezone(&Utc),
                LocalResult::Ambiguous(dt1, _) => dt1.with_timezone(&Utc),
                LocalResult::None => {
                    eprintln!("Impossible local time {}. Fix or remove.", naive_dt);
                    continue;
                },
            };
            return Ok(dt_utc.timestamp());
        }
    }
    Err(format!("Unrecognized datetime format: {}", s).into())
}
pub fn get_or_insert_data_file(
    conn: &Connection,
    datatype: DataType,
    file_name: &str,
    project_id: i64,
) -> Result<i64> {
    // First, check if the file already exists for this project
    if let Ok(existing_id) = conn.query_row(
        "SELECT id FROM data_files WHERE file_name = ?1 AND project_link = ?2",
        params![file_name, project_id],
        |row| row.get::<_, i64>(0),
    ) {
        // Found existing entry
        return Ok(existing_id);
    }

    // If not found, insert it
    conn.execute(
        "INSERT INTO data_files (file_name, data_type, project_link) VALUES (?1, ?2, ?3)",
        params![file_name, datatype.type_str(), project_id],
    )?;

    // Return the new ID
    Ok(conn.last_insert_rowid())
}
