use crate::instruments::instruments::get_or_insert_instrument;
use crate::instruments::instruments::{Instrument, InstrumentType};
use crate::processevent::{
    InsertEvent, ProcessEvent, ProcessEventSink, ProgressEvent, QueryEvent, ReadEvent,
};
use crate::project::Project;
use crate::traits::EqualLen;
use crate::utils::ensure_utf8;
use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::{Tz, UTC};
use rusqlite::{params, Connection, Result};
use std::error::Error;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task;

use crate::datatype::DataType;
use crate::utils::get_or_insert_data_file;
use std::path::PathBuf;
enum ParserType {
    Oulanka(OulankaManualFormat),
    Default(DefaultFormat),
}

impl TimeFormatParser for ParserType {
    fn name(&self) -> &'static str {
        match self {
            ParserType::Oulanka(p) => p.name(),
            ParserType::Default(p) => p.name(),
        }
    }

    fn parse(
        &self,
        path: &Path,
        tz: &Tz,
        project: &Project,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) -> Result<TimeData, Box<dyn Error>> {
        match self {
            ParserType::Oulanka(p) => p.parse(path, tz, project, progress_sender),
            ParserType::Default(p) => p.parse(path, tz, project, progress_sender),
        }
    }
}

pub trait TimeFormatParser {
    fn parse(
        &self,
        path: &Path,
        tz: &Tz,
        project_name: &Project,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) -> Result<TimeData, Box<dyn Error>>;

    fn name(&self) -> &'static str;
}

#[derive(Debug)]
pub struct TimeData {
    pub chamber_id: Vec<String>,
    pub start_time: Vec<i64>,
    pub close_offset: Vec<i64>,
    pub open_offset: Vec<i64>,
    pub end_offset: Vec<i64>,
    pub snow_depth: Vec<f64>,
    pub id: Vec<i64>,
    pub project_id: Vec<i64>,
    pub instrument_id: Vec<i64>,
}

