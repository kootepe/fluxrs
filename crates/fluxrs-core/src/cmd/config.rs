use crate::cycle_processor::{Datasets, Infra, Processor};
use crate::data_formats::chamberdata::{query_chamber_async, upload_chamber_metadata_async};
use crate::data_formats::gasdata::query_gas_async;
use crate::data_formats::heightdata::{query_height_async, upload_height_data_async};
use crate::data_formats::meteodata::{query_meteo_async, upload_meteo_data_async};
use crate::data_formats::timedata::{query_cycles_async, upload_cycle_data_async};
use crate::datatype::DataType;
use crate::gastype::GasType;
use crate::instruments::instruments::upload_gas_data_async;
use crate::instruments::instruments::{Instrument, InstrumentType};
use crate::mode::Mode;
use crate::processevent::{
    InsertEvent, ProcessEvent, ProcessEventSink, ProgressEvent, QueryEvent, ReadEvent,
};
use crate::project::Project;

use chrono::{DateTime, Utc};
use chrono_tz::{Tz, UTC};
use glob::glob;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
// use tokio::sync::mpsc;
use std::sync::mpsc;
use tokio::sync::mpsc::{
    error::TryRecvError, unbounded_channel, UnboundedReceiver, UnboundedSender,
};

/* =================== Public configuration types =================== */
// std::sync::mpsc::Receiver<processevent::ProcessEvent>
#[derive(Debug)]
pub struct Config {
    pub db_path: PathBuf,
    pub action: Action,
    pub progress_receiver: Option<UnboundedReceiver<ProcessEvent>>,
}

#[derive(Debug, Clone)]
pub enum Action {
    ProjectCreate(ProjectCreate),
    Upload(Upload),
    Run(Run),
}

#[derive(Debug, Clone)]
pub struct ProjectCreate {
    pub name: String,
    pub instrument: InstrumentType,
    pub instrument_serial: String,
    pub main_gas: GasType,
    pub deadband: f64,
    pub min_calc_len: f64,
    pub mode: Mode,
    pub tz: Tz,
}

#[derive(Debug, Clone)]
pub struct Upload {
    pub project: String,
    pub file_type: DataType,
    pub inputs: Vec<String>,
    pub use_newest: bool,
    pub tz: Option<Tz>, // only meaningful for Cycle
}

#[derive(Debug, Clone)]
pub struct Run {
    pub project: String,
    pub instrument: Option<InstrumentType>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub use_newest: bool,
    pub tz: Option<Tz>,
    pub initiate_data: bool,
}

/* =================== Error type (no process::exit) =================== */

#[derive(thiserror::Error, Debug)]
pub enum CmdError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
}

/* =================== Entry point =================== */

impl Config {
    pub fn run(&mut self) -> Result<(), CmdError> {
        match &self.action.clone() {
            Action::ProjectCreate(p) => self.run_project_create(p),
            Action::Upload(u) => self.run_upload(u),
            Action::Run(r) => self.run_process(r),
        }
    }
}

/* =================== Actions =================== */

impl Config {
    fn run_project_create(&self, p: &ProjectCreate) -> Result<(), CmdError> {
        let instrument =
            Instrument { model: p.instrument, serial: p.instrument_serial.clone(), id: None };
        let project = Project {
            id: None,
            name: p.name.clone(),
            instrument,
            main_gas: Some(p.main_gas),
            deadband: p.deadband,
            min_calc_len: p.min_calc_len,
            mode: p.mode,
            tz: p.tz,
            upload_from: None,
        };

        // Project::save expects Option<String> for db path in your API
        let db = Some(self.db_path.display().to_string());
        match Project::save(db, &project) {
            Ok(_) => {
                println!("Project '{}' created successfully.", project.name);
                Ok(())
            },
            Err(e) => {
                Err(CmdError::Msg(format!("Failed to create project '{}': {}", project.name, e)))
            },
        }
    }

