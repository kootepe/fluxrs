use crate::gastype::GasType;
use crate::instruments::InstrumentType;
use crate::processevent::{InsertEvent, ProcessEvent, ProgressEvent, QueryEvent, ReadEvent};
use crate::ui::project_ui::Project;
use crate::ui::validation_ui::{run_processing_dynamic, Mode};
use crate::ui::validation_ui::{
    upload_cycle_data_async, upload_gas_data_async, upload_height_data_async,
    upload_meteo_data_async, DataType,
};

use crate::chamber::query_chamber_async;
use crate::data_formats::gasdata::query_gas_async;
use crate::data_formats::heightdata::query_height_async;
use crate::data_formats::meteodata::query_meteo_async;
use crate::data_formats::timedata::query_cycles_async;

use chrono::TimeZone;
use chrono::{DateTime, NaiveDate, Utc};
use glob::glob;
use rusqlite::{Connection, Result};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process;
// use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use std::fmt;

pub struct Config {
    pub project: Option<String>,
    pub paths: Option<String>,
    pub file_type: Option<DataType>,
    pub use_newest: bool,
    pub initiate_data: bool,
    pub instrument: Option<InstrumentType>,
    pub db_path: Option<String>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,

    // Extra for project creation
    pub create_project: bool,
    pub name: Option<String>,
    pub instrument_serial: Option<String>,
    pub main_gas: Option<GasType>,
    pub deadband: Option<f64>,
    pub min_calc_len: Option<f64>,
    pub mode: Option<Mode>,
}
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // let len: usize = self.measurement_dt_v.len();
        write!(
            f,
            "{:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            self.name,
            self.instrument,
            self.instrument_serial,
            self.main_gas,
            self.deadband,
            self.min_calc_len,
            self.mode
        )
    }
}
impl Config {
    pub fn print_proj(&self) {
        println!(
            "{:?} {:?} {:?} {:?} {:?} {:?}",
            self.name,
            self.instrument_serial,
            self.main_gas,
            self.deadband,
            self.min_calc_len,
            self.mode
        );
    }
    pub fn build(mut args: impl Iterator<Item = String>) -> Result<Config, &'static str> {
        println!("Running build");
        args.next(); // Skip the first argument (program name)

        let mut project: Option<String> = None;
        let mut paths: Option<String> = None;
        let mut file_type: Option<DataType> = None;
        let mut db_path: Option<String> = None;
        let mut instrument: Option<InstrumentType> = None;
        let mut start: Option<DateTime<Utc>> = None;
        let mut end: Option<DateTime<Utc>> = None;
        let mut use_newest = false;
        let mut initiate_data = false;

