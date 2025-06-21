use crate::get_paths;
use crate::project_app::Project;
use crate::EqualLen;
use chrono::{
    naive, DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use chrono_tz::Europe::Helsinki;
use rusqlite::{params, Connection, Result};
use std::error::Error;
use std::fs::File;
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};
use tokio::task;

pub enum TimeFormat {
    Default,
    Oulanka,
}

pub trait TimeFormatParser {
    fn parse(&self, path: &Path, project_id: &Project) -> Result<TimeData, Box<dyn Error>>;
    fn name(&self) -> &'static str;
}

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
struct OulankaManualFormat;

impl TimeFormatParser for OulankaManualFormat {
    fn name(&self) -> &'static str {
        "Oulanka Manual Format"
    }

    fn parse(&self, path: &Path, project_id: &Project) -> Result<TimeData, Box<dyn Error>> {
        let file = File::open(path)?;
        let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(file);

        let mut date = NaiveDate::default();
        let mut instrument_serial = String::new();
        let mut instrument_model = String::new();
        let mut measurement_time: i64 = 0;

        let mut chamber_id: Vec<String> = Vec::new();
        let mut start_time: Vec<DateTime<Utc>> = Vec::new();
        let mut close_offset: Vec<i64> = Vec::new();
        let mut open_offset: Vec<i64> = Vec::new();
        let mut end_offset: Vec<i64> = Vec::new();
        let mut project: Vec<String> = Vec::new();
        let mut snow_in_chamber: Vec<i64> = Vec::new();

        let mut records = rdr.records();

        if let Some(result) = records.next() {
            let date_str = result?.get(1).unwrap_or("").to_string();
            match NaiveDate::parse_from_str(&date_str, "%y%m%d") {
                Ok(ndate) => date = ndate,
                Err(_) => {
                    let msg = format!("Failed to parse {} as YYMMDD", date_str);
                    println!("{}", msg);
                    return Err(msg.into());
                },
            }
        }

        if let Some(result) = records.next() {
            let record = result?; // <- This keeps the CSV record alive
            let val_str = record.get(1).unwrap_or("0");
            measurement_time = val_str.parse::<i64>().unwrap_or(0);
        }
        if let Some(result) = records.next() {
            instrument_serial = result?.get(1).unwrap_or("").to_string();
        }
        if let Some(result) = records.next() {
            instrument_model = result?.get(1).unwrap_or("").to_string();
        }

        // Skip header row before data
        records.next();

        for r in records {
            let record = r?;
            chamber_id.push(record[0].to_owned());

            match NaiveTime::parse_from_str(&record[1], "%H%M") {
                Ok(naive_time) => {
                    let naive_dt = date.and_time(naive_time);
                    let dt_utc = match Helsinki.from_local_datetime(&naive_dt) {
                        LocalResult::Single(dt) => dt.with_timezone(&Utc),
                        LocalResult::Ambiguous(dt1, _) => dt1.with_timezone(&Utc),
                        LocalResult::None => {
                            eprintln!("Impossible local time {}\nFix or remove.", naive_dt);
                            continue;
                        },
                    };
                    start_time.push(dt_utc - Duration::seconds(60));
                },
                Err(e) => {
                    println!("Failed to parse timestamp: {}", e);
                    continue;
                },
            }

            close_offset.push(record.get(2).unwrap_or("0").parse().unwrap_or(0));
            end_offset.push(record.get(4).unwrap_or("0").parse().unwrap_or(0));
            open_offset.push(measurement_time);
            project.push(project_id.name.to_owned());
        }

        Ok(TimeData {
            chamber_id,
            start_time,
            close_offset,
            open_offset,
            end_offset,
            project,
            // snow_in_chamber,
        })
    }
}

struct DefaultFormat;