impl EqualLen for TimeData {
    fn validate_lengths(&self) -> bool {
        let lengths = [
            &self.chamber_id.len(),
            &self.start_time.len(),
            &self.close_offset.len(),
            &self.open_offset.len(),
            &self.end_offset.len(),
            &self.snow_depth.len(),
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

impl OulankaManualFormat {
    fn parse_reader<R: Read>(
        &self,
        reader: R,
        path: &Path,
        tz: &Tz,
        project: &Project,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) -> Result<TimeData, Box<dyn Error>> {
        let mut rdr =
            csv::ReaderBuilder::new().flexible(true).has_headers(false).from_reader(reader);

        let mut date = NaiveDate::default();
        let mut instrument_serial = Vec::new();
        let mut instrument_model = Vec::new();
        let mut insmodel = InstrumentType::default();
        let mut insserial = String::new();
        let mut measurement_time: i64 = 0;

        let mut chamber_id: Vec<String> = Vec::new();
        let mut start_time: Vec<i64> = Vec::new();
        let mut close_offset: Vec<i64> = Vec::new();
        let mut open_offset: Vec<i64> = Vec::new();
        let mut end_offset: Vec<i64> = Vec::new();
        let mut project_id_vec: Vec<i64> = Vec::new();
        let mut instrument_id_vec: Vec<i64> = Vec::new();
        let mut snow_in_chamber: Vec<f64> = Vec::new();

        let mut records = rdr.records();

        if let Some(result) = records.next() {
            let date_str = result?.get(1).unwrap_or("").to_string();
            match NaiveDate::parse_from_str(&date_str, "%y%m%d") {
                Ok(ndate) => date = ndate,
                Err(_) => {
                    let msg = format!("Failed to parse first row {} as YYMMDD", date_str);
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
            let model = result?.get(1).unwrap_or("").trim().to_string();

            if model.is_empty() {
                return Err("Instrument model field is empty".into());
            }

            if let Ok(parsed_model) = model.parse::<InstrumentType>() {
                insmodel = parsed_model;
            } else {
                return Err(format!("Couldn't parse '{}' as instrument model", model).into());
            }
        }

        if let Some(result) = records.next() {
            let record = result?; // unwrap once
                                  //
            let parsed_serial = record.get(1).unwrap_or("").trim().to_string();
            let row_string = record.iter().collect::<Vec<_>>().join(",");

            if parsed_serial.is_empty() {
                return Err(format!("Instrument serial field is empty '{}'", row_string).into());
            }

            insserial = parsed_serial
        }

        // Skip header row before data
        records.next();

        for (i, r) in records.enumerate() {
            let record = match r {
                Ok(rec) => rec,
                Err(e) => {
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::RowFail(format!(
                        "Failed to read row {} in file {}, error: {}",
                        i + 6,
                        path.to_string_lossy(),
                        e
                    ))));
                    continue;
                },
            };

            match NaiveTime::parse_from_str(&record[1], "%H%M") {
                Ok(naive_time) => {
                    let naive_dt = date.and_time(naive_time);
                    let dt_utc = match tz.from_local_datetime(&naive_dt) {
                        LocalResult::Single(dt) => dt.with_timezone(&Utc),
                        LocalResult::Ambiguous(dt1, _) => dt1.with_timezone(&Utc),
                        LocalResult::None => {
                            eprintln!("Impossible local time {}. Fix or remove.", naive_dt);
                            continue;
                        },
                    };

                    let dt_unix = dt_utc.timestamp();
                    start_time.push(dt_unix - 60);
                },
                Err(_) => {
                    let row_string = record.iter().collect::<Vec<_>>().join(",");
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::RowFail(format!(
                        "Failed to parse datetime on row {}: '{}' in file {}",
                        i + 6,
                        row_string,
                        path.to_string_lossy()
                    ))));
                    continue;
                },
            }

            if let Some(snow_str) = record.get(2) {
                match snow_str.parse::<f64>() {
                    Ok(value) => snow_in_chamber.push(value / 100.),
                    Err(e) => {
                        snow_in_chamber.push(0.0);
                        eprintln!("No snow_depth supplied, using 0.0");
                    },
                }
            } else {
                snow_in_chamber.push(0.0);
                eprintln!("Missing column 2 in row {}", i + 1);
            }

            let conn = Connection::open("fluxrs.db").expect("Failed to open database");
            let instrument = Instrument { model: insmodel, serial: insserial.clone(), id: None };
            let instrument_id = get_or_insert_instrument(&conn, &instrument, project.id.unwrap())?;

            chamber_id.push(record[0].to_owned());
            instrument_model.push(insmodel);
            instrument_serial.push(insserial.clone());
            close_offset.push(60);
            open_offset.push(measurement_time + 60);
            end_offset.push(measurement_time + 120);
            project_id_vec.push(project.id.unwrap());
            instrument_id_vec.push(instrument_id)
        }
        let id = Vec::new();

        Ok(TimeData {
            chamber_id,
            start_time,
            close_offset,
            open_offset,
            end_offset,
            id,
            snow_depth: snow_in_chamber,
            project_id: project_id_vec,
            instrument_id: instrument_id_vec,
        })
    }
}
impl TimeFormatParser for OulankaManualFormat {
    fn name(&self) -> &'static str {
        "Oulanka Manual Format"
    }

    fn parse(
        &self,
        path: &Path,
        tz: &Tz,
        project: &Project,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) -> Result<TimeData, Box<dyn Error>> {
        let file = std::fs::File::open(path)
            .map_err(|e| format!("failed to open file {}: {}", path.display(), e))?;

        self.parse_reader(file, path, tz, project, progress_sender)
            .map_err(|e| format!("{}", e).into())
    }
}

struct DefaultFormat;