        let mut create_project = false;
        let mut name = None;
        let mut instrument_serial = None;
        let mut main_gas = None;
        let mut deadband = None;
        let mut min_calc_len = None;
        let mut mode = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" => {
                    print_help();
                    process::exit(0);
                },
                "-h" => {
                    print_help();
                    process::exit(0);
                },
                "--create-project" => {
                    create_project = true;
                    name = args.next();
                },
                "--serial" => {
                    instrument_serial = args.next();
                },
                "--main-gas" => {
                    if let Some(gas_str) = args.next() {
                        match gas_str.parse::<GasType>() {
                            Ok(gas) => main_gas = Some(gas),
                            Err(_) => {
                                eprintln!("Invalid gas type: '{}'", gas_str);
                                process::exit(1);
                            },
                        }
                    } else {
                        eprintln!("Expected a gas type after '--main-gas'");
                        process::exit(1);
                    }
                    // if let Some(gas_str) = args.next() {
                    //     main_gas = gas_str.parse::<GasType>().ok();
                    // }
                },
                "--deadband" => {
                    if let Some(db) = args.next() {
                        deadband = db.parse().ok();
                    }
                },
                "--min-calc-len" => {
                    if let Some(mcl) = args.next() {
                        min_calc_len = mcl.parse().ok();
                    }
                },
                "--mode" => {
                    if let Some(m) = args.next() {
                        mode = m.parse::<Mode>().ok();
                    }
                },

                "-i" | "--instrument" => {
                    instrument = parse_instrument(&mut args);
                },
                "-p" => {
                    project = args.next();
                },
                "--project" => {
                    if project.is_some() {
                        println!("Project already set.")
                    } else {
                        project = args.next();
                    }
                },
                // project name from db
                "-db" => {
                    db_path = args.next();
                },
                // start_time
                "-s" => {
                    if let Some(arg) = args.next() {
                        match parse_datetime(&arg) {
                            Ok(dt) => {
                                println!("Parsed: {} -> {}", arg, dt);
                                start = Some(dt);
                            },
                            Err(e) => eprintln!("Failed to parse '{}': {:?}", arg, e),
                        }
                    } else {
                        eprintln!("Expected a datetime string after '-s'");
                        process::exit(1)
                    }
                },

                // end_time
                "-e" => {
                    if let Some(arg) = args.next() {
                        match parse_datetime(&arg) {
                            Ok(dt) => {
                                println!("Parsed: {} -> {}", arg, dt);
                                end = Some(dt);
                            },
                            Err(e) => eprintln!("Failed to parse '{}': {:?}", arg, e),
                        }
                    } else {
                        eprintln!("Expected a datetime string after '-e'");
                        process::exit(1)
                    }
                },
                // start init
                // start init
                "--init" => {
                    initiate_data = true;
                },
                // use newest
                "-n" => {
                    use_newest = true;
                },
                "-newest" => {
                    use_newest = true;
                },
                // cycle data path
                // -t for time
                "-t" => {
                    paths = args.next();
                    file_type = Some(DataType::Cycle);
                },
                // gas data path
                // -g for gas
                "-g" => {
                    paths = args.next();
                    file_type = Some(DataType::Gas);
                },
                // height data path
                // -v for height
                "-v" => {
                    paths = args.next();
                    file_type = Some(DataType::Height);
                },
                //  meteo data path
                // -m for meteo
                "-m" => {
                    paths = args.next();
                    file_type = Some(DataType::Meteo);
                },
                // Ignore unknown arguments
                _ => {
                    println!("Unknown argument: {}", arg)
                },
            }
        }

        Ok(Config {
            create_project,
            project,
            paths,
            file_type,
            db_path,
            instrument,
            start,
            end,
            use_newest,
            initiate_data,
            instrument_serial,
            name,
            main_gas,
            deadband,
            min_calc_len,
            mode,
        })
    }
    pub fn run(&self) {
        if self.create_project {
            if let (
                Some(name),
                Some(inst),
                Some(serial),
                Some(main_gas),
                Some(deadband),
                Some(min_calc_len),
                Some(mode),
            ) = (
                &self.name,
                &self.instrument,
                &self.instrument_serial,
                self.main_gas,
                self.deadband,
                self.min_calc_len,
                &self.mode,
            ) {
                let project = Project {
                    name: name.clone(),
                    instrument: *inst,
                    instrument_serial: serial.clone(),
                    main_gas: Some(main_gas),
                    deadband,
                    min_calc_len,
                    mode: *mode,
                    upload_from: None,
                };

                // Save it — implement this however your Project system works
                match Project::save(self.db_path.clone(), &project) {
                    Ok(_) => {
                        println!("Project '{}' created successfully.", name);
                    },
                    Err(e) => {
                        eprintln!("Failed to create project '{}': {}", name, e);
                    },
                }
                process::exit(0)
            } else {
                eprintln!("Missing fields for project creation.");
                println!("{:?}", self);
                process::exit(1);
            }
        }
        if let Some(project_name) = &self.project {
            if let Some(mut project) = Project::load(self.db_path.clone(), project_name) {
                project.upload_from = self.instrument;
                println!("Loaded project: {}", project);
                if let Some(file_type) = &self.file_type {
                    let files = get_file_paths(self.paths.as_ref().unwrap(), self.use_newest);
                    let (progress_sender, mut progress_receiver) =
                        mpsc::unbounded_channel::<ProcessEvent>();
                    let mut conn = match Connection::open("fluxrs.db") {
                        Ok(conn) => conn,
                        Err(e) => {
                            eprintln!("Failed to open database: {}", e);
                            return;
                        },
                    };

                    let progress_thread = std::thread::spawn(move || {
                        while let Some(event) = progress_receiver.blocking_recv() {
                            handle_progress_messages(event);
                        }
                    });
                    let sender_clone = progress_sender.clone();
                    match file_type {
                        DataType::Gas => {
                            if !files.is_empty() {
                                upload_gas_data_async(files, &mut conn, &project, sender_clone);
                            } else {
                                println!("No files found with {}", self.paths.as_ref().unwrap(),)
                            }
                        },
                        DataType::Cycle => {
                            if !files.is_empty() {
                                upload_cycle_data_async(files, &mut conn, &project, sender_clone);
                            } else {
                                println!("No files found with {}", self.paths.as_ref().unwrap(),)
                            }
                        },
                        DataType::Meteo => {
                            if !files.is_empty() {
                                upload_meteo_data_async(files, &mut conn, &project, sender_clone);
                            } else {
                                println!("No files found with {}", self.paths.as_ref().unwrap(),)
                            }
                        },
                        DataType::Height => {
                            if !files.is_empty() {
                                upload_height_data_async(files, &mut conn, &project, sender_clone);
                            } else {
                                println!("No files found with {}", self.paths.as_ref().unwrap(),)
                            }
                        },
                        DataType::Chamber => {
                            if !files.is_empty() {
                                upload_height_data_async(files, &mut conn, &project, sender_clone);
                            } else {
                                println!("No files found with {}", self.paths.as_ref().unwrap(),)
                            }
                        },
                    }
                    drop(progress_sender);
                    let _ = progress_thread.join();
                }
                if self.initiate_data {
                    let conn = match Connection::open("fluxrs.db") {
                        Ok(conn) => conn,
                        Err(e) => {
                            eprintln!("Failed to open database: {}", e);
                            return;
                        },
                    };

                    let start_date = self
                        .start
                        .unwrap_or_else(|| get_newest_measurement_day(&conn).unwrap_or_default());
                    let end_date = self.end.unwrap_or(Utc::now());
                    if start_date > end_date {
                        eprintln!("Start time can't be after end time.");
                        process::exit(1)
                    }
                    println!("Initiating from {} to {}", start_date, end_date);

                    let arc_conn = Arc::new(Mutex::new(conn));
                    // let sender = self.task_done_sender.clone();
                    let (sender, _task_done_receiver) = std::sync::mpsc::channel();
                    let (progress_sender, mut progress_receiver) = mpsc::unbounded_channel();

                    // let progress_receiver = Some(progress_receiver);
                    // handle_progress_messages(progress_receiver);

                    let proj = project;
                    let runtime =
                        tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();

                    let progress_receiver_thread = std::thread::spawn(move || {
                        while let Some(msg) = progress_receiver.blocking_recv() {
                            handle_progress_messages(msg);
                        }
                    });

                    let handle = runtime.spawn(async move {
                        let cycles_result = query_cycles_async(
                            arc_conn.clone(),
                            start_date,
                            end_date,
                            proj.clone(),
                        )
                        .await;
                        let gas_result =
                            query_gas_async(arc_conn.clone(), start_date, end_date, proj.clone())
                                .await;
                        let meteo_result =
                            query_meteo_async(arc_conn.clone(), start_date, end_date, proj.clone())
                                .await;
                        let height_result = query_height_async(
                            arc_conn.clone(),
                            start_date,
                            end_date,
                            proj.clone(),
                        )
                        .await;

                        let chamber_result =
                            query_chamber_async(arc_conn.clone(), proj.clone()).await;
                        match (
                            cycles_result,
                            gas_result,
                            meteo_result,
                            height_result,
                            chamber_result,
                        ) {
                            (
                                Ok(times),
                                Ok(gas_data),
                                Ok(meteo_data),
                                Ok(height_data),
                                Ok(chamber_data),
                            ) => {
                                let _ = progress_sender
                                    .send(ProcessEvent::Query(QueryEvent::QueryComplete));
                                if !times.start_time.is_empty() && !gas_data.is_empty() {
                                    run_processing_dynamic(
                                        times,
                                        gas_data,
                                        meteo_data,
                                        height_data,
                                        chamber_data,
                                        proj.clone(),
                                        arc_conn.clone(),
                                        progress_sender,
                                    )
                                    .await;
                                    let _ = sender.send(());
                                } else {
                                    // let _ = progress_sender.send(ProcessEvent::Query(
                                    //     QueryEvent::NoGasData("No data available".into()),
                                    // ));
                                    let _ = progress_sender.send(ProcessEvent::Done(Err(
                                        "No data available.".to_owned(),
                                    )));
                                }
                            },
                            e => eprintln!("Failed to query database: {:?}", e),
                        }
                    });
                    runtime.block_on(handle).unwrap();
                    let _ = progress_receiver_thread.join();
                }
            } else {
                println!("No project found with name: {}", project_name);
                process::exit(0)
            }
        } else {
            println!("No project name given")
        }
        process::exit(0)
    }

    pub fn read_file(&self) {}
}

