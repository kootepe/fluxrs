use crate::constants::MIN_CALC_AREA_RANGE;
use crate::gastype::GasType;
use crate::ui::main_app::AppEvent;
use crate::ui::validation_ui::Mode;
use crate::InstrumentType;
use std::fmt;
use std::process;

use rusqlite;
// use rusqlite::Connection;
use rusqlite::{params, Connection, Result};

use std::collections::HashMap;

#[derive(Clone)]
pub struct Project {
    pub name: String,
    pub instrument: InstrumentType,
    pub instrument_serial: String,
    pub main_gas: Option<GasType>,
    pub deadband: f64,
    pub min_calc_len: f64,
    pub mode: Mode,
    pub upload_from: Option<InstrumentType>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            instrument: InstrumentType::default(),
            instrument_serial: "UNKNOWN_SERIAL".to_string(),
            main_gas: Some(GasType::default()),
            deadband: 0.0,
            min_calc_len: 0.0,
            mode: Mode::default(),
            upload_from: None,
        }
    }
}

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}, {}, {}, {}, {}, {}",
            self.name,
            self.instrument,
            self.instrument_serial,
            self.main_gas
                .as_ref()
                .map(|g| format!("{:?}", g))
                .unwrap_or_else(|| "None".to_string()),
            self.deadband,
            self.min_calc_len
        )
    }
}

impl Project {
    pub fn load(db_path: Option<String>, name: &String) -> Option<Project> {
        let mut conn = Connection::open("fluxrs.db").ok()?;
        if db_path.is_some() {
            conn = Connection::open(db_path.unwrap()).ok()?;
        }

        let result: Result<(String, String, String, usize, f64, f64, u8), _> = conn.query_row(
            "SELECT project_id, instrument_serial, instrument_model, main_gas, deadband, min_calc_len, mode FROM projects WHERE project_id = ?",
            [name],
            |row| Ok((
                row.get(0)?, // project_id
                row.get(1)?, // instrument_serial
                row.get(2)?, // instrument_model
                row.get(3)?, // main_gas
                row.get(4)?, // deadband
                row.get(5)?, // min_calc_len
                row.get(6)?, // mode
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
        ) = result.ok()?;

        let main_gas = GasType::from_int(gas_i);
        let mode = Mode::from_int(mode_i)?;
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
                project_id, instrument_model, instrument_serial, main_gas, deadband, min_calc_len, mode, current
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                project.name,
                project.instrument.to_string(),
                project.instrument_serial,
                project.main_gas.unwrap().as_int(),
                project.deadband,
                project.min_calc_len,
                project.mode.as_int(),
                0,
            ],
        )?;

        Ok(())
    }
}

pub struct ProjectApp {
    pub project: Option<Project>,
    all_projects: Vec<Project>,
    project_name: String,
    selected_instrument: InstrumentType,
    selected_serial: String,
    main_gas: Option<GasType>,
    deadband: f64,
    min_calc_len: f64,
    mode: Mode,
}

impl Default for ProjectApp {
    fn default() -> Self {
        Self {
            project: None,
            all_projects: Vec::new(),
            project_name: String::new(),
            selected_instrument: InstrumentType::default(),
            selected_serial: String::new(),
            main_gas: Some(GasType::default()),
            min_calc_len: MIN_CALC_AREA_RANGE,
            deadband: 30.,
            mode: Mode::default(),
        }
    }
}
impl ProjectApp {
    pub fn proj_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Project Management");
        ui.separator();

        // Load all projects once
        if self.all_projects.is_empty() {
            if let Err(err) = self.load_projects_from_db() {
                ui.colored_label(egui::Color32::RED, format!("Failed to load projects: {}", err));
            }
        }

        // Ensure current project exists for new-project form
        if self.project.is_none() {
            self.project = Some(Project::default()); // or your own constructor like Project::new()
        }

        ui.heading("Change current project");
        ui.add_space(10.0);