impl DefaultFormat {
    fn parse_reader<R: Read>(
        &self,
        reader: R,
        path: &Path,
        tz: &Tz,
        project: &Project,
    ) -> Result<TimeData, Box<dyn Error>> {
        let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(reader);

        let mut chamber_id: Vec<String> = Vec::new();
        let mut start_time: Vec<i64> = Vec::new();
        let mut close_offset: Vec<i64> = Vec::new();
        let mut open_offset: Vec<i64> = Vec::new();
        let mut end_offset: Vec<i64> = Vec::new();
        let mut snow_in_chamber: Vec<f64> = Vec::new();
        let mut project_id: Vec<i64> = Vec::new();
        let mut instrument_id: Vec<i64> = Vec::new();

        let mut parsed_any = false;
        let file_display = path.display().to_string();

        let conn = Connection::open("fluxrs.db").expect("Failed to open database");
        let instrument = Instrument {
            model: project.instrument.model,
            serial: project.instrument.serial.clone(),
            id: None,
        };
        let ins_id = get_or_insert_instrument(&conn, &instrument, project.id.unwrap())?;

        for (row_idx, r) in rdr.records().enumerate() {
            let record = r.map_err(|e| {
                format!("failed to read CSV row {} from {}: {}", row_idx + 1, path.display(), e)
            })?;

            if record.len() < 5 {
                return Err(format!(
                    "file {} row {}: expected at least 5 columns, got {}",
                    path.display(),
                    row_idx + 1,
                    record.len()
                )
                .into());
            }

            let ts_str = record.get(1).ok_or_else(|| {
                Box::<dyn std::error::Error>::from(format!(
                    "file {} row {}: missing timestamp column (index 1)",
                    path.display(),
                    row_idx + 1,
                ))
            })?; // ts_str: &str

            let naive_dt =
                NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%d %H:%M:%S").map_err(|e| {
                    Box::<dyn std::error::Error>::from(format!(
                        "failed to parse timestamp '{}' in file {} (row {}): {}",
                        ts_str,
                        path.display(),
                        row_idx + 1,
                        e
                    ))
                })?;

            let dt_utc = match tz.from_local_datetime(&naive_dt) {
                LocalResult::Single(dt) => dt.with_timezone(&UTC),
                LocalResult::Ambiguous(dt1, _) => dt1.with_timezone(&UTC),
                LocalResult::None => {
                    return Err(format!(
                "invalid local time '{}' in file {} (row {}): nonexistent timestamp in timezone Helsinki",
                naive_dt,
                path.display(),
                row_idx + 1,
            ).into());
                },
            };

            let dt_unix = dt_utc.timestamp();

            chamber_id.push(record[0].to_owned());
            start_time.push(dt_unix);
            close_offset.push(record[2].parse::<i64>().unwrap_or(0));
            open_offset.push(record[3].parse::<i64>().unwrap_or(0));
            end_offset.push(record[4].parse::<i64>().unwrap_or(0));
            project_id.push(project.id.unwrap());
            instrument_id.push(ins_id);
            snow_in_chamber.push(0.0);

