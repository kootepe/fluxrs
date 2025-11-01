use crate::ui::project_ui::Project;
use crate::utils::ensure_utf8;
use rusqlite;
use rusqlite::Row;
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
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
    Cylinder { diameter_m: f64, height_m: f64, snow_height_m: f64 },
    Box { width_m: f64, length_m: f64, height_m: f64, snow_height_m: f64 },
}

impl Default for ChamberShape {
    fn default() -> Self {
        ChamberShape::Box { width_m: 1., length_m: 1., height_m: 1., snow_height_m: 0. }
    }
}

impl ChamberShape {
    pub fn volume_m3(&self) -> f64 {
        match self {
            ChamberShape::Cylinder { diameter_m, height_m, snow_height_m, .. } => {
                let r = diameter_m / 2.0;
                std::f64::consts::PI * r * r * (height_m - snow_height_m)
            },
            ChamberShape::Box { width_m, length_m, height_m, snow_height_m, .. } => {
                width_m * length_m * (height_m - snow_height_m)
            },
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
            ChamberShape::Cylinder { height_m, snow_height_m, .. } => {
                (*height_m - *snow_height_m).max(0.0)
            },
            ChamberShape::Box { height_m, snow_height_m, .. } => {
                (*height_m - *snow_height_m).max(0.0)
            },
        }
    }

    pub fn adjusted_volume(&self) -> f64 {
        let adjusted_height = self.internal_height();
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
    pub fn set_snow_height(&mut self, new_snow_height: f64) {
        match self {
            ChamberShape::Cylinder { snow_height_m, .. } => *snow_height_m = new_snow_height,
            ChamberShape::Box { snow_height_m, .. } => *snow_height_m = new_snow_height,
        }
    }
}

impl fmt::Display for ChamberShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChamberShape::Cylinder { diameter_m, height_m, snow_height_m } => {
                write!(
                    f,
                    "Cylinder: height = {:.2} m, diameter = {:.2} m, snow height = {:.2}",
                    height_m, diameter_m, snow_height_m
                )
            },
            ChamberShape::Box { width_m, length_m, height_m, snow_height_m } => {
                write!(
                    f,
                    "Box: height = {:.2} m, length = {:.2} m, width = {:.2} m, snow height = {:.2}",
                    height_m, length_m, width_m, snow_height_m
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
        let snow_height_m = 0.;

        // Parse and build ChamberShape
        let shape_type = shape_str.parse::<ChamberShapeType>().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::fmt::Error),
            )
        })?;

        let chamber_shape = match shape_type {
            ChamberShapeType::Box => {
                ChamberShape::Box { width_m, length_m, height_m, snow_height_m }
            },
            ChamberShapeType::Cylinder => {
                ChamberShape::Cylinder { diameter_m, height_m, snow_height_m }
            },
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
            ChamberShape::Cylinder { diameter_m, height_m, snow_height_m } => {
                ("cylinder", *diameter_m, 0., 0., *height_m)
            },
            ChamberShape::Box { width_m, length_m, height_m, snow_height_m } => {
                ("box", 0., *width_m, *length_m, *height_m)
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
                snow_height_m: 0.,
            }),
            "cylinder" => Ok(ChamberShape::Cylinder {
                diameter_m: row.get("diameter")?,
                height_m: row.get("height")?,
                snow_height_m: 0.,
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
    let content = ensure_utf8(&path)?;
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(content.as_bytes());

    let mut chambers = HashMap::new();

    for (i, result) in rdr.records().enumerate() {
        let record = result?;

        let chamber_id = record.get(0).ok_or("Missing chamber_id")?.to_string();
        let shape = record.get(1).ok_or("Missing shape")?.to_lowercase();
        let diameter = parse_f64_field(&record, 2)?;
        let height = parse_f64_field(&record, 3)?;
        let width = parse_f64_field(&record, 4)?;
        let length = parse_f64_field(&record, 5)?;

        let chamber = match shape.as_str() {
            "cylinder" => {
                ChamberShape::Cylinder { diameter_m: diameter, height_m: height, snow_height_m: 0. }
            },
            "box" => ChamberShape::Box {
                width_m: width,
                length_m: length,
                height_m: height,
                snow_height_m: 0.,
            },
            _ => {
                eprintln!("Unknown shape '{}', skipping chamber {}", shape, chamber_id);
                continue;
            },
        };

        println!("{}", chamber);
        chambers.insert(chamber_id, chamber);
    }

    Ok(chambers)
}

fn parse_f64_field(record: &csv::StringRecord, idx: usize) -> Result<f64, Box<dyn Error>> {
    Ok(record.get(idx).filter(|s| !s.trim().is_empty()).unwrap_or("0").parse()?)
}
