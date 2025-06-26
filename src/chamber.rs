use crate::project_app::Project;
use chrono::prelude::DateTime;
use chrono::Utc;
use rusqlite;
use rusqlite::Row;
use rusqlite::{params, params_from_iter, Connection, Result};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::task;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChamberShapeType {
    Box,
    Cylinder,
}

impl ChamberShapeType {
    pub fn as_int(&self) -> usize {
        match self {
            ChamberShapeType::Box => 0,
            ChamberShapeType::Cylinder => 1,
        }
    }

    pub fn from_int(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Box),
            1 => Some(Self::Cylinder),
            _ => None,
        }
    }
}

impl FromStr for ChamberShapeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "box" => Ok(ChamberShapeType::Box),
            "cylinder" => Ok(ChamberShapeType::Cylinder),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Clone)]
pub enum ChamberShape {
    Cylinder { diameter_m: f64, height_m: f64 },
    Box { width_m: f64, length_m: f64, height_m: f64 },
}

impl Default for ChamberShape {
    fn default() -> Self {
        ChamberShape::Box { width_m: 1., length_m: 1., height_m: 1. }
    }
}

impl ChamberShape {
    pub fn volume_m3(&self) -> f64 {
        match self {
            ChamberShape::Cylinder { diameter_m, height_m } => {
                let r = diameter_m / 2.0;
                std::f64::consts::PI * r * r * height_m
            },
            ChamberShape::Box { width_m, length_m, height_m } => width_m * length_m * height_m,
        }
    }

    pub fn area_m2(&self) -> f64 {
        match self {
            ChamberShape::Cylinder { diameter_m, .. } => {
                let r = diameter_m / 2.0;
                std::f64::consts::PI * r * r
            },
            ChamberShape::Box { width_m, length_m, .. } => width_m * length_m,
        }
    }

    pub fn internal_height(&self) -> f64 {
        match self {
            ChamberShape::Cylinder { height_m, .. } => *height_m,
            ChamberShape::Box { height_m, .. } => *height_m,
        }
    }

    pub fn adjusted_volume(&self, snow_height_m: f64) -> f64 {
        let adjusted_height = (self.internal_height() - snow_height_m).max(0.0);
        match self {
            ChamberShape::Cylinder { diameter_m, .. } => {
                let r = diameter_m / 2.0;
                std::f64::consts::PI * r * r * adjusted_height
            },
            ChamberShape::Box { width_m, length_m, .. } => width_m * length_m * adjusted_height,
        }
    }

    pub fn kind(&self) -> ChamberShapeType {
        match self {
            ChamberShape::Box { .. } => ChamberShapeType::Box,
            ChamberShape::Cylinder { .. } => ChamberShapeType::Cylinder,
        }
    }
    pub fn set_height(&mut self, new_height: f64) {
        match self {
            ChamberShape::Cylinder { height_m, .. } => *height_m = new_height,
            ChamberShape::Box { height_m, .. } => *height_m = new_height,
        }
    }
}