            parsed_any = true;
        }

        if !parsed_any {
            return Err(format!(
                "failed to parse cycles from file {}: no valid rows found",
                file_display
            )
            .into());
        }
        let id = Vec::new();
        let df = TimeData {
            chamber_id,
            start_time,
            close_offset,
            open_offset,
            end_offset,
            id,
            snow_depth: snow_in_chamber,
            project_id,
            instrument_id,
        };
        Ok(df)
    }
}
impl TimeFormatParser for DefaultFormat {
    fn name(&self) -> &'static str {
        "Default time format"
    }

    fn parse(
        &self,
        path: &Path,
        tz: &Tz,
        project: &Project,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) -> Result<TimeData, Box<dyn Error>> {
        let file = std::fs::File::open(path)
            .map_err(|e| format!("failed to open file {}: {}", path.display(), e))?;

        self.parse_reader(file, path, tz, project).map_err(|e| {
            format!("failed to parse cycles from file {}: {}", path.display(), e).into()
        })
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
            id: Vec::new(),
            snow_depth: Vec::new(),
            project_id: Vec::new(),
            instrument_id: Vec::new(),
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
                id: self.id[i..end].to_vec(),
                snow_depth: self.snow_depth[i..end].to_vec(),
                project_id: self.project_id[i..end].to_vec(),
                instrument_id: self.instrument_id[i..end].to_vec(),
            };
            chunks.push(chunk);
        }

        chunks
    }
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&String, &i64, &i64, &i64, &i64, &f64, &i64, &i64, &i64)> {
        self.chamber_id
            .iter()
            .zip(&self.start_time)
            .zip(&self.close_offset)
            .zip(&self.open_offset)
            .zip(&self.end_offset)
            .zip(&self.snow_depth)
            .zip(&self.id)
            .zip(&self.project_id)
            .zip(&self.instrument_id)
            .map(
                |(
                    (((((((chamber, start), close), open), end), snow_depth), id), project_id),
                    instrument_id,
                )| {
                    (chamber, start, close, open, end, snow_depth, id, instrument_id, project_id)
                },
            )
    }
}
pub fn query_cycles(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: Project,
) -> Result<TimeData> {
    println!("Querying cycles");
    let mut stmt = conn.prepare(
        "SELECT c.chamber_id, c.start_time, c.close_offset, c.open_offset, c.end_offset, c.snow_depth, c.id, i.id AS instrument_id, p.id
         FROM cycles c
         LEFT JOIN instruments i ON c.instrument_link = i.id
         LEFT JOIN projects p ON c.project_link = p.id
         WHERE c.start_time BETWEEN ?1 AND ?2
         AND c.project_link = ?3
         ORDER BY c.start_time",
    )?;

    let mut times = TimeData::new();
    let cycle_iter =
        stmt.query_map(params![start.timestamp(), end.timestamp(), project.id.unwrap()], |row| {
            let chamber_id: String = row.get(0)?;
            let start_timestamp: i64 = row.get(1)?;
            let close_offset: i64 = row.get(2)?;
            let open_offset: i64 = row.get(3)?;
            let end_offset: i64 = row.get(4)?;
            let snow_depth: f64 = row.get(5)?;
            let id: i64 = row.get(6)?;
            let instrument_id: i64 = row.get(7)?;
            let project_id: i64 = row.get(8)?;

            times.chamber_id.push(chamber_id);
            times.start_time.push(start_timestamp);
            times.close_offset.push(close_offset);
            times.open_offset.push(open_offset);
            times.end_offset.push(end_offset);
            times.id.push(id);
            times.snow_depth.push(snow_depth);
            times.project_id.push(project_id);
            times.instrument_id.push(instrument_id);

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
        query_cycles(&conn, start, end, project)
    })
    .await;
    match result {
        Ok(inner) => inner,
        Err(e) => {
            // Convert JoinError into rusqlite::Error::ExecuteReturnedResults or custom error
            println!("{}", e);
            Err(rusqlite::Error::ExecuteReturnedResults) // or log `e` if needed
        },
    }
}

