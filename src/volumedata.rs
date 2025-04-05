use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result};
pub const ERROR_INT: i64 = -9999;
pub const ERROR_FLOAT: f64 = -9999.;

#[derive(Debug, Default, Clone)]
pub struct VolumeData {
    pub datetime: Vec<i64>,
    pub chamber_id: Vec<String>,
    pub volume: Vec<f64>,
}

impl VolumeData {
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

pub fn query_volume(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
) -> Result<VolumeData> {
    // let mut data = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, volume, chamber_id
             FROM volume
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