fn get_file_paths(pattern: &str, use_newest: bool) -> Vec<PathBuf> {
    let entries: Vec<PathBuf> =
        glob(pattern).expect("Failed to read glob pattern").filter_map(Result::ok).collect();

    if use_newest {
        entries
            .into_iter()
            .filter_map(|path| {
                let modified = fs::metadata(&path).ok()?.modified().ok()?;
                Some((path, modified))
            })
            .max_by_key(|&(_, time)| time)
            .map(|(path, _)| vec![path])
            .unwrap_or_default()
    } else {
        entries
    }
}

pub fn get_newest_measurement_day(conn: &Connection) -> Option<DateTime<Utc>> {
    let timestamp: Option<i64> =
        conn.query_row("SELECT MAX(start_time) FROM fluxes", [], |row| row.get(0)).ok()?;

    if let Some(time) = timestamp {
        let naive = DateTime::from_timestamp(time, 0).unwrap();
        Some(naive)
    } else {
        None
    }
}

fn parse_datetime(input: &str) -> Result<DateTime<Utc>, Box<dyn Error>> {
    // Try ISO 8601 (e.g. "2024-06-12T14:20:00Z")
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try RFC 2822 (e.g. "Wed, 12 Jun 2024 14:20:00 +0000")
    if let Ok(dt) = DateTime::parse_from_rfc2822(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try common custom format without timezone (e.g. "2024-06-12 14:20:00")
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y-%m-%d",
        "%Y-%m",
        "%d-%m-%Y %H:%M:%S",
        "%m/%d/%Y %H:%M:%S",
    ];

    for fmt in formats {
        if let Ok(naive_date) = NaiveDate::parse_from_str(input, fmt) {
            let naive_dt = naive_date.and_hms_opt(0, 0, 0).unwrap();
            return Ok(Utc.from_utc_datetime(&naive_dt));
        }
    }

    Err("Could not parse datetime".into())
}
pub fn handle_progress_messages(msg: ProcessEvent) {
    match msg {
        ProcessEvent::Query(query_event) => match query_event {
            QueryEvent::InitStarted => {},
            QueryEvent::InitEnded => {},
            QueryEvent::QueryComplete => {
                println!("Finished queries.");
            },
            QueryEvent::NoGasData(start_time) => {
                println!("No gas data found for cycle at {}", start_time);
            },
            QueryEvent::NoGasDataDay(day) => {
                // println!("No gas data found for day {}", day);
            },
        },

        ProcessEvent::Progress(progress_event) => match progress_event {
            ProgressEvent::Rows(current, total) => {
                // self.cycles_state = Some((current, total));
                // self.cycles_progress += current;
            },
            ProgressEvent::Day(date) => {
                println!("Loaded cycles from {}", date);
            },
            ProgressEvent::NoGas(msg) => {
                println!("Gas missing: {}", msg);
            },
        },

        ProcessEvent::Read(read_event) => match read_event {
            ReadEvent::File(filename) => {
                println!("Read file: {}", filename);
            },
            ReadEvent::FileRows(filename, rows) => {
                println!("Read file: {} with {} rows", filename, rows);
            },
            ReadEvent::RowFail(row_msg, msg) => {
                println!("{}", row_msg);
                println!("{}", msg);
            },
            ReadEvent::FileFail(filename, e) => {
                println!("Failed to read file {}, error: {}", filename, e);
            },
        },

        ProcessEvent::Insert(insert_event) => match insert_event {
            InsertEvent::Ok(rows) => {
                println!("Inserted {} rows", rows);
            },
            InsertEvent::OkSkip(rows, duplicates) => {
                println!("Inserted {} rows, skipped {} duplicates.", rows, duplicates);
            },
            InsertEvent::Fail(e) => {
                println!("Failed to insert rows: {}", e);
            },
        },

        // ProcessEvent::Error(e) | ProcessEvent::NoGasError(e) => {
        //     self.log_messages.push_front(format!("Error: {}", e));
        // },
        ProcessEvent::Done(result) => {
            match result {
                Ok(()) => {
                    println!("All processing finished.");
                },
                Err(e) => {
                    println!("Processing finished with error: {}", e);
                },
            }
            // self.cycles_progress = 0;
            // self.init_in_progress = false;
            // self.init_enabled = true;
            // self.query_in_progress = false;
        },
    }
}