impl TimeFormatParser for DefaultFormat {
    fn name(&self) -> &'static str {
        "Default time format"
    }

    fn parse(&self, path: &Path, project_id: &Project) -> Result<TimeData, Box<dyn Error>> {
        let file = File::open(path)?;
        let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(file);

        // chamber_id,start_time,close_offset,open_offset,end_offset
        let mut chamber_id: Vec<String> = Vec::new();
        let mut start_time: Vec<DateTime<Utc>> = Vec::new();
        let mut close_offset: Vec<i64> = Vec::new();
        let mut open_offset: Vec<i64> = Vec::new();
        let mut end_offset: Vec<i64> = Vec::new();
        let mut project: Vec<String> = Vec::new();

        for r in rdr.records() {
            let record: &csv::StringRecord = &r?;
            chamber_id.push(record[0].to_owned());

            match NaiveDateTime::parse_from_str(&record[1], "%Y-%m-%d %H:%M:%S") {
                Ok(naive_dt) => {
                    let dt_utc = match Helsinki.from_local_datetime(&naive_dt) {
                        LocalResult::Single(dt) => dt.with_timezone(&Utc),
                        LocalResult::Ambiguous(dt1, _) => dt1.with_timezone(&Utc),
                        LocalResult::None => {
                            eprintln!("Impossible local time {}\nFix or remove.", naive_dt);
                            process::exit(1)
                        },
                    };
                    start_time.push(dt_utc)
                },
                Err(e) => println!("Failed to parse timestamp: {}", e),
            }
            if let Ok(val) = record[2].parse::<i64>() {
                close_offset.push(val)
            }
            if let Ok(val) = record[3].parse::<i64>() {
                open_offset.push(val)
            }
            if let Ok(val) = record[4].parse::<i64>() {
                end_offset.push(val)
            }
            project.push(project_id.name.to_owned());
        }
        let df =
            TimeData { chamber_id, start_time, close_offset, open_offset, end_offset, project };
        Ok(df)
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
    pub fn chunk(&self) -> Vec<TimeData> {
        let mut chunks = Vec::new();
        let len = self.start_time.len();
        let chunk_size = std::cmp::max(1, len / 100);

        for i in (0..len).step_by(chunk_size) {
            let end = (i + chunk_size).min(len);
            let chunk = TimeData {
                start_time: self.start_time[i..end].to_vec(),
                close_offset: self.close_offset[i..end].to_vec(),
                open_offset: self.open_offset[i..end].to_vec(),
                end_offset: self.end_offset[i..end].to_vec(),
                chamber_id: self.chamber_id[i..end].to_vec(),
                project: self.project[i..end].to_vec(),
            };
            chunks.push(chunk);
        }

        chunks
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

            let start_time = chrono::DateTime::from_timestamp(start_timestamp, 0).unwrap();
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

pub async fn query_cycles_async(
    conn: Arc<Mutex<Connection>>, // Arc<Mutex> for shared async access
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: Project,
) -> Result<TimeData> {
    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_cycles(&conn, start, end, project.name)
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
pub fn read_time_csv<P: AsRef<Path>>(filename: P) -> Result<TimeData, Box<dyn Error>> {
    let file = File::open(filename)?;
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(file);

    // chamber_id,start_time,close_offset,open_offset,end_offset
    let mut chamber_id: Vec<String> = Vec::new();
    let mut start_time: Vec<DateTime<Utc>> = Vec::new();
    let mut close_offset: Vec<i64> = Vec::new();
    let mut open_offset: Vec<i64> = Vec::new();
    let mut end_offset: Vec<i64> = Vec::new();
    let mut project: Vec<String> = Vec::new();

    for r in rdr.records() {
        let record: &csv::StringRecord = &r?;
        chamber_id.push(record[0].to_owned());

        match NaiveDateTime::parse_from_str(&record[1], "%Y-%m-%d %H:%M:%S") {
            Ok(naive_dt) => {
                let dt_utc = match Helsinki.from_local_datetime(&naive_dt) {
                    LocalResult::Single(dt) => dt.with_timezone(&Utc),
                    LocalResult::Ambiguous(dt1, _) => dt1.with_timezone(&Utc),
                    LocalResult::None => {
                        eprintln!("Impossible local time {}\nFix or remove.", naive_dt);
                        process::exit(1)
                    },
                };
                start_time.push(dt_utc)
            },
            Err(e) => println!("Failed to parse timestamp: {}", e),
        }
        if let Ok(val) = record[2].parse::<i64>() {
            close_offset.push(val)
        }
        if let Ok(val) = record[3].parse::<i64>() {
            open_offset.push(val)
        }
        if let Ok(val) = record[4].parse::<i64>() {
            end_offset.push(val)
        }
    }
    let df = TimeData { chamber_id, start_time, close_offset, open_offset, end_offset, project };
    Ok(df)
}

pub fn insert_cycles(
    conn: &mut Connection,
    cycles: &TimeData,
    project: &String,
) -> Result<(usize, usize)> {
    let close_vec = &cycles.close_offset;
    let open_vec = &cycles.open_offset;
    let end_vec = &cycles.end_offset;
    let chamber_vec = &cycles.chamber_id;
    let start_vec = cycles.start_time.iter().map(|dt| dt.timestamp()).collect::<Vec<i64>>();

    let tx = conn.transaction()?;
    let mut duplicates = 0;
    let mut inserted = 0;
    // let site_id = "oulanka_fen"; // Example site

    //   Prepare the statements **before** the loop
    let mut insert_stmt = tx.prepare(
        "INSERT INTO cycles (start_time, close_offset, open_offset, end_offset, chamber_id, project_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;

    let mut check_stmt = tx.prepare(
        "SELECT 1 FROM cycles WHERE start_time = ?1 AND chamber_id = ?2 AND project_id = ?3",
    )?;

    println!("Pushing data!");
    for i in 0..start_vec.len() {
        // Check for duplicates first
        let mut rows = check_stmt.query(params![start_vec[i], chamber_vec[i], project])?;
        if rows.next()?.is_some() {
            //   Duplicate found, skip insert
            duplicates += 1;
            println!(
                "Warning: Duplicate record found for start_time: {}, chamber_id: {}, site: {}",
                start_vec[i], chamber_vec[i], project
            );
        } else {
            //   Insert new record
            insert_stmt.execute(params![
                start_vec[i],
                close_vec[i],
                open_vec[i],
                end_vec[i],
                chamber_vec[i],
                project,
            ])?;
            inserted += 1;
        }
    }

    drop(insert_stmt);
    drop(check_stmt);

    tx.commit()?;
    println!("Inserted {} rows into cycles, {} duplicates skipped.", inserted, duplicates);

    Ok((inserted, duplicates))
}
fn get_time_data(path: &str) -> Result<TimeData, Box<dyn Error>> {
    let time_paths = get_paths::get_paths(path.to_owned(), "time")?;
    let mut all_times = TimeData::new();
    for path in time_paths {
        let res = read_time_csv(&path)?;
        if res.validate_lengths() {
            all_times.chamber_id.extend(res.chamber_id);
            all_times.start_time.extend(res.start_time);
            all_times.close_offset.extend(res.close_offset);
            all_times.open_offset.extend(res.open_offset);
            all_times.end_offset.extend(res.end_offset);
            // timev.push(res);
        }
    }
    Ok(all_times)
}

fn try_all_formats(
    path: &Path,
    project: &Project,
) -> Result<(TimeData, &'static str), Box<dyn Error>> {
    let parsers: Vec<Box<dyn TimeFormatParser>> =
        vec![Box::new(DefaultFormat), Box::new(OulankaManualFormat)];

    for parser in parsers {
        match parser.parse(path, project) {
            Ok(data) => return Ok((data, parser.name())),
            Err(_) => continue,
        }
    }

    Err("No known time format could parse this file.".into())
}
