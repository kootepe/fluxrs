use crate::constants::MIN_CALC_AREA_RANGE;
use crate::gastype::GasType;
use crate::instruments::instruments::InstrumentType;
use crate::ui::main_app::AppEvent;
use crate::ui::manage_proj::manage_ui::ManageApp;
use crate::ui::manage_proj::project::ProjectExistsError;
use crate::ui::tz_picker::TimezonePickerState;
use crate::ui::validation_ui::Mode;
use crate::Project;
use chrono_tz::Tz;
use egui::Color32;
use egui::{Area, Button, Context, Id};
use std::fmt;
use std::process;

use rusqlite::{params, Connection, Result};
use std::collections::HashMap;

#[derive(Clone)]
pub enum MsgType {
    Good(String),
    Bad(String),
}

impl fmt::Display for MsgType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MsgType::Good(str) => write!(f, "{}", str),
            MsgType::Bad(str) => write!(f, "{}", str),
        }
    }
}
impl MsgType {
    pub fn as_str_and_color(&self) -> (String, egui::Color32) {
        match self {
            MsgType::Good(msg) => (msg.clone(), egui::Color32::GREEN),
            MsgType::Bad(msg) => (msg.clone(), egui::Color32::RED),
        }
    }
}

pub struct ProjectApp {
    pub project: Option<Project>,
    pub all_projects: Vec<Project>,
    pub project_name: String,
    pub selected_instrument: InstrumentType,
    pub selected_serial: String,
    pub main_gas: Option<GasType>,
    pub deadband: f64,
    pub min_calc_len: f64,
    pub mode: Mode,
    pub tz_state: TimezonePickerState,
    pub project_timezone: Option<Tz>, // store the choice (or keep as String if you prefer)
    pub project_timezone_str: String,
    pub message: Option<MsgType>,
    pub del_message: Option<MsgType>,
    pub proj_create_open: bool,
    pub proj_delete_open: bool,
    pub proj_manage_open: bool,
    pub verify_delete_open: bool,
    pub delete_success: bool,
    pub proj_to_delete: Option<String>,
    pub manage: ManageApp,
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
            tz_state: TimezonePickerState::default(),
            project_timezone: None,
            project_timezone_str: String::new(),
            mode: Mode::default(),
            message: None,
            del_message: None,
            proj_create_open: false,
            proj_delete_open: false,
            proj_manage_open: false,
            verify_delete_open: false,
            delete_success: false,
            proj_to_delete: None,
            manage: ManageApp::new(),
        }
    }
}
impl ProjectApp {
    pub fn proj_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.heading("Project Management");
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            if ui.add(Button::new("Create project").fill(Color32::DARK_GREEN)).clicked() {
                self.proj_create_open = true;
            }
            if ui
                .add_enabled(self.project.is_some(), Button::new("Manage current project data"))
                .clicked()
            {
                self.manage.open = true;
            }
            if ui
                .add_enabled(
                    !self.all_projects.is_empty(),
                    Button::new("Delete projects").fill(Color32::DARK_RED),
                )
                .clicked()
            {
                self.proj_delete_open = true;
            }
        });

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

        ui.add_space(10.0);
        ui.heading("Change current project");
        ui.add_space(5.0);

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
        let any_prompt_open = self.proj_create_open
            || self.proj_delete_open
            || self.verify_delete_open
            || self.manage.open;
        if any_prompt_open {
            input_block_overlay(ctx, "blocker222");
        }
        self.manage.show_manage_proj_data(ctx, self.project.clone().unwrap());
        self.show_proj_create_prompt(ctx);
        self.show_proj_delete_prompt(ctx);
        self.show_verify_delete(ctx);
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
            tz: self.project_timezone.unwrap_or_default(),
            upload_from: None,
        })
    }

    pub fn load_projects_from_db(&mut self) -> rusqlite::Result<()> {
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
            let tz_str: String = row.get(*column_index.get("tz").unwrap())?;
            let tz: Tz = tz_str.parse().expect("Invalid timezone string");

            self.all_projects.push(Project {
                name,
                instrument,
                instrument_serial,
                deadband,
                min_calc_len,
                main_gas,
                mode,
                tz,
                upload_from: None,
            })
        }

        let result: Result<(String, String,String, usize, f64, f64, u8, String), _> = conn.query_row(
            "SELECT project_id, instrument_serial, instrument_model, main_gas, deadband, min_calc_len, mode, tz FROM projects WHERE current = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?,row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
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
                tz_str,
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
                let tz: Tz = tz_str.parse().expect("Invalid timezone string");

                let project = Project {
                    name,
                    instrument,
                    instrument_serial: serial,
                    main_gas,
                    deadband,
                    min_calc_len,
                    mode,
                    tz,
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
    pub fn save_project_to_db(&mut self, project: &Project) -> rusqlite::Result<()> {
        let mut conn = Connection::open("fluxrs.db")?;
        let current_project = project.clone();

        let main_gas = self.main_gas.unwrap().as_int();
        let instrument_model = self.selected_instrument.to_string();
        let deadband = self.deadband;
        let mode = self.mode.as_int();
        let min_calc_len = self.min_calc_len;
        let tz = &self.project_timezone_str;

        let tx = conn.transaction()?; //   Use transaction for consistency

        if self.check_exists(&self.project_name)? {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(ProjectExistsError { project_id: self.project_name.clone() }),
            ));
        }
        tx.execute("UPDATE projects SET current = 0 WHERE current = 1", [])?;

        tx.execute(
            "INSERT OR REPLACE INTO projects (project_id, main_gas, instrument_model, instrument_serial, deadband, min_calc_len, mode, tz, current)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1)",
            params![&self.project_name, &main_gas, &instrument_model, &self.selected_serial, &deadband, min_calc_len, &mode, tz],
        )?;

        tx.commit()?; //   Commit the transaction

        println!(
            "Project set as current: {}, {}, {}, {}, {}",
            current_project.name, main_gas, instrument_model, deadband, mode
        );

        self.load_projects_from_db()?;

        Ok(())
    }
    fn check_exists(&self, project_id: &str) -> Result<bool> {
        let conn = Connection::open("fluxrs.db")?;

        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE project_id = ?1 LIMIT 1)",
            params![project_id],
            |row| row.get(0),
        )?;

        Ok(exists)
    }
}

pub fn input_block_overlay(ctx: &Context, id_name: &str) -> egui::InnerResponse<()> {
    Area::new(Id::new(id_name)).order(egui::Order::Background).interactable(true).show(ctx, |ui| {
        let desired_size = ui.ctx().screen_rect().size();
        let (rect, _resp) = ui.allocate_exact_size(desired_size, egui::Sense::click());

        // Dark translucent backdrop
        ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 160));
    })
}

pub fn clicked_outside_window<R>(
    ctx: &egui::Context,
    response: Option<&egui::InnerResponse<R>>,
) -> bool {
    if let Some(resp) = response {
        ctx.input(|i| {
            if i.pointer.any_pressed() {
                if let Some(click_pos) = i.pointer.press_origin() {
                    return !resp.response.rect.contains(click_pos);
                }
            }
            false
        })
    } else {
        false
    }
}