pub fn insert_cycles(
    conn: &mut Connection,
    cycles: &TimeData,
    project_id: &i64,
    file_id: &i64,
) -> Result<(usize, usize)> {
    let close_vec = &cycles.close_offset;
    let open_vec = &cycles.open_offset;
    let end_vec = &cycles.end_offset;
    let chamber_vec = &cycles.chamber_id;
    let snow_vec = &cycles.snow_depth;
    let ins_id_vec = &cycles.instrument_id;
    let proj_id_vec = &cycles.project_id;
    let start_vec = &cycles.start_time;

    if !(close_vec.len() == open_vec.len()
        && open_vec.len() == end_vec.len()
        && end_vec.len() == chamber_vec.len()
        && chamber_vec.len() == snow_vec.len()
        && snow_vec.len() == ins_id_vec.len()
        && ins_id_vec.len() == proj_id_vec.len()
        && proj_id_vec.len() == start_vec.len())
    {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Mismatched vector lengths:\n\
            close: {}, open: {}, end: {}, chamber: {}, snow: {}, ins_id: {}, proj_id: {}, start: {}",
                    close_vec.len(),
                    open_vec.len(),
                    end_vec.len(),
                    chamber_vec.len(),
                    snow_vec.len(),
                    ins_id_vec.len(),
                    proj_id_vec.len(),
                    start_vec.len()
                ),
            )),
        ));
    }

    let tx = conn.transaction()?;
    let mut duplicates = 0;
    let mut inserted = 0;

    // Cache so we don't repeatedly look up the same instrument in the DB

    // Prepare statements BEFORE the loop
    // 1) Find instrument id for this project + model + serial

    // 2) Insert new instrument for this project

    // 3) Check for duplicate cycle
    let mut check_stmt = tx.prepare(
        "SELECT 1 FROM cycles
         WHERE start_time = ?1
           AND chamber_id = ?2
           AND project_link = ?3",
    )?;

    // 4) Insert new cycle row (note: instrument_link instead of model/serial)
    let mut insert_cycle_stmt = tx.prepare(
        "INSERT INTO cycles (
            start_time,
            close_offset,
            open_offset,
            end_offset,
            chamber_id,
            snow_depth,
            project_link,
            instrument_link,
            file_link
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;

    for i in 0..start_vec.len() {
        // Check for duplicates first
        let mut rows = check_stmt.query(params![start_vec[i], chamber_vec[i], project_id])?;
        if rows.next()?.is_some() {
            duplicates += 1;
            println!(
                "Warning: Duplicate record found for start_time: {}, chamber_id: {}, project_id: {}",
                start_vec[i], chamber_vec[i], project_id
            );
            continue;
        }

        // Resolve instrument_id: from cache -> from DB -> insert new
        let instrument = match get_instrument_by_project_and_id(&tx, *project_id, ins_id_vec[i])? {
            Some(inst) => inst,
            None => {
                eprintln!("Instrument not found for project={} id={}", project_id, ins_id_vec[i]);
                return Ok((0, 0)); // or whatever your function returns
            },
        };
        // let key = (instrument.model.to_string(), instrument.serial.clone());

        // let instrument_id = if let Some(id) = instrument_cache.get(&key) {
        //     *id
        // } else {
        //     // Try to find in DB
        //     let mut instr_rows =
        //         get_instrument_id_stmt.query(params![project_id, &model_str, &serial_str])?;
        //
        //     let id = if let Some(row) = instr_rows.next()? {
        //         row.get::<_, i64>(0)?
        //     } else {
        //         // Not found -> insert new instrument
        //         insert_instrument_stmt.execute(params![&model_str, &serial_str, project_id])?;
        //         tx.last_insert_rowid()
        //     };
        //
        //     instrument_cache.insert(key, id);
        //     id
        // };

        // Insert cycle row with instrument_link
        insert_cycle_stmt.execute(params![
            start_vec[i],
            close_vec[i],
            open_vec[i],
            end_vec[i],
            chamber_vec[i],
            snow_vec[i],
            project_id,
            instrument.id,
            file_id,
        ])?;

        inserted += 1;
    }

    drop(insert_cycle_stmt);
    drop(check_stmt);

    tx.commit()?;
    println!("Inserted {} rows into cycles, {} duplicates skipped.", inserted, duplicates);

    Ok((inserted, duplicates))
}

