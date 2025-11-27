use crate::datatype::DataType;
use chrono::{LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{params, Connection, Result};
use std::error::Error;
use std::fmt;
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
                    return Err(format!(
                        "Impossible local time {}. Selected timezone ({}) is likely incorrect.",
                        naive_dt, tz
                    )
                    .into());
                },
            };
            return Ok(dt_utc.timestamp());
        }
    }
    Err(format!("Unrecognized datetime format: {}", s).into())
}

pub fn get_file_id(
    conn: &Connection,
    datatype: DataType,
    file_name: &str,
    project_id: i64,
) -> Option<i64> {
    conn.query_row(
        "SELECT id FROM data_files
         WHERE file_name = ?1 AND data_type = ?2 AND project_link = ?3",
        params![file_name, datatype.type_str(), project_id],
        |row| row.get::<_, i64>(0),
    )
    .ok()
}
pub fn insert_data_file(
    conn: &Connection,
    datatype: DataType,
    file_name: &str,
    project_id: i64,
) -> Result<i64, DataFileError> {
    conn.execute(
        "INSERT INTO data_files (file_name, data_type, project_link)
         VALUES (?1, ?2, ?3)",
        params![file_name, datatype.type_str(), project_id],
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn get_or_insert_data_file(
    conn: &Connection,
    datatype: DataType,
    file_name: &str,
    project_id: i64,
) -> Result<i64, DataFileError> {
    if let Some(id) = get_file_id(conn, datatype, file_name, project_id) {
        return Err(DataFileError::FileAlreadyExists(id));
    }

    insert_data_file(conn, datatype, file_name, project_id)
}
#[derive(Debug)]
pub enum DataFileError {
    FileAlreadyExists(i64),
    Sql(rusqlite::Error),
}

impl From<rusqlite::Error> for DataFileError {
    fn from(err: rusqlite::Error) -> Self {
        DataFileError::Sql(err)
    }
}

impl fmt::Display for DataFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataFileError::FileAlreadyExists(i) => write!(f, "File already exists, id: {}", i),
            DataFileError::Sql(err) => write!(f, "Database error: {}", err),
        }
    }
}
