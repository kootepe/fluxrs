use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use clap::{Args, Parser, Subcommand, ValueHint};
use glob::glob;
use std::error::Error;
use std::path::PathBuf;

use crate::cmd::config::{Action, Config, ProjectCreate, Run as RunCfg, Upload as UploadCfg};
use crate::datatype::DataType;
use crate::gastype::GasType;
use crate::instruments::instruments::InstrumentType;
use crate::mode::Mode;

// Reuse your flexible parser
fn parse_datetime_str(s: &str) -> Result<DateTime<Utc>, String> {
    parse_datetime(s).map_err(|e| format!("{e}"))
}

#[derive(Debug, Parser)]
#[command(
    name = "fluxrs",
    about = "Data Upload and Project Management Tool",
    version,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Path to SQLite database
    #[arg(long = "db", value_name = "PATH", default_value = "fluxrs.db", global = true)]
    pub db_path: String,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Project-related operations
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },

    /// Upload data files (no processing here)
    Upload {
        #[command(subcommand)]
        kind: UploadKind,
    },

    /// Run processing / queries (no file uploads here)
    Run(RunArgs),
}

/* --------------------- project create --------------------- */

#[derive(Debug, Subcommand)]
pub enum ProjectCmd {
    /// Create a new project in the database
    Create(ProjectCreateArgs),
}

#[derive(Debug, Args)]
pub struct ProjectCreateArgs {
    /// Project name
    #[arg(long)]
    pub name: String,

    /// Instrument type
    #[arg(long)]
    pub instrument: InstrumentType,

    /// Instrument serial number
    #[arg(long = "serial")]
    pub instrument_serial: String,

    /// Main gas (e.g., CO2, CH4)
    #[arg(long = "main-gas")]
    pub main_gas: GasType,

    /// Deadband threshold in seconds (positive integer)
    #[arg(long)]
    pub deadband: u64,

    /// Minimum calculation duration in seconds (positive integer)
    #[arg(long = "min-calc-len")]
    pub min_calc_len: u64,

    /// Mode (e.g., bestr, deadband)
    #[arg(long)]
    pub mode: Mode,

    /// Timezone, e.g., Europe/Helsinki
    #[arg(long = "tz")]
    pub tz: Tz,
}

/* ----------------------- upload ----------------------- */

#[derive(Debug, Subcommand)]
pub enum UploadKind {
    /// Upload cycle data files
    Cycle(UploadArgs),

    /// Upload gas data files
    Gas(UploadArgs),

    /// Upload height data files
    Height(UploadArgs),

    /// Upload meteo data files
    Meteo(UploadArgs),
}

#[derive(Debug, Args)]
pub struct UploadArgs {
    /// Project name
    #[arg(short = 'p', long = "project")]
    pub project: String,

    /// Glob path to files to upload (quote the pattern)
    #[arg(num_args = 1..,
        value_hint = ValueHint::AnyPath,
        trailing_var_arg = true,
        required = true,
        short = 'i', long = "inputs",
        value_name = "Input files")]
    pub inputs: Vec<String>,

    /// Use only files modified after last ingested timestamp
    #[arg(short = 'n', long = "newest")]
    pub use_newest: bool,

    /// Timezone of the timestamps (if needed by your pipeline)
    #[arg(short = 'z', long = "tz")]
    pub tz: Option<Tz>,
}
impl UploadArgs {
    /// Expand `inputs` into actual files.
    pub fn resolve_files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();

        for inp in &self.inputs {
            // treat as glob if it has wildcards
            if inp.contains('*') || inp.contains('?') || inp.contains('[') {
                match glob(inp) {
                    Ok(paths) => {
                        for path in paths.filter_map(Result::ok) {
                            out.push(path);
                        }
                    },
                    Err(e) => {
                        eprintln!("Invalid glob '{}': {}", inp, e);
                    },
                }
            } else {
                // treat as literal path
                out.push(PathBuf::from(inp));
            }
        }

        out
    }
}
/* ------------------------- run ------------------------- */

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Project name
    #[arg(short = 'p', long = "project")]
    pub project: String,

    /// Source instrument (optional)
    #[arg(short = 'i', long = "instrument")]
    pub instrument: Option<InstrumentType>,

    /// Start datetime
    #[arg(short = 's', value_parser = parse_datetime_str, value_name = "START")]
    pub start: Option<DateTime<Utc>>,

    /// End datetime
    #[arg(short = 'e', value_parser = parse_datetime_str, value_name = "END")]
    pub end: Option<DateTime<Utc>>,

    /// Use newest available date as start (fallback if --start missing)
    #[arg(short = 'n', long = "newest")]
    pub use_newest: bool,

    /// Timezone (if needed by your processing)
    #[arg(short = 'z', long = "tz")]
    pub tz: Option<Tz>,

    /// Initiate processing immediately (kept for parity)
    #[arg(long = "init")]
    pub initiate_data: bool,
}

// -------- Map CLI -> new Config/Action types --------

impl Cli {
    pub fn into_config(self) -> Config {
        let db_path = PathBuf::from(self.db_path);

        match self.command {
            Commands::Project { cmd } => match cmd {
                ProjectCmd::Create(args) => Config {
                    db_path,
                    progress_receiver: None,
                    action: Action::ProjectCreate(ProjectCreate {
                        name: args.name,
                        instrument: args.instrument,
                        instrument_serial: args.instrument_serial,
                        main_gas: args.main_gas,
                        deadband: args.deadband as f64,
                        min_calc_len: args.min_calc_len as f64,
                        mode: args.mode,
                        tz: args.tz,
                    }),
                },
            },

            Commands::Upload { kind } => {
                let (project, file_type, inputs, use_newest, tz) = match kind {
                    UploadKind::Gas(u) => (u.project, DataType::Gas, u.inputs, u.use_newest, u.tz),
                    UploadKind::Height(u) => {
                        (u.project, DataType::Height, u.inputs, u.use_newest, u.tz)
                    },
                    UploadKind::Meteo(u) => {
                        (u.project, DataType::Meteo, u.inputs, u.use_newest, u.tz)
                    },
                    UploadKind::Cycle(u) => {
                        (u.project, DataType::Cycle, u.inputs, u.use_newest, u.tz)
                    },
                };

                Config {
                    db_path,
                    progress_receiver: None,
                    action: Action::Upload(UploadCfg {
                        project,
                        file_type,
                        inputs,
                        use_newest,
                        tz,
                    }),
                }
            },

            Commands::Run(run) => Config {
                db_path,
                progress_receiver: None,
                action: Action::Run(RunCfg {
                    project: run.project,
                    instrument: run.instrument,
                    start: run.start,
                    end: run.end,
                    use_newest: run.use_newest,
                    tz: run.tz,
                    initiate_data: run.initiate_data,
                }),
            },
        }
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