fn parse_instrument<I>(args: &mut I) -> Option<InstrumentType>
where
    I: Iterator<Item = String>,
{
    if let Some(instr_str) = args.next() {
        match instr_str.to_lowercase().parse::<InstrumentType>() {
            Ok(instr) => Some(instr),
            Err(_) => {
                eprintln!("Invalid instrument type: '{}'", instr_str);
                process::exit(1);
            },
        }
    } else {
        eprintln!("Expected an instrument type after '-i' or '--instrument'");
        process::exit(1);
    }
}

fn print_help() {
    println!(
        r#"Usage: fluxrs [OPTIONS]

Data Upload and Project Management Tool

General Options:
  -p, --project <NAME>         Project name (required for most actions)
  -db <PATH>                   Path to SQLite database (default: fluxrs.db)
  -s <START_DATETIME>          Start datetime in RFC3339 (e.g. 2024-01-01T00:00:00Z)
  -e <END_DATETIME>            End datetime in RFC3339 (e.g. 2024-01-02T00:00:00Z)
  -i, --instrument <TYPE>      Instrument type (e.g. licor, picarro)
  -n, -newest                  Use newest available date as start
  --upload-from <TYPE>         Source instrument for upload eg. li7810, li7820
  --init                       Initiate processing after upload

File Upload:
  -t <PATH>                    Upload cycle data from path
  -g <PATH>                    Upload gas data from path
  -h <PATH>                    Upload height data from path
  -m <PATH>                    Upload meteo data from path

Project Creation:
  --create-project             Create a new project (must be used with below)
  --serial <SERIAL>            Instrument serial number
  --main-gas <TYPE>            Main gas (e.g. CO2, CH4, etc.)
  --deadband <VALUE>           Deadband threshold in secods (e.g. 0.1)
  --min-calc-len <VALUE>       Minimum calculation duration in seconds (e.g. 5.0)
  --mode <MODE>                Mode (e.g. bestr, deadband)

Misc:
  -h, --help                       Print this help message
"#
    );
}
