use crate::gastype::GasType;
use crate::instruments::instruments::{Instrument, InstrumentType};
use crate::ui::validation_ui::Mode;
use chrono_tz::Tz;
use std::error::Error;
use std::fmt;
use std::process;

use rusqlite::{params, Connection, Result};

#[derive(Debug)]
pub struct ProjectExistsError {
    pub project_name: String,
}

impl fmt::Display for ProjectExistsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Project with id '{}' already exists", self.project_name)
    }
}

impl Error for ProjectExistsError {}

#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub id: Option<i64>,
    pub instrument: Instrument,
    pub main_gas: Option<GasType>,
    pub deadband: f64,
    pub min_calc_len: f64,
    pub mode: Mode,
    pub tz: Tz,
    pub upload_from: Option<InstrumentType>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            id: None,
            instrument: Instrument::default(),
            main_gas: Some(GasType::default()),
            deadband: 0.0,
            min_calc_len: 0.0,
            mode: Mode::default(),
            tz: Tz::UTC,
            upload_from: None,
        }
    }
}

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}, {}, {}, {}, {}, {}, {} {}",
            self.id.unwrap_or(96969696),
            self.name,
            self.instrument.model,
            self.instrument.serial,
            self.main_gas
                .as_ref()
                .map(|g| format!("{:?}", g))
                .unwrap_or_else(|| "None".to_string()),
            self.deadband,
            self.min_calc_len,
            self.tz
        )
    }
}

impl Project {
    pub fn load(db_path: Option<String>, name: &String) -> Option<Project> {
        let mut conn = Connection::open("fluxrs.db").ok()?;
        if db_path.is_some() {
            conn = Connection::open(db_path.unwrap()).ok()?;
        }

        let result: Result<(String, String, String, usize, f64, f64, u8, String), _> = conn.query_row(
            "SELECT project_id, instrument_serial, instrument_model, main_gas, deadband, min_calc_len, mode, tz FROM projects WHERE project_id = ?",
            [name],
            |row| Ok((
                row.get(0)?, // project_id
                row.get(1)?, // instrument_serial
                row.get(2)?, // instrument_model
                row.get(3)?, // main_gas
                row.get(4)?, // deadband
                row.get(5)?, // min_calc_len
                row.get(6)?, // mode
                row.get(7)?, // tz
            )),
        );

        let (
            project_id,
            instrument_serial,
            instrument_string,
            gas_i,
            deadband,
            min_calc_len,
            mode_i,
            tz_str,
        ) = result.ok()?;

        let main_gas = GasType::from_int(gas_i);
        let mode = Mode::from_int(mode_i)?;

        let tz = tz_str.parse().expect("Invalid timezone string");
        // let instrument = InstrumentType::from_str(&instrument_string);
        // let instrument =
        //     instrument_string.parse::<InstrumentType>().expect("Invalid instrument type");

        let instrument = match instrument_string.parse::<InstrumentType>() {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Unexpected invalid instrument type from DB: '{}'", instrument_string);
                process::exit(1);
            },
        };

        Some(Project {
            name: project_id,
            instrument,
            instrument_serial,
            main_gas,
            deadband,
            min_calc_len,
            mode,
            tz,
            upload_from: None,
        })
    }
    pub fn save(
        db_path: Option<String>,
        project: &Project,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db_path = db_path.unwrap_or_else(|| "fluxrs.db".to_string());
        let conn = Connection::open(db_path)?;
        // Check if project name already exists
        let mut stmt = conn.prepare("SELECT 1 FROM projects WHERE project_id = ?1")?;
        let mut rows = stmt.query(params![project.name])?;

        if rows.next()?.is_some() {
            return Err("Project already exists.".into());
        }
        conn.execute(
            "INSERT OR IGNORE INTO projects (
                project_id, instrument_model, instrument_serial, main_gas, deadband, min_calc_len, mode, tz, current
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                project.name,
                project.instrument.to_string(),
                project.instrument_serial,
                project.main_gas.unwrap().as_int(),
                project.deadband,
                project.min_calc_len,
                project.mode.as_int(),
                project.tz.to_string(),
                0,
            ],
        )?;

        Ok(())
    }
}
