use crate::gastype::GasType;
use crate::instruments::instruments::{Instrument, InstrumentType};
use crate::mode::Mode;
use crate::types::FastMap;
use chrono_tz::Tz;
use std::error::Error;
use std::fmt;
use std::process;
use std::str::FromStr;

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
            "{}, {}, {}, {}, {}, {}, {}, {} {}",
            self.id.map_or("None".to_string(), |id| id.to_string()),
            self.name,
            self.instrument.model,
            self.instrument.serial,
            self.instrument.id.map_or("None".to_string(), |id| id.to_string()),
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
    pub fn load_instruments(&self) -> rusqlite::Result<FastMap<i64, Instrument>> {
        let conn = Connection::open("fluxrs.db")?;
        let mut stmt = conn.prepare(
            "SELECT id, instrument_model, instrument_serial
         FROM instruments
         WHERE project_link = ?1",
        )?;

        let rows = stmt.query_map(params![self.id.unwrap()], |row| {
            let id: i64 = row.get(0)?;
            let model_str: String = row.get(1)?;

            Ok((
                id,
                Instrument {
                    id: Some(id),
                    model: InstrumentType::from_str(&model_str)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    serial: row.get(2)?,
                },
            ))
        })?;

        let mut instruments = FastMap::default();
        for inst in rows {
            let (id, instrument) = inst?;
            instruments.insert(id, instrument);
        }

        Ok(instruments)
    }
    pub fn load(db_path: Option<String>, name: &String) -> Option<Project> {
        let mut conn = Connection::open("fluxrs.db").ok()?;
        if db_path.is_some() {
            conn = Connection::open(db_path.unwrap()).ok()?;
        }

        let result: Result<(i64, String, String, String, i64, usize, f64, f64, u8, String), _> =
            conn.query_row(
                "SELECT
                    p.id,
                    p.project_name,
                    i.instrument_serial,
                    i.instrument_model,
                    i.id,
                    p.main_gas,
                    p.deadband,
                    p.min_calc_len,
                    p.mode,
                    p.tz
                FROM projects p
                LEFT JOIN instruments i on i.id = p.main_instrument_link
                WHERE p.project_name = ?",
                [name],
                |row| {
                    Ok((
                        row.get(0)?, // id
                        row.get(1)?, // project_name
                        row.get(2)?, // instrument_serial
                        row.get(3)?, // instrument_model
                        row.get(4)?, // instrument id
                        row.get(5)?, // main_gas
                        row.get(6)?, // deadband
                        row.get(7)?, // min_calc_len
                        row.get(8)?, // mode
                        row.get(9)?, // tz
                    ))
                },
            );

        let (
            id,
            project_name,
            instrument_serial,
            instrument_string,
            instrument_id,
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

        let instrumenttype = match instrument_string.parse::<InstrumentType>() {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Unexpected invalid instrument type from DB: '{}'", instrument_string);
                process::exit(1);
            },
        };
        let instrument = Instrument {
            model: instrumenttype,
            serial: instrument_serial,
            id: Some(instrument_id),
        };

        Some(Project {
            id: Some(id),
            name: project_name,
            instrument,
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
        let mut conn = Connection::open(db_path)?;
        let tx = conn.transaction()?;
        // Check if project name already exists

        // Check if project name already exists
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE project_name = ?1)",
            rusqlite::params![project.name],
            |row| row.get(0),
        )?;
        if exists {
            return Err("Project already exists.".into());
        }
        tx.execute("UPDATE projects SET current = 0 WHERE current = 1", [])?;

        tx.execute(
            "INSERT OR IGNORE INTO projects (
                project_name, main_gas, deadband, min_calc_len, mode, tz, current
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                project.name,
                project.main_gas.unwrap().as_int(),
                project.deadband,
                project.min_calc_len,
                project.mode.as_int(),
                project.tz.to_string(),
                1,
            ],
        )?;

        let project_rowid = tx.last_insert_rowid(); // i64

        tx.execute(
            "INSERT INTO instruments (instrument_model, instrument_serial, project_link)
         VALUES (?1, ?2, ?3)",
            params![project.instrument.model.to_string(), project.instrument.serial, project_rowid],
        )?;
        // DB id of the new instrument
        let instrument_id = tx.last_insert_rowid();
        // If your Project has a field for this, set it:
        // project.instrument_link = Some(instrument_id);

        // Now update the project to point at this instrument
        tx.execute(
            "UPDATE projects
         SET main_instrument_link = ?1
         WHERE id = ?2",
            params![instrument_id, project_rowid],
        )?;

        tx.commit()?;

        Ok(())
    }
}