    fn run_upload(&mut self, u: &Upload) -> Result<(), CmdError> {
        self.handle_progress_messages();
        let dbp_str = self.db_path.display().to_string();
        let mut conn = Connection::open(&self.db_path)?;

        let project = Project::load(Some(dbp_str.clone()), &u.project).ok_or_else(|| {
            CmdError::Msg(format!("No project found in {} with name: {}", dbp_str, u.project))
        })?;

        // prefer CLI tz, then project tz, then UTC
        let tz = u.tz.or(Some(project.tz)).unwrap_or(UTC);

        // resolve files from hybrid inputs
        let files0 = resolve_inputs(&u.inputs);

        // incremental cutoff per dataset
        let files = if u.use_newest {
            match last_ingested_ts(&conn, u.file_type) {
                Some(since) => {
                    let v = filter_since(files0, since);
                    if v.is_empty() {
                        println!("No files with mtime ≥ {} among inputs {:?}", since, u.inputs);
                    } else {
                        println!("Incremental upload: {} file(s) since {}", v.len(), since);
                    }
                    v
                },
                None => {
                    println!("No previous data in DB; uploading all resolved files.");
                    files0
                },
            }
        } else {
            files0
        };

        if files.is_empty() {
            return Ok(());
        }

        let (progress_sender, progress_receiver) = unbounded_channel::<ProcessEvent>();
        self.progress_receiver = Some(progress_receiver);

        let sender_clone = progress_sender.clone();
        match u.file_type {
            DataType::Gas => upload_gas_data_async(files, &mut conn, &project, tz, sender_clone),
            DataType::Cycle => {
                upload_cycle_data_async(files, &mut conn, &project, tz, sender_clone)
            },
            DataType::Meteo => {
                upload_meteo_data_async(files, &mut conn, &project, tz, sender_clone)
            },
            DataType::Height => {
                upload_height_data_async(files, &mut conn, &project, tz, sender_clone)
            },
            DataType::Chamber => {
                upload_chamber_metadata_async(files, &mut conn, &project, tz, sender_clone)
            },
        }

        self.handle_progress_messages();
        drop(progress_sender);
        // let _ = progress_thread.join();
        Ok(())
    }

