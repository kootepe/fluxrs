use rusqlite::{params, Connection, Result};
pub const ERROR_INT: i64 = -9999;
pub const ERROR_FLOAT: f64 = -9999.;

#[derive(Debug, Default)]
pub struct VolumeData {
    pub datetime: Vec<i64>,
    pub chamber_id: Vec<String>,
    pub volume: Vec<f64>,
}

impl VolumeData {
    // pub fn new() -> VolumeData {
    //     VolumeData { datetime: Vec::new(), chamber_id: Vec::new(), volume: Vec::new() }
    // }
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
            "INSERT INTO volume (chamber_id, project_id, datetime, volume)
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
        "SELECT volume
             FROM volume
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
