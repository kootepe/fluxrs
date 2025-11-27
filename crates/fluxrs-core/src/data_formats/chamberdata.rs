use crate::project::Project;

use crate::datatype::DataType;
use crate::processevent::{InsertEvent, ProcessEvent, ReadEvent};
use crate::utils::ensure_utf8;
use crate::utils::get_or_insert_data_file;

use chrono_tz::Tz;
use rusqlite;
use rusqlite::Row;
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
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

#[derive(Debug, Clone, Copy)]
pub enum ChamberShape {
    Cylinder { diameter_m: f64, height_m: f64, snow_height_m: f64 },
    Box { width_m: f64, length_m: f64, height_m: f64, snow_height_m: f64 },
}

impl Default for ChamberShape {
    fn default() -> Self {
        ChamberShape::Box { width_m: 1.0, length_m: 1.0, height_m: 1.0, snow_height_m: 0.0 }
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
            ChamberShape::Cylinder { diameter_m, height_m, snow_height_m } => write!(
                f,
                "Cylinder: L={:.2}m, D={:.2}m, snow h = {:.2}",
                height_m, diameter_m, snow_height_m
            ),
            ChamberShape::Box { width_m, length_m, height_m, snow_height_m } => write!(
                f,
                "Box: L={:.2}m, W={:.2}m, H={:.2}m, S={:.2}m",
                height_m, length_m, width_m, snow_height_m
            ),
        }
    }
}

/// Where did this chamber definition come from?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChamberOrigin {
    /// Synthetic default (e.g. not present in DB/CSV)
    Default,
    /// Loaded from the database
    Raw,
}

/// A chamber plus metadata about its origin.
///
/// Use `chamber.shape` for geometry, and `chamber.origin` to know
/// if it’s default vs DB vs CSV.
#[derive(Debug, Clone, Copy)]
pub struct Chamber {
    pub shape: ChamberShape,
    pub origin: ChamberOrigin,
}

impl Default for Chamber {
    fn default() -> Self {
        Self { shape: ChamberShape::default(), origin: ChamberOrigin::Default }
    }
}

impl Chamber {
    pub fn is_default(&self) -> bool {
        self.origin == ChamberOrigin::Default
    }

    pub fn is_raw(&self) -> bool {
        self.origin == ChamberOrigin::Raw
    }

    pub fn volume_m3(&self) -> f64 {
        self.shape.volume_m3()
    }

    pub fn area_m2(&self) -> f64 {
        self.shape.area_m2()
    }

    pub fn internal_height(&self) -> f64 {
        self.shape.internal_height()
    }

    pub fn adjusted_volume(&self) -> f64 {
        self.shape.adjusted_volume()
    }

    pub fn kind(&self) -> ChamberShapeType {
        self.shape.kind()
    }

    pub fn set_height(&mut self, new_height: f64) {
        self.shape.set_height(new_height);
    }

    pub fn set_snow_height(&mut self, new_snow_height: f64) {
        self.shape.set_snow_height(new_snow_height);
    }
}

impl fmt::Display for Chamber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let origin_str = match self.origin {
            ChamberOrigin::Default => "default",
            ChamberOrigin::Raw => "raw",
        };
        write!(f, "{} (origin: {})", self.shape, origin_str)
    }
}

/// Query chambers from the DB for a project.
/// All returned chambers have origin = `ChamberOrigin::Db`.
pub fn query_chambers(conn: &Connection, project: i64) -> Result<HashMap<String, Chamber>> {
    println!("Querying chamber data");
    let mut chamber_map: HashMap<String, Chamber> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT chamber_id, shape, diameter, width, length, height
         FROM chamber_metadata
         WHERE project_link = ?1",
    )?;

    let rows = stmt.query_map(params![project], |row| {
        let chamber_id: String = row.get("chamber_id")?;
        let shape_str: String = row.get("shape")?;
        let diameter_m: f64 = row.get("diameter")?;
        let width_m: f64 = row.get("width")?;
        let length_m: f64 = row.get("length")?;
        let height_m: f64 = row.get("height")?;
        let snow_height_m = 0.0;

        let shape_type = shape_str.parse::<ChamberShapeType>().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::fmt::Error),
            )
        })?;

        let shape = match shape_type {
            ChamberShapeType::Box => {
                ChamberShape::Box { width_m, length_m, height_m, snow_height_m }
            },
            ChamberShapeType::Cylinder => {
                ChamberShape::Cylinder { diameter_m, height_m, snow_height_m }
            },
        };

        Ok((chamber_id, Chamber { shape, origin: ChamberOrigin::Raw }))
    })?;

    for row in rows {
        let (chamber_id, chamber) = row?;
        chamber_map.insert(chamber_id, chamber);
    }

    Ok(chamber_map)
}

pub async fn query_chamber_async(
    conn: Arc<Mutex<Connection>>, // Arc<Mutex> for shared async access
    project: Project,
) -> Result<HashMap<String, Chamber>> {
    let result = task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        query_chambers(&conn, project.id.unwrap())
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => {
            // Convert JoinError into rusqlite::Error
            Err(rusqlite::Error::ExecuteReturnedResults)
        },
    }
}