        if !self.all_projects.is_empty() {
            egui::ComboBox::from_label("Current project")
                .selected_text(
                    self.project
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "Select Project".to_string()),
                )
                .show_ui(ui, |ui| {
                    for project in &self.all_projects {
                        let is_selected =
                            self.project.as_ref().map(|p| p.name == project.name).unwrap_or(false);
                        if ui.selectable_label(is_selected, &project.name).clicked() {
                            if let Err(err) = self.set_current_project(&project.name) {
                                eprintln!("Failed to set project as current: {}", err);
                            }
                            self.project = Some(project.clone());
                        }
                    }
                });
        } else {
            ui.label("No projects found.");
        }

        ui.separator();
        ui.collapsing("Instructions", |ui| {
            ui.label("Project name");
            ui.label("Instrument");
            ui.label("Main gas");
        });

        ui.heading("New project");

        // let project = self.project.as_mut().unwrap(); // Safe now

        ui.label("Project name:");
        ui.text_edit_singleline(&mut self.project_name);

        ui.label("Select instrument:");
        egui::ComboBox::from_label("Instrument")
            .selected_text(self.selected_instrument.to_string())
            .show_ui(ui, |ui| {
                for instrument in InstrumentType::available_instruments() {
                    ui.selectable_value(
                        &mut self.selected_instrument,
                        instrument,
                        instrument.to_string(),
                    );
                }
            });

        ui.label("Instrument serial:");
        ui.text_edit_singleline(&mut self.selected_serial);

        let available_gases = self.selected_instrument.available_gases();
        if !available_gases.is_empty() {
            ui.label("Select Gas:");
            egui::ComboBox::from_label("Gas Type")
                .selected_text(
                    self.main_gas.map_or_else(|| "Select Gas".to_string(), |g| g.to_string()),
                )
                .show_ui(ui, |ui| {
                    for gas in available_gases {
                        ui.selectable_value(&mut self.main_gas, Some(gas), gas.to_string());
                    }
                });

            if let Some(gas) = self.main_gas {
                ui.label(format!("Selected Gas: {}", gas));
            }
        } else {
            ui.label("No gases available for this instrument.");
        }

        ui.add_space(10.0);
        ui.label("Minimum calculation data length in seconds");
        ui.label("Don't make this longer than your measurement.");
        ui.add(egui::DragValue::new(&mut self.min_calc_len).speed(1.0).range(0.0..=3600.0));
        ui.add_space(10.0);
        ui.label("Deadband in seconds:");
        ui.add(egui::DragValue::new(&mut self.deadband).speed(1.0).range(0.0..=3600.0));

        ui.add_space(10.0);
        ui.label("Select flux finding mode:");
        egui::ComboBox::from_label("Mode").selected_text(format!("{}", self.mode)).show_ui(
            ui,
            |ui| {
                ui.selectable_value(
                    &mut self.mode,
                    Mode::AfterDeadband,
                    Mode::AfterDeadband.to_string(),
                );
                ui.selectable_value(
                    &mut self.mode,
                    Mode::BestPearsonsR,
                    Mode::BestPearsonsR.to_string(),
                );
            },
        );

        ui.add_space(10.0);
        if ui.button("Add Project").clicked() {
            if let Some(project) = self.build_project_from_form() {
                if let Err(err) = self.save_project_to_db(&project) {
                    ui.colored_label(
                        egui::Color32::RED,
                        format!("Failed to save project: {}", err),
                    );
                } else {
                    self.project = Some(project.clone());
                    self.all_projects.push(project); // Optional: update list
                }
            } else {
                ui.colored_label(egui::Color32::YELLOW, "Please fill out all required fields.");
            }
        }
    }

    pub fn update_project(&mut self) -> Option<AppEvent> {
        Some(AppEvent::SelectProject(self.project.clone()))
    }
    pub fn build_project_from_form(&self) -> Option<Project> {
        Some(Project {
            name: self.project_name.clone(),
            instrument: self.selected_instrument,
            instrument_serial: self.selected_serial.clone(),
            main_gas: self.main_gas,
            deadband: self.deadband,
            mode: self.mode,
            min_calc_len: self.min_calc_len,
            upload_from: None,
        })
    }

    pub fn load_projects_from_db(&mut self) -> rusqlite::Result<()> {
        println!("loading project");
        self.all_projects = Vec::new();
        let conn = Connection::open("fluxrs.db")?;

        let mut stmt = conn.prepare("SELECT * FROM projects")?;

        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let column_index: HashMap<String, usize> =
            column_names.iter().enumerate().map(|(i, name)| (name.clone(), i)).collect();

        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(*column_index.get("project_id").unwrap())?;
            let model_string: String = row.get(*column_index.get("instrument_model").unwrap())?;

            let instrument = match model_string.parse::<InstrumentType>() {
                Ok(val) => val,
                Err(_) => {
                    eprintln!("Unexpected invalid instrument type from DB: '{}'", model_string);
                    process::exit(1);
                },
            };
            let instrument_serial: String =
                row.get(*column_index.get("instrument_serial").unwrap())?;
            let gas_int = row.get(*column_index.get("main_gas").unwrap())?;
            let main_gas = GasType::from_int(gas_int);
            let mode_int = row.get(*column_index.get("mode").unwrap())?;
            let mode = Mode::from_int(mode_int).unwrap();
            let deadband = row.get(*column_index.get("deadband").unwrap())?;
            let min_calc_len = row.get(*column_index.get("min_calc_len").unwrap())?;

            self.all_projects.push(Project {
                name,
                instrument,
                instrument_serial,
                deadband,
                min_calc_len,
                main_gas,
                mode,
                upload_from: None,
            })
        }

        println!("loading current project");
        let result: Result<(String, String,String, usize, f64, f64, u8), _> = conn.query_row(
            "SELECT project_id, instrument_serial, instrument_model, main_gas, deadband, min_calc_len, mode FROM projects WHERE current = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?,row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        );

        match result {
            Ok((
                project_id,
                instrument_serial,
                instrument_string,
                gas_i,
                deadband,
                min_calc_len,
                mode_i,
            )) => {
                let name = project_id.clone();
                let serial = instrument_serial.clone();

                let main_gas = GasType::from_int(gas_i);
                let mode = Mode::from_int(mode_i).unwrap();

                let instrument = match instrument_string.parse::<InstrumentType>() {
                    Ok(val) => val,
                    Err(_) => {
                        eprintln!(
                            "Unexpected invalid instrument type from DB: '{}'",
                            instrument_string
                        );
                        process::exit(1);
                    },
                };

                let project = Project {
                    name,
                    instrument,
                    instrument_serial: serial,
                    main_gas,
                    deadband,
                    min_calc_len,
                    mode,
                    upload_from: None,
                };

                self.project = Some(project); // assuming you have this field
            },
            Err(_) => {
                self.project = None;
                // self.instrument_serial = String::new();
                // self.current_project = None; // clear if failed to load
            },
        }

        Ok(())
    }
    fn set_current_project(&self, project_name: &str) -> rusqlite::Result<()> {
        let mut conn = Connection::open("fluxrs.db")?;
        let tx = conn.transaction()?;
        tx.execute("UPDATE projects SET current = 0 WHERE current = 1", [])?;
        tx.execute("UPDATE projects SET current = 1 WHERE project_id = ?1", [project_name])?;
        tx.commit()?;
        println!("Current project set: {}", project_name);

        Ok(())
    }
    fn save_project_to_db(&mut self, project: &Project) -> rusqlite::Result<()> {
        let mut conn = Connection::open("fluxrs.db")?;
        let current_project = project.clone();

        let main_gas = self.main_gas.unwrap().as_int();
        let instrument_model = self.selected_instrument.to_string();
        let deadband = self.deadband;
        let mode = self.mode.as_int();
        let min_calc_len = self.min_calc_len;

        let tx = conn.transaction()?; //   Use transaction for consistency

        tx.execute("UPDATE projects SET current = 0 WHERE current = 1", [])?;

        tx.execute(
            "INSERT OR REPLACE INTO projects (project_id, main_gas, instrument_model, instrument_serial, deadband, min_calc_len, mode, current)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![&self.project_name, &main_gas, &instrument_model, &self.selected_serial, &deadband, min_calc_len, &mode],
        )?;

        tx.commit()?; //   Commit the transaction

        println!(
            "Project set as current: {}, {}, {}, {}, {}",
            current_project.name, main_gas, instrument_model, deadband, mode
        );

        self.load_projects_from_db()?;

        Ok(())
    }
}