    fn run_process(&mut self, r: &Run) -> Result<(), CmdError> {
        self.handle_progress_messages();
        let dbp_str = self.db_path.display().to_string();
        let project = Project::load(Some(dbp_str.clone()), &r.project)
            .ok_or_else(|| CmdError::Msg(format!("No project found with name: {}", r.project)))?;

        let conn = Connection::open(&self.db_path)?;

        let start_date = r
            .start
            .or_else(|| if r.use_newest { get_newest_measurement_day(&conn) } else { None })
            .unwrap_or_else(|| get_newest_measurement_day(&conn).unwrap_or_default());

        let end_date = r.end.unwrap_or_else(Utc::now);
        if start_date > end_date {
            return Err(CmdError::Msg("Start time can't be after end time.".to_string()));
        }
        println!("Initiating from {} to {}", start_date, end_date);

        let arc_conn = Arc::new(Mutex::new(conn));

        let (progress_sender, mut progress_receiver) = unbounded_channel::<ProcessEvent>();
        self.progress_receiver = Some(progress_receiver);

        let proj = project.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap(); // keep as-is (per your request to skip #8)

        // let progress_receiver_thread = std::thread::spawn(move || {
        //     while let Some(_) = progress_receiver.blocking_recv() {
        //         self.handle_progress_messages();
        //     }
        // });

        let handle = runtime.spawn(async move {
            let cycles_result =
                query_cycles_async(arc_conn.clone(), start_date, end_date, proj.clone()).await;
            let gas_result =
                query_gas_async(arc_conn.clone(), start_date, end_date, proj.clone()).await;
            let meteo_result =
                query_meteo_async(arc_conn.clone(), start_date, end_date, proj.clone()).await;
            let height_result =
                query_height_async(arc_conn.clone(), start_date, end_date, proj.clone()).await;
            let chamber_result = query_chamber_async(arc_conn.clone(), proj.clone()).await;

            match (cycles_result, gas_result, meteo_result, height_result, chamber_result) {
                (Ok(times), Ok(gas_data), Ok(meteo_data), Ok(height_data), Ok(chamber_data)) => {
                    let _ = progress_sender.send(ProcessEvent::Query(QueryEvent::QueryComplete));
                    if !times.start_time.is_empty() && !gas_data.is_empty() {
                        let processor = Processor::new(
                            project.clone(),
                            Datasets {
                                gas: Arc::new(gas_data),
                                meteo: meteo_data,
                                height: height_data,
                                chambers: chamber_data,
                            },
                            Infra { conn: arc_conn, progress: progress_sender },
                        );
                        processor.run_processing_dynamic(times).await;
                    } else if times.start_time.is_empty() && gas_data.is_empty() {
                        let msg = "No gas data or cycles found.";
                        let _ = progress_sender.send(ProcessEvent::Done(Err(msg.to_owned())));
                    } else if times.start_time.is_empty() {
                        let msg = "No cycles found.";
                        let _ = progress_sender.send(ProcessEvent::Done(Err(msg.to_owned())));
                    } else {
                        let msg = "No gas data found.";
                        let _ = progress_sender.send(ProcessEvent::Done(Err(msg.to_owned())));
                    }
                },
                e => eprintln!("Failed to query database: {:?}", e),
            }
        });
        println!("Running drain messages");
        loop {
            // drain anything that arrived since last tick
            self.drain_progress_messages();

            // if the async task is finished, break out
            if handle.is_finished() {
                break;
            }

            // avoid busy-spinning a core at 100%
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        runtime.block_on(handle).unwrap();
        self.drain_progress_messages();
        // let _ = progress_receiver_thread.join();
        Ok(())
    }

    pub fn handle_progress_messages(&mut self) {
        // Step 1: take the receiver out, leaving None in its place
        if let Some(mut receiver) = self.progress_receiver.take() {
            // Step 2: process all pending events
            drain_progress_messages(self, &mut receiver);

            // Step 3: put it back
            self.progress_receiver = Some(receiver);
        }
    }
    fn apply_single_event(&mut self, msg: ProcessEvent) {
        match msg {
            ProcessEvent::Query(ev) => self.on_query_event(&ev),
            ProcessEvent::Progress(ev) => self.on_progress_event(&ev),
            ProcessEvent::Read(ev) => self.on_read_event(&ev),
            ProcessEvent::Insert(ev) => self.on_insert_event(&ev),
            ProcessEvent::Done(res) => self.on_done(&res),
        }
    }
    fn drain_progress_messages(&mut self) {
        if let Some(mut rx) = self.progress_receiver.take() {
            loop {
                match rx.try_recv() {
                    Ok(msg) => {
                        self.apply_single_event(msg);
                    },
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => {
                        break;
                    },
                }
            }

            // put it back so we can keep using it later
            self.progress_receiver = Some(rx);
        }
    }
}
pub fn drain_progress_messages<T: ProcessEventSink>(
    sink: &mut T,
    receiver: &mut UnboundedReceiver<ProcessEvent>,
) {
    loop {
        match receiver.try_recv() {
            Ok(msg) => match msg {
                ProcessEvent::Query(ev) => sink.on_query_event(&ev),
                ProcessEvent::Progress(ev) => sink.on_progress_event(&ev),
                ProcessEvent::Read(ev) => sink.on_read_event(&ev),
                ProcessEvent::Insert(ev) => sink.on_insert_event(&ev),
                ProcessEvent::Done(res) => sink.on_done(&res),
            },

            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
}
impl ProcessEventSink for Config {
    fn on_query_event(&mut self, ev: &QueryEvent) {
        match ev {
            QueryEvent::InitStarted => {},
            QueryEvent::InitEnded => {},
            QueryEvent::QueryComplete => {
                println!("Finished queries.");
            },
            QueryEvent::NoGasData(start_time) => {
                println!("No gas data found for cycle at {}", start_time);
            },
            QueryEvent::HeightFail(msg) => {
                println!("{}", msg);
            },
            QueryEvent::DbFail(msg) => {
                println!("{}", msg);
            },
            QueryEvent::CyclesFail(msg) => {
                println!("{}", msg)
            },
            QueryEvent::NoGasDataDay(day) => {
                println!("No gas data found for cycles at day {}", day);
            },
        }
    }

    fn on_progress_event(&mut self, ev: &ProgressEvent) {
        match ev {
            ProgressEvent::Rows(_, _) => {},
            ProgressEvent::Recalced(_, _) => {},
            ProgressEvent::Day(date) => {
                println!("Loaded cycles from {}", date);
            },
            ProgressEvent::NoGas(msg) => {
                println!("gas missing: {}", msg);
            },
            ProgressEvent::CalculationStarted => {},
            ProgressEvent::Generic(msg) => {
                println!("{}", msg);
            },
        }
    }

    fn on_read_event(&mut self, ev: &ReadEvent) {
        match ev {
            ReadEvent::File(filename) => {
                println!("Read file: {}", filename);
            },
            ReadEvent::FileDetail(filename, detail) => {
                println!("Read file: {} {}", filename, detail);
            },
            ReadEvent::MeteoFail(filename, msg) => {
                println!("Could not parse as meteo file: {}, {}", filename, msg);
            },
            ReadEvent::GasFail(filename, msg) => {
                println!("Could not parse as gas file: {}, {}", filename, msg);
            },
            ReadEvent::HeightFail(filename, msg) => {
                println!("Could not parse as height file: {}, {}", filename, msg);
            },
            ReadEvent::CycleFail(filename, msg) => {
                println!("Could not parse as cycle file: {}, {}", filename, msg);
            },
            ReadEvent::MetadataFail(filename, msg) => {
                println!("Could not parse as chamber metadata file: {}, {}", filename, msg);
            },
            ReadEvent::FileRows(filename, rows) => {
                println!("Read file: {} with {} rows", filename, rows);
            },
            ReadEvent::RowFail(msg) => {
                println!("{}", msg);
            },
            ReadEvent::FileFail(filename, e) => {
                println!("Failed to read file {}, error: {}", filename, e);
            },
        }
    }

    fn on_insert_event(&mut self, ev: &InsertEvent) {
        match ev {
            InsertEvent::Ok(msg, rows) => {
                println!("{}{}", rows, msg);
            },
            InsertEvent::OkSkip(rows, skips) => {
                println!("Inserted {} rows, skipped {} duplicates.", rows, skips);
            },
            InsertEvent::CycleOkSkip(rows, skips) => {
                println!("Inserted {} cycles, skipped {} entries. Either they failed during calculation or are already in the db..", rows, skips);
                if skips == &0 {
                    println!("Inserted {} cycles.", rows);
                } else {
                    println!(
                        "Inserted {} cycles, skipped {} entries. Either something went wrong with the calculation or the cycles already exist in the db.",
                        rows, skips
                    );
                }
            },
            InsertEvent::Fail(e) => {
                println!("Failed to insert rows: {}", e);
            },
        }
    }

    fn on_done(&mut self, res: &Result<(), String>) {
        match res {
            Ok(()) => {
                println!("All processing finished.");
            },
            Err(e) => {
                println!("Processing finished with error: {}", e);
            },
        }
    }
}

/// Return all paths matching glob
fn get_file_paths(pattern: &str) -> Vec<PathBuf> {
    glob(pattern).expect("Failed to read glob pattern").filter_map(Result::ok).collect()
}

/// Return all paths whose filesystem mtime is >= `since` (UTC).
fn get_file_paths_since(pattern: &str, since: DateTime<Utc>) -> Vec<PathBuf> {
    let since_sys: SystemTime = SystemTime::from(since);
    glob(pattern)
        .expect("Failed to read glob pattern")
        .filter_map(Result::ok)
        .filter(|path| match fs::metadata(path).and_then(|m| m.modified()) {
            Ok(modified) => modified >= since_sys,
            Err(_) => false,
        })
        .collect()
}

/// If you still use it elsewhere, keep this table-agnostic "fluxes" max.
pub fn get_newest_measurement_day(conn: &Connection) -> Option<DateTime<Utc>> {
    let timestamp: Option<i64> =
        conn.query_row("SELECT MAX(start_time) FROM fluxes", [], |row| row.get(0)).ok()?;
    DateTime::from_timestamp(timestamp?, 0)
}

/// Per-dataset "last ingested" timestamp (improves incremental uploads).
fn last_ingested_ts(conn: &Connection, dt: DataType) -> Option<DateTime<Utc>> {
    let sql = match dt {
        DataType::Gas => "SELECT MAX(datetime) FROM gas",
        DataType::Meteo => "SELECT MAX(datetime) FROM meteo",
        DataType::Height => "SELECT MAX(datetime) FROM height",
        DataType::Cycle => "SELECT MAX(start_time) FROM cycles",
        DataType::Chamber => return None,
    };
    let ts: Option<i64> = conn.query_row(sql, [], |row| row.get(0)).ok().flatten();
    ts.and_then(|s| DateTime::from_timestamp(s, 0))
}
fn resolve_inputs(inputs: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for inp in inputs {
        if inp.contains('*') || inp.contains('?') || inp.contains('[') {
            match glob(inp) {
                Ok(paths) => out.extend(paths.filter_map(Result::ok)),
                Err(e) => eprintln!("Invalid glob '{}': {}", inp, e),
            }
        } else {
            out.push(PathBuf::from(inp));
        }
    }
    out
}

fn filter_since(files: Vec<PathBuf>, since: DateTime<Utc>) -> Vec<PathBuf> {
    let cutoff: SystemTime = SystemTime::from(since);
    files
        .into_iter()
        .filter(|p| {
            fs::metadata(p).and_then(|m| m.modified()).map(|t| t >= cutoff).unwrap_or(false)
        })
        .collect()
}