impl fmt::Display for ChamberShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChamberShape::Cylinder { diameter_m, height_m } => {
                write!(f, "Cylinder: diameter = {:.2} m, height = {:.2} m", diameter_m, height_m)
            },
            ChamberShape::Box { width_m, length_m, height_m } => {
                write!(
                    f,
                    "Box: width = {:.2} m, length = {:.2} m, height = {:.2} m",
                    width_m, length_m, height_m
                )
            },
        }
    }
}
pub fn query_chambers(conn: &Connection, project: String) -> Result<HashMap<String, ChamberShape>> {
    println!("Querying chamber data");
    let mut chamber_map: HashMap<String, ChamberShape> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT chamber_id, shape, diameter, width, length, height
         FROM chamber_metadata
         WHERE project_id = ?1",
    )?;

    let rows = stmt.query_map(params![project], |row| {
        let chamber_id: String = row.get("chamber_id")?;
        let shape_str: String = row.get("shape")?;
        let diameter_m: f64 = row.get("diameter")?;
        let width_m: f64 = row.get("width")?;
        let length_m: f64 = row.get("length")?;
        let height_m: f64 = row.get("height")?;

        // Parse and build ChamberShape
        let shape_type = shape_str.parse::<ChamberShapeType>().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::fmt::Error),
            )
        })?;

        let chamber_shape = match shape_type {
            ChamberShapeType::Box => ChamberShape::Box { width_m, length_m, height_m },
            ChamberShapeType::Cylinder => ChamberShape::Cylinder { diameter_m, height_m },
        };

        Ok((chamber_id, chamber_shape))
    })?;

    for row in rows {
        let (chamber_id, shape) = row?;
        chamber_map.insert(chamber_id, shape);
    }

    Ok(chamber_map)
}
pub async fn query_chamber_async(
    conn: Arc<Mutex<Connection>>, // Arc<Mutex> for shared async access
    project: Project,
) -> Result<HashMap<String, ChamberShape>> {
    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_chambers(&conn, project.name)
    })
    .await;
    match result {
        Ok(inner) => inner,
        Err(_) => {
            // Convert JoinError into rusqlite::Error::ExecuteReturnedResults or custom error
            Err(rusqlite::Error::ExecuteReturnedResults) // or log `e` if needed
        },
    }
}

pub fn insert_chamber_metadata(
    conn: &mut Connection,
    chambers: &HashMap<String, ChamberShape>,
    project_id: &str,
) -> Result<()> {
    let tx = conn.transaction()?; // Use transaction for performance and atomicity

    for (chamber_id, shape) in chambers {
        let (shape_str, diameter, width, length, height) = match shape {
            ChamberShape::Cylinder { diameter_m, height_m } => {
                ("cylinder", Some(*diameter_m), Some(0.), Some(0.), *height_m)
            },
            ChamberShape::Box { width_m, length_m, height_m } => {
                ("box", None, Some(*width_m), Some(*length_m), *height_m)
            },
        };

        tx.execute(
            "INSERT OR IGNORE INTO chamber_metadata (
                chamber_id, shape, diameter, width, length, height, project_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![chamber_id, shape_str, diameter, width, length, height, project_id],
        )?;
    }

    tx.commit()?;
    Ok(())
}
impl TryFrom<&Row<'_>> for ChamberShape {
    type Error = rusqlite::Error;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let shape_type_str: String = row.get("shape_type")?;
        match shape_type_str.to_lowercase().as_str() {
            "box" => Ok(ChamberShape::Box {
                width_m: row.get("width")?,
                length_m: row.get("length")?,
                height_m: row.get("height")?,
            }),
            "cylinder" => Ok(ChamberShape::Cylinder {
                diameter_m: row.get("diameter")?,
                height_m: row.get("height")?,
            }),
            _ => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(rusqlite::types::FromSqlError::Other("Unknown chamber shape type".into())),
            )),
        }
    }
}

pub fn read_chamber_metadata<P: AsRef<Path>>(
    path: P,
) -> Result<HashMap<String, ChamberShape>, Box<dyn Error>> {
    let file = File::open(path)?;
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(file);

    let mut chambers = HashMap::new();

    for result in rdr.records() {
        let record = result?;

        let chamber_id = record.get(0).ok_or("Missing chamber_id")?.to_string();
        let shape = record.get(1).ok_or("Missing shape")?.to_lowercase();

        let diameter: f64 = record.get(2).unwrap_or("0").parse()?;
        let width: f64 = record.get(3).unwrap_or("0").parse()?;
        let length: f64 = record.get(4).unwrap_or("0").parse()?;
        let height: f64 = record.get(5).unwrap_or("0").parse()?;

        let chamber = match shape.as_str() {
            "cylinder" => ChamberShape::Cylinder { diameter_m: diameter, height_m: height },
            "box" => ChamberShape::Box { width_m: width, length_m: length, height_m: height },
            _ => {
                eprintln!("Unknown shape '{}', skipping chamber {}", shape, chamber_id);
                continue;
            },
        };

        chambers.insert(chamber_id, chamber);
    }

    Ok(chambers)
}