pub fn try_all_formats(
    path: &Path,
    tz: &Tz,
    project: &Project,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) -> Result<(TimeData, &'static str), Box<dyn Error>> {
    let parsers =
        vec![ParserType::Oulanka(OulankaManualFormat), ParserType::Default(DefaultFormat)];

    for parser in parsers {
        let _ = progress_sender.send(ProcessEvent::Progress(ProgressEvent::Generic(format!(
            "Trying parser: {} for file {}",
            parser.name(),
            path.to_string_lossy()
        ))));
        match parser.parse(path, tz, project, progress_sender.clone()) {
            Ok(data) => return Ok((data, parser.name())),
            Err(e) => {
                let _ = progress_sender
                    .send(ProcessEvent::Progress(ProgressEvent::Generic(format!("{}", e))));
                continue;
            },
        }
    }

    Err("Could not parse as a cycle file, check that your file is correct.".into())
}
pub fn upload_cycle_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    let mut all_times = TimeData::new();

    for path in &selected_paths {
        if ensure_utf8(path).is_err() {
            let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                path.to_string_lossy().to_string(),
                "Invalid UTF-8, make sure your file is UTF-8 encoded.".to_owned(),
            )));
            continue;
        }
        let project_id = project.id.unwrap();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                // Optionally notify UI:
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::cycle_fail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                return (); // or `continue` if this is in a loop
            },
        };

        let file_id = match get_or_insert_data_file(conn, DataType::Cycle, file_name, project_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                // Optionally notify UI
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue; // or return if not inside a loop
            },
        };
        match try_all_formats(path, &tz, project, progress_sender.clone()) {
            //   Pass `path` directly
            Ok((res, _)) => {
                if res.validate_lengths() {
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::File(
                        path.to_string_lossy().to_string(),
                    )));
                    match insert_cycles(conn, &res, &project.id.unwrap(), &file_id) {
                        Ok((row_count, duplicates)) => {
                            let _ = progress_sender.send(ProcessEvent::Insert(
                                InsertEvent::cycle_okskip(row_count, duplicates),
                            ));
                        },
                        Err(e) => {
                            let _ = progress_sender
                                .send(ProcessEvent::Insert(InsertEvent::Fail(e.to_string())));
                        },
                    }
                } else {
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                        path.to_string_lossy().to_string(),
                        "Skipped, data vectors are not equal length, check your data file."
                            .to_owned(),
                    )));
                }
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::cycle_fail(
                    path.to_string_lossy().to_string(),
                    e.to_string(),
                )));
            },
        }
    }
    let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
}
pub fn get_instrument_by_project_and_id(
    conn: &Connection,
    project_link: i64,
    instrument_id: i64,
) -> Result<Option<Instrument>> {
    let mut stmt = conn.prepare(
        "SELECT id, instrument_model, instrument_serial
         FROM instruments
         WHERE project_link = ?1 AND id = ?2",
    )?;

    let row_result = stmt.query_row(params![project_link, instrument_id], |row| {
        let id: i64 = row.get(0)?;
        let model_str: String = row.get(1)?;
        let serial: String = row.get(2)?;

        // Use your FromStr implementation for InstrumentType
        let model = model_str.parse::<InstrumentType>().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?;

        Ok(Instrument { id: Some(id), model, serial })
    });

    match row_result {
        Ok(inst) => Ok(Some(inst)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::Path;

    fn mock_project() -> Project {
        Project::default()
    }

    fn mock_path() -> &'static Path {
        Path::new("test.csv")
    }

    #[test]
    fn parses_valid_csv_with_two_chambers() {
        let csv = "\
,240621
,120
,LI7810
,TG10-01169
chamber_id,start_time,snow_depth
CH1,1234,0
CH2,1250,0";

        let (progress_sender, _) = mpsc::unbounded_channel();
        let parser = OulankaManualFormat;
        let reader = Cursor::new(csv);
        let result =
            parser.parse_reader(reader, mock_path(), &UTC, &mock_project(), progress_sender);

        assert!(result.is_ok(), "Expected successful parsing");
        let data = result.unwrap();
        assert_eq!(data.chamber_id.len(), 2);
        assert_eq!(data.project_id[0], 0);
    }

    #[test]
    fn fails_on_invalid_date() {
        let csv = "\
,notadate
,120
,LI7810
,TG10-01169
chamber_id,start_time,snow_depth
CH1,1234,0";

        let (progress_sender, _) = mpsc::unbounded_channel();
        let parser = OulankaManualFormat;
        let reader = Cursor::new(csv);
        let result =
            parser.parse_reader(reader, mock_path(), &UTC, &mock_project(), progress_sender);

        assert!(result.is_err(), "Expected parsing to fail on invalid date");
    }

    #[test]
    fn fails_on_invalid_instrument_model() {
        let csv = "\
,240621
,120
,LI7811
,TG10-01169
chamber_id,start_time,snow_depth
CH1,1234,0";

        let (progress_sender, _) = mpsc::unbounded_channel();
        let parser = OulankaManualFormat;
        let reader = Cursor::new(csv);
        let result =
            parser.parse_reader(reader, mock_path(), &UTC, &mock_project(), progress_sender);

        assert!(result.is_err(), "Expected parsing to fail on invalid instrument model");
    }

    #[test]
    fn skips_rows_with_invalid_time_format() {
        let csv = "\
,240621
,120
,LI7810
,TG10-01169
chamber_id,start_time,snow_depth
CH1,12AA,
CH2,1250,";

        let (progress_sender, _) = mpsc::unbounded_channel();
        let parser = OulankaManualFormat;
        let reader = Cursor::new(csv);
        let result =
            parser.parse_reader(reader, mock_path(), &UTC, &mock_project(), progress_sender);

        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.chamber_id.len(), 1); // Only CH2 should succeed
    }
}
