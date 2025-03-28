use rusqlite::{params, Connection, Error, Result};
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
) -> Result<()> {
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
        }
    }
    tx.commit()?;
    Ok(())
}
