use crate::EqualLen;
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{params, Connection, Result};

#[derive(Debug)]
pub struct TimeData {
    pub chamber_id: Vec<String>,
    pub start_time: Vec<DateTime<Utc>>,
    pub close_offset: Vec<i64>,
    pub open_offset: Vec<i64>,
    pub end_offset: Vec<i64>,
    pub project: Vec<String>,
}

impl EqualLen for TimeData {
    fn validate_lengths(&self) -> bool {
        let lengths = [
            &self.chamber_id.len(),
            &self.start_time.len(),
            &self.close_offset.len(),
            &self.open_offset.len(),
            &self.end_offset.len(),
        ];
        let mut check: bool = true;

        for vec_len in lengths.iter() {
            let len = vec_len;
            if vec_len != len {
                check = false;
                break;
            } else {
                continue;
            };
        }
        check
    }
}
impl Default for TimeData {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeData {
    pub fn new() -> TimeData {
        TimeData {
            chamber_id: Vec::new(),
            start_time: Vec::new(),
            close_offset: Vec::new(),
            open_offset: Vec::new(),
            end_offset: Vec::new(),
            project: Vec::new(),
        }
    }
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&String, &DateTime<Utc>, &i64, &i64, &i64, &String)> {
        self.chamber_id
            .iter()
            .zip(&self.start_time)
            .zip(&self.close_offset)
            .zip(&self.open_offset)
            .zip(&self.end_offset)
            .zip(&self.project)
            .map(|(((((chamber, start), close), open), end), project)| {
                (chamber, start, close, open, end, project)
            })
    }
}
pub fn query_cycles(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
) -> Result<TimeData> {
    println!("Querying cycles");
    let mut stmt = conn.prepare(
        "SELECT chamber_id, start_time, close_offset, open_offset, end_offset, project_id
         FROM cycles
         WHERE start_time BETWEEN ?1 AND ?2
         AND project_id = ?3
         ORDER BY start_time",
    )?;

    let mut times = TimeData::new();
    let cycle_iter =
        stmt.query_map(params![start.timestamp(), end.timestamp(), project], |row| {
            let chamber_id: String = row.get(0)?;
            let start_timestamp: i64 = row.get(1)?; // Start time as UNIX timestamp
            let close_offset: i64 = row.get(2)?;
            let open_offset: i64 = row.get(3)?;
            let end_offset: i64 = row.get(4)?;
            let project_id: String = row.get(5)?;

            let start_time = DateTime::<Utc>::from_utc(
                NaiveDateTime::from_timestamp_opt(start_timestamp, 0).expect("Invalid timestamp"),
                Utc,
            );

            times.chamber_id.push(chamber_id);
            times.start_time.push(start_time);
            times.close_offset.push(close_offset);
            times.open_offset.push(open_offset);
            times.end_offset.push(end_offset);
            times.project.push(project_id);

            Ok(())
        })?;

    for row in cycle_iter {
        row?; // Ensure errors are propagated
    }
    Ok(times)
}