/// Insert chamber metadata into DB from a map.
/// Geometry comes from `chamber.shape`; origin is *not* stored in DB.
pub fn insert_chamber_metadata(
    tx: &Connection,
    chambers: &HashMap<String, Chamber>,
    project_id: &i64,
    file_id: &i64,
) -> Result<()> {
    for (chamber_id, chamber) in chambers {
        let shape = &chamber.shape;

        let (shape_str, diameter, width, length, height) = match shape {
            ChamberShape::Cylinder { diameter_m, height_m, .. } => {
                ("cylinder", *diameter_m, 0.0, 0.0, *height_m)
            },
            ChamberShape::Box { width_m, length_m, height_m, .. } => {
                ("box", 0.0, *width_m, *length_m, *height_m)
            },
        };

        tx.execute(
            "INSERT OR IGNORE INTO chamber_metadata (
                chamber_id, shape, diameter, width, length, height, project_link, file_link
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![chamber_id, shape_str, diameter, width, length, height, project_id, file_id],
        )?;
    }

    Ok(())
}

/// Convert a DB row to a Chamber (origin = Db).
impl TryFrom<&Row<'_>> for Chamber {
    type Error = rusqlite::Error;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let shape_type_str: String = row.get("shape_type")?;
        let shape = match shape_type_str.to_lowercase().as_str() {
            "box" => ChamberShape::Box {
                width_m: row.get("width")?,
                length_m: row.get("length")?,
                height_m: row.get("height")?,
                snow_height_m: 0.0,
            },
            "cylinder" => ChamberShape::Cylinder {
                diameter_m: row.get("diameter")?,
                height_m: row.get("height")?,
                snow_height_m: 0.0,
            },
            _ => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(rusqlite::types::FromSqlError::Other(
                        "Unknown chamber shape type".into(),
                    )),
                ))
            },
        };

        Ok(Chamber { shape, origin: ChamberOrigin::Raw })
    }
}

/// Read chamber metadata from CSV.
/// All chambers loaded here have origin = `ChamberOrigin::Csv`.
pub fn read_chamber_metadata<P: AsRef<Path>>(
    path: P,
) -> Result<HashMap<String, Chamber>, Box<dyn Error>> {
    let content = ensure_utf8(&path)?;
    let mut rdr =
        csv::ReaderBuilder::new().has_headers(true).flexible(true).from_reader(content.as_bytes());

    let mut chambers = HashMap::new();

    for (i, result) in rdr.records().enumerate() {
        let record = result?;

        let chamber_id = record.get(0).ok_or("Missing chamber_id")?.to_string();
        let shape = record.get(1).ok_or("Missing shape")?.to_lowercase();
        let diameter = parse_f64_field(&record, 2)?;
        let height = parse_f64_field(&record, 3)?;
        let width = parse_f64_field(&record, 4)?;
        let length = parse_f64_field(&record, 5)?;

        let shape_val = match shape.as_str() {
            "cylinder" => ChamberShape::Cylinder {
                diameter_m: diameter,
                height_m: height,
                snow_height_m: 0.0,
            },
            "box" => ChamberShape::Box {
                width_m: width,
                length_m: length,
                height_m: height,
                snow_height_m: 0.0,
            },
            _ => {
                eprintln!("Unknown shape '{}', skipping chamber {}", shape, chamber_id);
                continue;
            },
        };

        let chamber = Chamber { shape: shape_val, origin: ChamberOrigin::Raw };

        println!("{}", chamber);
        chambers.insert(chamber_id, chamber);
    }

    Ok(chambers)
}

fn parse_f64_field(record: &csv::StringRecord, idx: usize) -> Result<f64, Box<dyn Error>> {
    Ok(record.get(idx).filter(|s| !s.trim().is_empty()).unwrap_or("0").parse()?)
}

/// Upload chamber metadata from selected CSV paths, insert into DB,
/// and send progress events. Chambers from CSV are marked as `Csv` origin.
pub fn upload_chamber_metadata_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    _tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    for path in &selected_paths {
        let project_id = project.id.unwrap();

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::chamber_fail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                continue;
            },
        };

        let tx = match conn.transaction() {
            Ok(tx) => tx,
            Err(e) => {
                eprintln!("Failed to start transaction: {}", e);
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "Could not start transaction for '{}': {}",
                    file_name, e
                ))));
                continue;
            },
        };

        let file_id = match get_or_insert_data_file(&tx, DataType::Chamber, file_name, project_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue;
            },
        };

        match read_chamber_metadata(path) {
            Ok(chambers) => {
                match insert_chamber_metadata(&tx, &chambers, &project.id.unwrap(), &file_id) {
                    Ok(_) => {
                        if let Err(e) = tx.commit() {
                            eprintln!("Failed to commit transaction for '{}': {}", file_name, e);
                            let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(
                                format!("Commit failed for file '{}': {}", file_name, e),
                            )));
                            continue;
                        }
                    },
                    Err(e) => {
                        let msg = format!("Failed to insert chamber data. Error: {}", e);
                        let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(msg)));
                    },
                }
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::chamber_fail(
                    path.to_string_lossy().to_string(),
                    e.to_string(),
                )));
            },
        }
    }

    let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
}
