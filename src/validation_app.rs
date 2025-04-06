use crate::app_plotting::{
    init_calc_r_plot, init_flux_plot, init_gas_plot, init_lag_plot, init_measurement_r_plot,
};
use crate::csv_parse;
use crate::cycle::{insert_fluxes_ignore_duplicates, load_fluxes, update_fluxes};
use crate::errorcode::ErrorCode;
use crate::gasdata::{insert_measurements, GasData};
use crate::index::Index;
use crate::instruments::InstrumentType;
use crate::instruments::{GasType, Li7810};
use crate::meteodata::{insert_meteo_data, query_meteo, MeteoData};
use crate::query_gas;
use crate::timedata::{query_cycles, TimeData};
use crate::volumedata::{insert_volume_data, query_volume, VolumeData};
use crate::Cycle;
use crate::EqualLen;
use crate::{insert_cycles, process_cycles};

use eframe::egui::{Color32, Context, Id, Key, PointerButton, Stroke, Ui, WidgetInfo, WidgetType};
use egui_file::FileDialog;
use egui_plot::{PlotPoints, PlotUi, Polygon};

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rusqlite::{types::ValueRef, Connection, Result, Row};

pub enum DataType {
    Gas,
    Cycle,
    Meteo,
    Volume,
}
#[derive(PartialEq, Eq)]
pub enum Panel {
    Validation,
    DataInit,
    DataLoad,
    FileInit,
    ProjectInit,
    DataTable,
    Empty,
}

impl Default for Panel {
    fn default() -> Self {
        Self::Empty
    }
}
#[derive(Default)]
pub struct MainApp {
    current_panel: Panel,
    validation_panel: ValidationApp,
    table_panel: TableApp,
    empty_panel: EmptyPanel,
}
impl MainApp {
    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            let container_response = ui.response();
            container_response
                .widget_info(|| WidgetInfo::labeled(WidgetType::RadioGroup, true, "Select panel"));

            ui.ctx().clone().with_accessibility_parent(container_response.id, || {
                ui.selectable_value(
                    &mut self.current_panel,
                    Panel::Validation,
                    "Validate measurements",
                );
                ui.selectable_value(&mut self.current_panel, Panel::DataLoad, "Load measurements");
                ui.selectable_value(
                    &mut self.current_panel,
                    Panel::DataInit,
                    "Initiate measurements",
                );
                ui.selectable_value(&mut self.current_panel, Panel::FileInit, "Upload files to db");
                ui.selectable_value(
                    &mut self.current_panel,
                    Panel::ProjectInit,
                    "Select and initiate project",
                );
                ui.selectable_value(&mut self.current_panel, Panel::DataTable, "View data in db");
            });
        });
        ui.separator();
        if !self.validation_panel.initiated {
            self.validation_panel.load_projects_from_db().unwrap();
        }

        match self.current_panel {
            Panel::Validation => {
                self.validation_panel.ui(ui, ctx);
            },
            Panel::DataLoad => {
                self.validation_panel.load_ui(ui, ctx);
            },
            Panel::DataInit => {
                self.validation_panel.init_ui(ui, ctx);
            },
            Panel::FileInit => {
                self.validation_panel.file_ui(ui, ctx);
            },
            Panel::DataTable => {
                self.table_panel.table_ui(ui, ctx);
            },
            Panel::ProjectInit => {
                self.validation_panel.proj_ui(ui);
            },
            Panel::Empty => {
                self.empty_panel.ui(ui);
            },
        }
    }
}

// #[derive(Default)]
pub struct ValidationApp {
    pub runtime: tokio::runtime::Runtime,
    init_enabled: bool,
    init_in_progress: bool,
    pub task_done_sender: Sender<()>,
    pub task_done_receiver: Receiver<()>,
    pub instrument_serial: String,
    pub enabled_gases: HashSet<GasType>, // Stores which gases are enabled for plotting
    pub enabled_fluxes: HashSet<GasType>, // Stores which fluxes are enabled for plotting
    pub enabled_calc_rs: HashSet<GasType>, // Stores which r values are enabled for plotting
    pub enabled_measurement_rs: HashSet<GasType>, // Stores which gases are enabled for plotting
    pub cycles: Vec<Cycle>,
    pub gases: Vec<GasType>,
    pub lag_plot_w: f32,
    pub lag_plot_h: f32,
    pub gas_plot_w: f32,
    pub gas_plot_h: f32,
    pub flux_plot_w: f32,
    pub flux_plot_h: f32,
    pub measurement_r_plot_w: f32,
    pub measurement_r_plot_h: f32,
    pub calc_r_plot_w: f32,
    pub calc_r_plot_h: f32,
    pub lag_idx: f64, // Add a vecxy tor of values to your struct
    pub close_idx: f64,
    pub open_offset: f64,
    pub close_offset: f64,
    pub open_idx: f64,
    pub start_time_idx: f64,
    pub end_time_idx: f64,
    pub calc_range_start: HashMap<GasType, f64>,
    pub calc_range_end: HashMap<GasType, f64>,
    pub calc_r2: HashMap<GasType, f64>,
    pub measurement_r2: HashMap<GasType, f64>,
    pub flux: HashMap<GasType, f64>,
    pub measurement_max_y: HashMap<GasType, f64>,
    pub measurement_min_y: HashMap<GasType, f64>,
    pub zoom_to_measurement: bool,
    pub drag_panel_width: f64,
    pub min_calc_area_range: f64,
    pub index: Index,
    pub selected_point: Option<[f64; 2]>,
    pub dragged_point: Option<[f64; 2]>,
    pub chamber_colors: HashMap<String, Color32>, // Stores colors per chamber
    pub visible_traces: HashMap<String, bool>,
    pub all_traces: HashSet<String>,
    pub visible_cycles: Vec<usize>,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub opened_files: Option<Vec<PathBuf>>,
    pub open_file_dialog: Option<FileDialog>,
    pub initial_path: Option<PathBuf>,
    pub selected_data_type: Option<DataType>,
    pub log_messages: Vec<String>,
    pub show_valids: bool,
    pub show_invalids: bool,
    pub project_name: String,
    pub main_gas: Option<GasType>,
    pub instrument: InstrumentType,
    pub projects: Vec<String>,
    pub initiated: bool,
    pub selected_project: Option<String>,
    pub proj_serial: String,
}

impl Default for ValidationApp {
    fn default() -> Self {
        let (task_done_sender, task_done_receiver) = std::sync::mpsc::channel();
        Self {
            runtime: tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap(),
            task_done_sender,
            task_done_receiver,
            init_enabled: true,
            init_in_progress: false,
            instrument_serial: String::new(),
            proj_serial: String::new(),
            enabled_gases: HashSet::new(),
            enabled_fluxes: HashSet::new(),
            enabled_measurement_rs: HashSet::new(),
            enabled_calc_rs: HashSet::new(),
            cycles: Vec::new(),
            gases: Vec::new(),
            lag_plot_w: 600.,
            lag_plot_h: 350.,
            gas_plot_w: 600.,
            gas_plot_h: 350.,
            flux_plot_w: 600.,
            flux_plot_h: 350.,
            calc_r_plot_w: 600.,
            calc_r_plot_h: 350.,
            measurement_r_plot_w: 600.,
            measurement_r_plot_h: 350.,
            lag_idx: 0.0,
            close_idx: 0.0,
            open_offset: 0.0,
            close_offset: 0.0,
            open_idx: 0.0,
            start_time_idx: 0.0,
            end_time_idx: 0.0,
            calc_range_start: HashMap::new(),
            calc_range_end: HashMap::new(),
            calc_r2: HashMap::new(),
            measurement_r2: HashMap::new(),
            flux: HashMap::new(),
            measurement_max_y: HashMap::new(),
            measurement_min_y: HashMap::new(),
            zoom_to_measurement: false,
            drag_panel_width: 40.0, // Default width for UI panel
            min_calc_area_range: 180.0,
            index: Index::default(), // Assuming Index implements Default
            selected_point: None,
            dragged_point: None,
            chamber_colors: HashMap::new(),
            visible_traces: HashMap::new(),
            all_traces: HashSet::new(),
            visible_cycles: Vec::new(),
            end_date: NaiveDate::from_ymd_opt(2024, 9, 25)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            start_date: NaiveDate::from_ymd_opt(2024, 9, 23)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            opened_files: None,
            open_file_dialog: None,
            initial_path: Some(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            selected_data_type: None,
            log_messages: Vec::new(),
            show_invalids: true,
            show_valids: true,
            project_name: String::new(),
            main_gas: None,
            instrument: InstrumentType::Li7810,
            projects: Vec::new(),
            initiated: false,
            selected_project: None,
        }
    }
}
impl ValidationApp {
    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        if self.cycles.is_empty() {
            ui.label("No cycles with data loaded, use Initiate measurements tab to initiate data.");
            return;
        }
        let main_gas = self.cycles[*self.index].main_gas;
        egui::Window::new("Select visible traces").max_width(50.).show(ctx, |ui| {
            self.render_legend(ui, &self.chamber_colors.clone());
        });
        egui::Window::new("Adjust plot widths").show(ctx, |ui| {
            ui.label("Drag boxes right/left or down/up to adjust plot sizes.");
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.lag_plot_w)
                            .speed(1.)
                            .range(150.0..=1920.0)
                            .prefix("Lag plot width: "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.lag_plot_h)
                            .speed(1.)
                            .range(150.0..=1920.0)
                            .prefix("Lag plot height: "),
                    );
                });
                ui.vertical(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.flux_plot_w)
                            .speed(1.)
                            .range(150.0..=1920.0)
                            .prefix("Flux plot width: "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.flux_plot_h)
                            .speed(1.)
                            .range(150.0..=1920.0)
                            .prefix("Flux plot height: "),
                    );
                });
                ui.vertical(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.gas_plot_w)
                            .speed(1.)
                            .range(150.0..=1920.0)
                            .prefix("Gas plot width: "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.gas_plot_h)
                            .speed(1.)
                            .range(150.0..=1920.0)
                            .prefix("Gas plot height: "),
                    );
                });
            });
        });
        egui::Window::new("Select displayed plots").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("main gas plots");
                        for &gas in &self.cycles[*self.index].gases {
                            let mut is_enabled = self.is_gas_enabled(&gas);
                            ui.checkbox(&mut is_enabled, format!("{:?}", gas));

                            // update the enabled_gases set when the checkbox is toggled
                            if is_enabled {
                                self.enabled_gases.insert(gas);
                            } else {
                                self.enabled_gases.remove(&gas);
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Flux plots");
                        for &gas in &self.cycles[*self.index].gases {
                            let mut is_enabled = self.is_flux_enabled(&gas);
                            ui.checkbox(&mut is_enabled, format!("{:?}", gas));

                            // Update the enabled_gases set when the checkbox is toggled
                            if is_enabled {
                                self.enabled_fluxes.insert(gas);
                            } else {
                                self.enabled_fluxes.remove(&gas);
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Calc r plots");
                        for &gas in &self.cycles[*self.index].gases {
                            let mut is_enabled = self.is_measurement_r_enabled(&gas);
                            ui.checkbox(&mut is_enabled, format!("{:?}", gas));

                            // Update the enabled_gases set when the checkbox is toggled
                            if is_enabled {
                                self.enabled_measurement_rs.insert(gas);
                            } else {
                                self.enabled_measurement_rs.remove(&gas);
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Measurement r plots");
                        for &gas in &self.cycles[*self.index].gases {
                            let mut is_enabled = self.is_calc_r_enabled(&gas);
                            ui.checkbox(&mut is_enabled, format!("{:?}", gas));

                            // Update the enabled_gases set when the checkbox is toggled
                            if is_enabled {
                                self.enabled_calc_rs.insert(gas);
                            } else {
                                self.enabled_calc_rs.remove(&gas);
                            }
                        }
                    });
                });
            });
        });

        egui::Window::new("Current Cycle details").show(ctx, |ui| {
            // let errors = ErrorCode::from_mask(self.cycles[*self.index].error_code.0);
            let errors = ErrorCode::from_mask(self.cycles[*self.index].error_code.0);
            let error_messages: Vec<String> =
                errors.iter().map(|error| error.to_string()).collect();
            // for error in errors {
            //     println!("{}", error);
            // }
            let current_pts = format!(
                "Showing: {}/{} cycles in current range.",
                self.visible_cycles.len(),
                self.cycles.len(),
            );
            let datetime = format!("datetime: {}", self.cycles[*self.index].start_time);
            let model = format!("model: {}", self.cycles[*self.index].instrument_model);
            let serial = format!("serial: {}", self.cycles[*self.index].instrument_serial);
            let ch_id = format!("Chamber: {}", self.cycles[*self.index].chamber_id.clone());
            let valid_txt = format!("Is valid: {}", self.cycles[*self.index].is_valid);
            let vld = format!("manual valid: {:?}", self.cycles[*self.index].manual_valid);
            let over = format!("override: {:?}", self.cycles[*self.index].override_valid);
            let error = format!("override: {:?}", self.cycles[*self.index].error_code.0);
            ui.label(model);
            ui.label(serial);
            ui.label(datetime);
            ui.label(current_pts);
            ui.label(ch_id);
            ui.label(valid_txt);
            ui.label(vld);
            ui.label(over);
            ui.label(error);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    for gas in &self.enabled_gases {
                        // let r_val = match self.cycles[*self.index].calc_r.get(gas) {
                        let r_val = match self.cycles[*self.index].calc_r2.get(gas) {
                            Some(r) => format!("calc_r2 {} : {:.6}", gas, r),
                            None => "Calc r: N/A".to_string(), // Handle missing data
                        };
                        ui.label(r_val);
                    }
                });

                // NOTE: BAD CLONE
                ui.vertical(|ui| {
                    for gas in &self.enabled_gases {
                        // let flux = match self.cycles[*self.index].flux.get(gas) {
                        let flux = match self.cycles[*self.index].flux.get(gas) {
                            Some(r) => format!("flux {} : {:.6}", gas, r),
                            None => "Flux: N/A".to_string(), // Handle missing data
                        };
                        ui.label(flux);
                    }
                });
            });
            let measurement_r2 = match self.cycles[*self.index].measurement_r2.get(&main_gas) {
                Some(r) => format!("measurement r2: {:.6}", r),
                None => "Measurement r: N/A".to_string(), // Handle missing data
            };
            // );
            ui.label(measurement_r2);
            ui.label(error_messages.join("\n"));
            // let flux = format!("flux: {:.6}", self.cycles[*self.index].flux);

            // egui::SidePanel::left("my_left_panel").show(ctx, |ui| {});

            ui.style_mut().text_styles.insert(
                egui::TextStyle::Button,
                egui::FontId::new(14.0, eframe::epaint::FontFamily::Proportional),
            );
        });

        // let gas_plot = self.create_gas_plot();
        // let lag_plot = self.create_lag_plot();

        let mut prev_clicked = false;
        let mut next_clicked = false;
        let mut highest_r = false;
        let mut reset_cycle = false;
        let mut toggle_valid = false;
        // let mut show_invalids_clicked = false;
        // let mut show_valids_clicked = false;
        // let mut zoom_clicked = false;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            prev_clicked = ui.add(egui::Button::new("Prev measurement")).clicked();
            next_clicked = ui.add(egui::Button::new("Next measurement")).clicked();
        });

        let show_valids_clicked = ui.checkbox(&mut self.show_valids, "Show valids").clicked();
        let mut show_invalids_clicked =
            ui.checkbox(&mut self.show_invalids, "Show invalids").clicked();
        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            highest_r = ui.add(egui::Button::new("Find highest r")).clicked();
            reset_cycle = ui.add(egui::Button::new("Reset cycle")).clicked();
            toggle_valid = ui.add(egui::Button::new("Toggle validity")).clicked();
        });

        ui.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::Key { key: Key::H, pressed, .. } = event {
                    if *pressed {
                        self.show_invalids = !self.show_invalids;
                        show_invalids_clicked = true;
                    }
                }
                if let egui::Event::Key { key: Key::I, pressed, .. } = event {
                    if *pressed {
                        toggle_valid = true;
                    }
                }
                if let egui::Event::Key { key: Key::ArrowLeft, pressed, .. } = event {
                    if *pressed {
                        prev_clicked = true;
                    }
                }
                if let egui::Event::Key { key: Key::ArrowRight, pressed, .. } = event {
                    if *pressed {
                        next_clicked = true;
                    }
                }
                if let egui::Event::Key { key: Key::Z, pressed, .. } = event {
                    if *pressed {
                        self.zoom_to_measurement = !self.zoom_to_measurement;
                    }
                }
                if let egui::Event::Key { key: Key::R, pressed, .. } = event {
                    if *pressed {
                        reset_cycle = true;
                    }
                }

                if let egui::Event::Key { key: Key::S, pressed, .. } = event {
                    if *pressed && *self.index > 0 {
                        let (before, after) = self.cycles.split_at_mut(*self.index);
                        let current_cycle = &mut after[0]; // The cycle at *self.index (mutable)

                        // Loop backwards through `before` to find a matching chamber_id
                        if let Some(previous_cycle) = before
                            .iter()
                            .rev()
                            .find(|cycle| cycle.chamber_id == current_cycle.chamber_id)
                        {
                            // If found, copy `lag_s`
                            current_cycle.lag_s = previous_cycle.lag_s;
                            let target = current_cycle.start_time
                                + chrono::TimeDelta::seconds(current_cycle.open_offset)
                                + chrono::TimeDelta::seconds(current_cycle.lag_s as i64);
                            current_cycle.get_peak_near_timestamp(main_gas, target.timestamp());
                            self.update_current_cycle();
                            self.update_plots();
                        }
                    }
                }
                if let egui::Event::Key { key: Key::ArrowDown, pressed, .. } = event {
                    if *pressed {
                        self.cycles[*self.index].lag_s -= 1.;
                        // self.cycles[*self.index].update_cycle();
                        self.update_current_cycle();
                        self.update_plots();
                    }
                }
                if let egui::Event::Key { key: Key::ArrowUp, pressed, .. } = event {
                    if *pressed {
                        self.cycles[*self.index].lag_s += 1.;
                        // self.cycles[*self.index].update_cycle();
                        self.update_current_cycle();
                        // let mut conn = Connection::open("fluxrs.db").unwrap();
                        // let proj = self.project_name.unwrap().clone();

                        // let proj = self.selected_project.as_ref().unwrap().clone();
                        // insert_fluxes(&mut conn, &[self.cycles[*self.index].clone()], proj);
                        self.update_plots();
                    }
                }
            }
        });
        ui.add_space(10.);

        if show_invalids_clicked {
            self.update_plots();
        }
        if show_valids_clicked {
            self.update_plots();
        }
        if toggle_valid {
            self.cycles[*self.index].toggle_manual_valid();
            self.update_current_cycle();
            self.update_plots();
        }

        if reset_cycle {
            self.cycles[*self.index].reset();
            self.update_current_cycle();
            self.update_plots();
        }

        if highest_r {
            self.cycles[*self.index].recalc_r();
            self.update_current_cycle();
            self.update_plots();
        }
        // jump to the nearest point if current point is not visible
        if !self.visible_cycles.contains(&self.index.count) {
            if let Some(nearest) = self.find_nearest_visible_cycle() {
                self.index.set(nearest);
                self.update_plots();
            } else {
                return; // No visible cycles, do nothing
            }
        }
        if prev_clicked {
            if let Some(current_pos) = self.visible_cycles.iter().position(|&i| i == *self.index) {
                // Find the previous index cyclically
                let new_index_pos = if current_pos == 0 {
                    self.visible_cycles.len() - 1 // Wrap to last visible index
                } else {
                    current_pos - 1
                };

                let new_index = self.visible_cycles[new_index_pos]; // Get the valid index

                self.index.set(new_index);
                self.update_plots();
            }
        }

        if next_clicked {
            // insert_cycle(conn, self.cycles[*self.index]);

            if let Some(current_pos) = self.visible_cycles.iter().position(|&i| i == *self.index) {
                // Find the next index cyclically
                let new_index_pos = (current_pos + 1) % self.visible_cycles.len();
                let new_index = self.visible_cycles[new_index_pos]; // Get the valid index

                self.index.set(new_index);
                self.update_plots();
            }
        }

        let lag_s = self.cycles[*self.index].lag_s;

        let drag_panel_width = 40.;
        let mut calc_area_color = Color32::BLACK;
        let mut calc_area_adjust_color = Color32::BLACK;
        let mut calc_area_stroke_color = Color32::BLACK;
        if ctx.style().visuals.dark_mode {
            calc_area_color = Color32::from_rgba_unmultiplied(255, 255, 255, 1);
            calc_area_adjust_color = Color32::from_rgba_unmultiplied(255, 255, 255, 20);
            calc_area_stroke_color = Color32::from_rgba_unmultiplied(255, 255, 255, 60);
        } else {
            calc_area_color = Color32::from_rgba_unmultiplied(0, 0, 0, 10);
            calc_area_adjust_color = Color32::from_rgba_unmultiplied(0, 0, 20, 20);
            calc_area_stroke_color = Color32::from_rgba_unmultiplied(0, 0, 0, 90);
        }

        // let close_line_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
        let left_id = Id::new("left_test");
        let main_id = Id::new("main_area");
        let right_id = Id::new("right_area");

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    for gas_type in self.enabled_gases.clone() {
                        if self.is_gas_enabled(&gas_type) {
                            let gas_plot = init_gas_plot(
                                &gas_type,
                                self.start_time_idx,
                                self.end_time_idx,
                                self.gas_plot_w,
                                self.gas_plot_h,
                            );
                            let response = gas_plot.show(ui, |plot_ui| {
                                self.render_gas_plot_ui(
                                    plot_ui,
                                    gas_type,
                                    lag_s,
                                    drag_panel_width,
                                    calc_area_color,
                                    calc_area_stroke_color,
                                    calc_area_adjust_color,
                                    main_id,
                                    left_id,
                                    right_id,
                                )
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                // Hide cursor
                                // println!("Gas plot is hovered!");
                            }
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    for gas in self.enabled_fluxes.clone() {
                        let measurement_r_plot =
                            init_flux_plot(&gas, self.flux_plot_w, self.flux_plot_h);
                        // ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                        let response = measurement_r_plot.show(ui, |plot_ui| {
                            self.render_attribute_plot(
                                plot_ui,
                                &gas,
                                |cycle, gas_type| *cycle.flux.get(gas_type).unwrap_or(&0.0),
                                "Flux",
                            );
                        });
                        if response.response.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            // Hide cursor
                            // println!("Gas plot is hovered!");
                        }
                    }
                });
                ui.vertical(|ui| {
                    for gas in self.enabled_measurement_rs.clone() {
                        let measurement_r_plot = init_measurement_r_plot(
                            &gas,
                            self.measurement_r_plot_w,
                            self.measurement_r_plot_h,
                        );
                        // ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                        let response = measurement_r_plot.show(ui, |plot_ui| {
                            self.render_attribute_plot(
                                plot_ui,
                                &gas,
                                |cycle, gas_type| {
                                    *cycle.measurement_r2.get(gas_type).unwrap_or(&0.0)
                                },
                                "Measurement r",
                            );
                        });
                        if response.response.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            // Hide cursor
                            // println!("Gas plot is hovered!");
                        }
                    }
                });
                ui.vertical(|ui| {
                    for gas in self.enabled_calc_rs.clone() {
                        let calc_r_plot =
                            init_calc_r_plot(&gas, self.calc_r_plot_w, self.calc_r_plot_h);
                        // ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                        let response = calc_r_plot.show(ui, |plot_ui| {
                            self.render_attribute_plot(
                                plot_ui,
                                &gas,
                                |cycle, gas_type| *cycle.calc_r2.get(gas_type).unwrap_or(&0.0),
                                "Calc r",
                            );
                        });
                        if response.response.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            // Hide cursor
                            // println!("Gas plot is hovered!");
                        }
                    }
                });
            });
        });
        ui.horizontal(|ui| {
            let lag_plot = init_lag_plot(&main_gas, self.lag_plot_w, self.lag_plot_h);
            let response = lag_plot.show(ui, |plot_ui| {
                self.render_lag_plot(plot_ui);
            });
            if response.response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                // Hide cursor
                // println!("Gas plot is hovered!");
            }
        });
    }

    // fn parse_dates(&mut self) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    //     let naive_start = match NaiveDateTime::parse_from_str(&self.start_date, "%Y-%m-%dT%H:%M:%S")
    //     {
    //         Ok(dt) => dt,
    //         Err(e) => {
    //             return Err(format!(
    //                 "Failed to parse start date: {}, {}\nUse format YYYY-MM-DDTHH:MM:SS",
    //                 self.start_date, e
    //             ));
    //         }
    //     };
    //
    //     let naive_end = match NaiveDateTime::parse_from_str(&self.end_date, "%Y-%m-%dT%H:%M:%S") {
    //         Ok(dt) => dt,
    //         Err(e) => {
    //             return Err(format!(
    //                 "Failed to parse end date: {}, {}\nUse format YYYY-MM-DDTHH:MM:SS",
    //                 self.end_date, e
    //             ));
    //         }
    //     };
    //
    //     let start = DateTime::<Utc>::from_utc(naive_start, Utc);
    //     let end = DateTime::<Utc>::from_utc(naive_end, Utc);
    //
    //     Ok((start, end))
    // }
    fn find_nearest_visible_cycle(&self) -> Option<usize> {
        // If no visible cycles exist, return None
        if self.visible_cycles.is_empty() {
            return None;
        }

        // Try to find the closest visible cycle
        self.visible_cycles
            .iter()
            .min_by_key(|&&i| (i as isize - *self.index as isize).abs()) // Find the closest visible cycle
            .copied()
    }
    fn log_display(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        if ui.button("Clear Log").clicked() {
            self.log_messages.clear();
        }
        ui.label("**Log Messages:**");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for message in &self.log_messages {
                ui.label(message);
            }
        });
    }
    pub fn _display_ui(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        if self.cycles.is_empty() {
            println!("No cycles");
            return;
        }

        ui.heading("Plot selection");
    }
    pub fn load_ui(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        let mut picker_start = self.start_date.date_naive();
        let mut picker_end = self.end_date.date_naive();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Pick start date:");

                if ui
                    .add(
                        egui_extras::DatePickerButton::new(&mut picker_start)
                            .highlight_weekends(false)
                            .id_salt("start_date"),
                    )
                    .changed()
                {
                    let pick = DateTime::<Utc>::from_naive_utc_and_offset(
                        NaiveDateTime::from(picker_start),
                        Utc,
                    );
                    if pick > self.end_date {
                        self.log_messages.push("Start date can't be after end date.".to_owned());
                    } else {
                        self.start_date = pick
                    }
                };
                let delta_days = self.end_date - self.start_date;
                if ui
                    .button(format!("Next {} days", delta_days.to_std().unwrap().as_secs() / 86400))
                    .clicked()
                {
                    self.start_date += delta_days;
                    self.end_date += delta_days;
                }
                if ui
                    .button(format!(
                        "Previous {:?} days",
                        delta_days.to_std().unwrap().as_secs() / 86400
                    ))
                    .clicked()
                {
                    self.start_date -= delta_days;
                    self.end_date -= delta_days;
                }
            });
            ui.vertical(|ui| {
                ui.label("Pick end date:");
                if ui
                    .add(
                        egui_extras::DatePickerButton::new(&mut picker_end)
                            .highlight_weekends(false)
                            .id_salt("end_date"),
                    )
                    .changed()
                {
                    let pick = DateTime::<Utc>::from_naive_utc_and_offset(
                        NaiveDateTime::from(picker_end),
                        Utc,
                    );
                    if pick < self.start_date {
                        self.log_messages.push("End date can't be before start date.".to_owned());
                    } else {
                        self.end_date = pick
                    }
                };
            });
        });
        if self.start_date > self.end_date {
            self.log_messages.push("End date can't be before start date.".to_owned());
        }
        if ui.button("Init from db").clicked() {
            let mut conn = match Connection::open("fluxrs.db") {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to open database: {}", e);
                    return; // Exit early if connection fails
                },
            };
            match load_fluxes(
                &mut conn,
                self.start_date,
                self.end_date,
                self.selected_project.as_ref().unwrap().clone(),
                self.instrument_serial.clone(),
                None,
            ) {
                Ok(cycles) => {
                    self.index.set(0);
                    self.cycles = cycles;
                    self.update_plots();
                },
                // invalidquery returned if cycles is empty
                Err(rusqlite::Error::InvalidQuery) => {
                    self.log_messages
                        .push("No cycles found in db, have you initiated the data?".to_owned());
                    eprintln!("No cycles found in db, have you initiated the data?")
                },
                Err(e) => eprintln!("Error processing cycles: {}", e),
            }
        }
        self.log_display(ui);
    }
    pub fn init_ui(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        if self.task_done_receiver.try_recv().is_ok() {
            // while let Ok(_) = self.task_done_receiver.try_recv() {
            self.init_in_progress = false;
            self.init_enabled = true;
            self.log_messages.push("Processing complete.".to_string());
        }
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        let mut picker_start = self.start_date.date_naive();
        let mut picker_end = self.end_date.date_naive();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Pick start date:");

                        if ui
                            .add(
                                egui_extras::DatePickerButton::new(&mut picker_start)
                                    .highlight_weekends(false)
                                    .id_salt("start_date"),
                            )
                            .changed()
                        {
                            let pick = DateTime::<Utc>::from_naive_utc_and_offset(
                                NaiveDateTime::from(picker_start),
                                Utc,
                            );
                            if pick > self.end_date {
                                self.log_messages
                                    .push("Start date can't be after end date.".to_owned());
                            } else {
                                self.start_date = pick
                            }
                        };
                        let delta_days = self.end_date - self.start_date;
                        if ui
                            .button(format!(
                                "Next {} days",
                                delta_days.to_std().unwrap().as_secs() / 86400
                            ))
                            .clicked()
                        {
                            self.start_date += delta_days;
                            self.end_date += delta_days;
                        }
                        if ui
                            .button(format!(
                                "Previous {:?} days",
                                delta_days.to_std().unwrap().as_secs() / 86400
                            ))
                            .clicked()
                        {
                            self.start_date -= delta_days;
                            self.end_date -= delta_days;
                        }
                        if ui
                            .add_enabled(
                                self.init_enabled && !self.init_in_progress,
                                egui::Button::new("Use range"),
                            )
                            .clicked()
                        {
                            self.init_enabled = false;
                            self.init_in_progress = true;
                            let start_date = self.start_date;
                            let end_date = self.end_date;
                            let project = self.selected_project.as_ref().unwrap().clone();
                            let instrument_serial = self.instrument_serial.clone();

                            // Create connection and wrap in Arc<Mutex<>>
                            let conn = match Connection::open("fluxrs.db") {
                                Ok(conn) => conn,
                                Err(e) => {
                                    eprintln!("Failed to open database: {}", e);
                                    return;
                                },
                            };
                            let arc_conn = Arc::new(Mutex::new(conn)); //   stays alive
                            let conn_guard = arc_conn.lock().unwrap();
                            match (
                                query_cycles(&conn_guard, start_date, end_date, project.clone()),
                                query_gas(
                                    &conn_guard,
                                    start_date,
                                    end_date,
                                    project.clone(),
                                    instrument_serial,
                                ),
                                query_meteo(&conn_guard, start_date, end_date, project.clone()),
                                query_volume(&conn_guard, start_date, end_date, project.clone()),
                            ) {
                                (Ok(times), Ok(gas_data), Ok(meteo_data), Ok(volume_data)) => {
                                    // ui.add_enabled(false);
                                    if times.start_time.is_empty() {
                                        self.log_messages.push(format!(
                                            "NO CYCLES FOUND IN RANGE {} - {}",
                                            start_date, end_date
                                        ));
                                    }
                                    if gas_data.is_empty() {
                                        self.log_messages.push(format!(
                                            "NO GAS DATA FOUND IN RANGE {} - {}",
                                            start_date, end_date
                                        ));
                                    }

                                    if !times.start_time.is_empty() && !gas_data.is_empty() {
                                        println!("Run processing.");
                                        let arc_conn_clone = arc_conn.clone(); //   now this is valid
                                        let sender = self.task_done_sender.clone(); // Channel to signal when done
                                        self.runtime.spawn(async move {
                                            run_processing(
                                                times,
                                                gas_data,
                                                meteo_data,
                                                volume_data,
                                                project,
                                                arc_conn_clone,
                                            )
                                            .await;
                                            let _ = sender.send(()); // done
                                        });
                                    } else {
                                        println!("Empty data.")
                                    }
                                },
                                e => eprintln!("Failed to query database: {:?}", e),
                            }
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label("Pick end date:");
                        if ui
                            .add(
                                egui_extras::DatePickerButton::new(&mut picker_end)
                                    .highlight_weekends(false)
                                    .id_salt("end_date"),
                            )
                            .changed()
                        {
                            let pick = DateTime::<Utc>::from_naive_utc_and_offset(
                                NaiveDateTime::from(picker_end),
                                Utc,
                            );
                            if pick < self.start_date {
                                self.log_messages
                                    .push("End date can't be before start date.".to_owned());
                            } else {
                                self.end_date = pick
                            }
                        };
                    });
                    // ui.vertical(|ui| {
                    //     ui.label("");
                    //     ui.button("inittest");
                    // });
                });
            });
            if self.start_date > self.end_date {
                self.log_messages.push("End date can't be before start date.".to_owned());
            }

            ui.separator();
            ui.vertical(|ui| {
                ui.label(
                    "Compare the current chamber volume of all calculated fluxes and recalculate if a new one is found.",
                );
                ui.label(
                    "Only changes the fluxes and volume, no need to redo manual validation.",
                );
                if ui.button("Recalculate.").clicked() {
                    let mut conn = match Connection::open("fluxrs.db") {
                        Ok(conn) => conn,
                        Err(e) => {
                            eprintln!("Failed to open database: {}", e);
                            return; // Exit early if connection fails
                        },
                    };
                    let project = self.project_name.clone();
                    match (load_fluxes(
                        &mut conn,
                        self.start_date,
                        self.end_date,
                        self.selected_project.as_ref().unwrap().clone(),
                        self.instrument_serial.clone(),
                        None,
                        ),
                        query_volume(&mut conn, self.start_date, self.end_date, self.project_name.clone())) {
                        (Ok(mut cycles), Ok(volumes)) => {
                            println!("volumes: {}", volumes.datetime.len());
                            self.runtime.spawn_blocking(move || {
                                for c in &mut cycles {
                                    c.chamber_volume = volumes.get_nearest_previous_volume(c.start_time.timestamp(),&c.chamber_id).unwrap_or(1.0);
                                    c.calculate_fluxes();
                            }
                                let mut conn = rusqlite::Connection::open("fluxrs.db").unwrap();
                                if let Err(e) = update_fluxes(&mut conn, &cycles, project) {
                                    eprintln!("Flux update error: {}", e);
                                }
                            });
                        },
                        // invalidquery returned if cycles is empty
                        (Err(rusqlite::Error::InvalidQuery), Err(e)) => {
                            self.log_messages
                                .push("No cycles found in db, have you initiated the data?".to_owned());
                            eprintln!("No cycles found in db, have you initiated the data?")
                        },
                        e => eprintln!("Error processing cycles"),
                        // (Err(_), Err(_)) => eprintln!("Error processing cycles.")
                    }
                }
            });
        });

        self.log_display(ui);
    }
    pub fn file_ui(&mut self, ui: &mut Ui, ctx: &Context) {
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("Select Gas Files").clicked() {
                self.selected_data_type = Some(DataType::Gas);
                self.open_file_dialog();
            }
            if ui.button("Select Cycle Files").clicked() {
                self.selected_data_type = Some(DataType::Cycle);
                self.open_file_dialog();
            }
            if ui.button("Select Meteo Files").clicked() {
                self.selected_data_type = Some(DataType::Meteo);
                self.open_file_dialog();
            }
            if ui.button("Select Volume Files").clicked() {
                self.selected_data_type = Some(DataType::Volume);
                self.open_file_dialog();
            }
        });

        // Handle file selection
        self.handle_file_selection(ctx);

        self.log_display(ui);
    }

    fn upload_cycle_data(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
        self.log_messages.push("Uploading cycle data...".to_string());

        let mut all_times = TimeData::new();

        for path in &selected_paths {
            match csv_parse::read_time_csv(path) {
                //   Pass `path` directly
                Ok(res) => {
                    if res.validate_lengths() {
                        all_times.chamber_id.extend(res.chamber_id);
                        all_times.start_time.extend(res.start_time);
                        all_times.close_offset.extend(res.close_offset);
                        all_times.open_offset.extend(res.open_offset);
                        all_times.end_offset.extend(res.end_offset);

                        self.log_messages.push(format!("Successfully read {:?}", path));
                    } else {
                        self.log_messages
                            .push(format!("Skipped file {:?}: Invalid data length", path));
                    }
                },
                Err(e) => {
                    self.log_messages.push(format!("Failed to read file {:?}: {}", path, e));
                },
            }
        }
        match insert_cycles(conn, &all_times, self.selected_project.as_ref().unwrap().clone()) {
            Ok(row_count) => {
                self.log_messages
                    .push(format!("Successfully inserted {} rows into DB.", row_count));
            },
            Err(e) => {
                self.log_messages.push(format!("Failed to insert cycle data to db.Error {}", e));
            },
        }
    }
    fn upload_meteo_data(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
        let mut meteos = MeteoData::default();
        for path in &selected_paths {
            match csv_parse::read_meteo_csv(path) {
                //   Pass `path` directly
                Ok(res) => {
                    meteos.datetime.extend(res.datetime);
                    meteos.pressure.extend(res.pressure);
                    meteos.temperature.extend(res.temperature);
                },
                Err(e) => {
                    self.log_messages.push(format!("Failed to read file {:?}: {}", path, e));
                },
            }
        }
        match insert_meteo_data(conn, &self.selected_project.as_ref().unwrap().clone(), &meteos) {
            Ok(row_count) => {
                self.log_messages
                    .push(format!("Successfully inserted {} rows into DB.", row_count));
            },
            Err(e) => {
                self.log_messages.push(format!("Failed to insert cycle data to db.Error {}", e));
            },
        }
        self.log_messages.push("Uploading meteo data...".to_string());
    }
    fn upload_volume_data(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
        let mut volumes = VolumeData::default();
        for path in &selected_paths {
            match csv_parse::read_volume_csv(path) {
                //   Pass `path` directly
                Ok(res) => {
                    volumes.datetime.extend(res.datetime);
                    volumes.chamber_id.extend(res.chamber_id);
                    volumes.volume.extend(res.volume);
                },
                Err(e) => {
                    self.log_messages.push(format!("Failed to read file {:?}: {}", path, e));
                },
            }
        }
        match insert_volume_data(conn, &self.selected_project.as_ref().unwrap().clone(), &volumes) {
            Ok(row_count) => {
                self.log_messages
                    .push(format!("Successfully inserted {} rows into DB.", row_count));
            },
            Err(e) => {
                self.log_messages.push(format!("Failed to insert cycle data to db.Error {}", e));
            },
        }
        self.log_messages.push("Uploading meteo data...".to_string());
    }

    fn open_file_dialog(&mut self) {
        let mut dialog = FileDialog::open_file(self.initial_path.clone())
            .open_button_text(Cow::from("Upload"))
            .multi_select(true)
            .show_rename(false)
            .show_new_folder(false);

        dialog.open();
        self.open_file_dialog = Some(dialog);
    }

    /// Handles the file selection process
    fn handle_file_selection(&mut self, ctx: &Context) {
        if let Some(dialog) = &mut self.open_file_dialog {
            dialog.show(ctx);

            match dialog.state() {
                egui_file::State::Selected => {
                    let selected_paths: Vec<PathBuf> =
                        dialog.selection().into_iter().map(|p: &Path| p.to_path_buf()).collect();

                    if !selected_paths.is_empty() {
                        self.opened_files = Some(selected_paths.clone()); //   Store files properly
                        self.log_messages.push(format!("Selected files: {:?}", selected_paths));
                        self.process_files(selected_paths);
                    }

                    self.open_file_dialog = None; //   Close the dialog
                },
                egui_file::State::Cancelled | egui_file::State::Closed => {
                    self.log_messages.push("File selection cancelled.".to_string());
                    self.open_file_dialog = None;
                },
                _ => {}, // Do nothing if still open
            }
        }
    }

    fn process_gas_files(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
        self.log_messages.push("Uploading gas data...".to_owned());
        let mut all_gas = GasData::new();
        for path in &selected_paths {
            let instrument = Li7810::default(); // Assuming you have a default instrument
            match instrument.read_data_file(path) {
                Ok(data) => {
                    if data.validate_lengths() && !data.any_col_invalid() {
                        let rows = data.diag.len();
                        all_gas.datetime.extend(data.datetime);
                        all_gas.diag.extend(data.diag);
                        all_gas.instrument_model = data.instrument_model;
                        all_gas.instrument_serial = data.instrument_serial;

                        // Merge gas values correctly
                        for (gas_type, values) in data.gas {
                            all_gas.gas.entry(gas_type).or_insert_with(Vec::new).extend(values);
                        }
                        self.log_messages
                            .push(format!("Succesfully read file {:?} with {} rows.", path, rows));
                    }
                },
                Err(e) => {
                    self.log_messages.push(format!("Failed to read file {:?}: {}", path, e));
                },
            }
        }
        match insert_measurements(conn, &all_gas, self.selected_project.as_ref().unwrap().clone()) {
            Ok(row_count) => {
                self.log_messages
                    .push(format!("Successfully inserted {} rows into DB.", row_count));
            },
            Err(_) => {
                self.log_messages.push("Failed to insert gas data to db.".to_owned());
            },
        }
    }
    fn process_files(&mut self, selected_paths: Vec<PathBuf>) {
        match Connection::open("fluxrs.db") {
            Ok(mut conn) => {
                if let Some(data_type) = self.selected_data_type.as_ref() {
                    match data_type {
                        DataType::Gas => self.process_gas_files(selected_paths, &mut conn),
                        DataType::Cycle => self.upload_cycle_data(selected_paths, &mut conn),
                        DataType::Meteo => self.upload_meteo_data(selected_paths, &mut conn),
                        DataType::Volume => self.upload_volume_data(selected_paths, &mut conn),
                    }
                }
            },
            Err(e) => {
                self.log_messages.push(format!("❌ Failed to connect to database: {}", e));
            },
        }
    }

    fn load_projects_from_db(&mut self) -> Result<()> {
        let mut conn = Connection::open("fluxrs.db")?;

        let mut stmt = conn.prepare("SELECT project_id FROM projects")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        self.projects = rows.collect::<Result<Vec<String>, _>>()?;

        let result: Result<(String, String, String), _> = conn.query_row(
            "SELECT project_id, instrument_serial, main_gas FROM projects WHERE current = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        match result {
            Ok((project_id, instrument_serial, main_gas)) => {
                self.selected_project = Some(project_id);
                self.instrument_serial = instrument_serial;
                self.main_gas = GasType::from_str(&main_gas);
            },
            Err(_) => {
                self.selected_project = None;
                self.instrument_serial = "".to_owned();
            },
        }

        Ok(())
    }
    fn save_project_to_db(&mut self) -> Result<()> {
        let mut conn = Connection::open("fluxrs.db")?;

        let main_gas = self.main_gas.map_or("Unknown".to_string(), |g| g.to_string());
        let instrument_model = self.instrument.to_string();

        let tx = conn.transaction()?; //   Use transaction for consistency

        tx.execute("UPDATE projects SET current = 0 WHERE current = 1", [])?;

        tx.execute(
            "INSERT OR REPLACE INTO projects (project_id, main_gas, instrument_model, instrument_serial, current)
             VALUES (?1, ?2, ?3, ?4, 1)",
            [&self.project_name, &main_gas, &instrument_model, &self.proj_serial],
        )?;

        tx.commit()?; //   Commit the transaction

        println!(
            "Project set as current: {}, {}, {}",
            self.project_name, main_gas, instrument_model
        );

        self.load_projects_from_db()?;

        Ok(())
    }

    fn set_current_project(&mut self, project_name: &str) -> Result<()> {
        let mut conn = Connection::open("fluxrs.db")?;
        let tx = conn.transaction()?;

        tx.execute("UPDATE projects SET current = 0 WHERE current = 1", [])?;

        tx.execute("UPDATE projects SET current = 1 WHERE project_id = ?1", [project_name])?;

        tx.commit()?;
        println!("Current project set: {}", project_name);

        self.selected_project = Some(project_name.to_string());

        Ok(())
    }

    pub fn proj_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Project Management");

        ui.separator();
        ui.heading("Change current project");
        ui.add_space(10.);
        if !self.initiated {
            self.load_projects_from_db().unwrap();
        }

        if !self.projects.is_empty() {
            egui::ComboBox::from_label("Current project")
                .selected_text(
                    self.selected_project.clone().unwrap_or_else(|| "Select Project".to_string()),
                )
                .show_ui(ui, |ui| {
                    for project in &self.projects.clone() {
                        if ui
                            .selectable_label(
                                Some(project) == self.selected_project.as_ref(),
                                project,
                            )
                            .clicked()
                        {
                            if let Err(err) = self.set_current_project(project) {
                                eprintln!("Failed to set project as current: {}", err);
                            }
                        }
                    }
                });
        } else {
            ui.label("No projects found.");
        }
        ui.separator();
        ui.collapsing("Instructions", |ui| {
            ui.label("Project name:");
            ui.label("Instrument");
            ui.label("Main gas");
        });

        ui.heading("New project");
        ui.label("Project name");
        ui.text_edit_singleline(&mut self.project_name);

        ui.label("Select instrument");
        egui::ComboBox::from_label("Instrument")
            .selected_text(self.instrument.to_string())
            .show_ui(ui, |ui| {
                for instrument in InstrumentType::available_instruments() {
                    ui.selectable_value(&mut self.instrument, instrument, instrument.to_string());
                }
            });

        ui.label("Instrument serial");
        ui.text_edit_singleline(&mut self.proj_serial);

        let available_gases = self.instrument.available_gases();
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

        ui.add_space(10.);

        if ui.button("Add Project").clicked() {
            if let Err(err) = self.save_project_to_db() {
                eprintln!("Failed to save project: {}", err);
            }
        }
    }
}

#[derive(Default, PartialEq)]
struct EmptyPanel {}

impl EmptyPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui) {}
}

pub fn is_inside_polygon(
    point: egui_plot::PlotPoint,
    start_x: f64,
    end_x: f64,
    min_y: f64,
    max_y: f64,
) -> bool {
    point.x >= start_x && point.x <= end_x && point.y >= min_y && point.y <= max_y
}
pub fn limit_to_bounds(plot_ui: &mut PlotUi, app: &mut ValidationApp, gas_type: &GasType) {
    // app.min_calc_area_range = 240.;
    let calc_area_range = app.get_calc_end(*gas_type) - app.get_calc_start(*gas_type);
    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
    let at_min_area = calc_area_range as i64 == app.min_calc_area_range as i64;
    // let after_close = app.cycles[app.index].calc_range_start.get(&gas_type).unwrap() >= app.close_idx;
    // let before_open = app.cycles[app.index].calc_range_end.get(&gas_type).unwrap() <= app.open_idx;
    // let in_bounds = after_close && before_open;
    // let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
    let at_start = app.get_calc_start(*gas_type) <= app.close_idx;
    let at_end = app.get_calc_end(*gas_type) >= app.open_idx;
    let positive_drag = drag_delta.x > 0.;
    let negative_drag = drag_delta.x < 0.;

    let range_len = app.get_calc_end(*gas_type) - app.get_calc_start(*gas_type);
    if at_start && positive_drag && !at_min_area {
        app.increment_calc_start(*gas_type, drag_delta.x as f64);
        return;
    }

    if at_end && negative_drag && !at_min_area {
        app.increment_calc_end(*gas_type, drag_delta.x as f64);
        return;
    }

    if app.get_calc_start(*gas_type) < app.close_idx {
        let diff = (app.get_calc_start(*gas_type) - app.close_idx).abs();
        app.set_calc_start(*gas_type, app.close_idx);
        if app.get_calc_end(*gas_type) < app.open_idx {
            app.increment_calc_end(*gas_type, diff);
        }
        return;
    }
    if app.get_calc_end(*gas_type) > app.open_idx {
        let diff = (app.cycles[*app.index].calc_range_end.get(gas_type).unwrap_or(&0.0)
            - app.open_idx)
            .abs();

        app.set_calc_end(*gas_type, app.open_idx);
        if app.get_calc_start(*gas_type) > app.close_idx {
            app.decrement_calc_start(*gas_type, diff);
        }
    }
}
pub fn handle_drag_polygon(
    plot_ui: &mut PlotUi,
    app: &mut ValidationApp,
    is_left: bool,
    gas_type: &GasType,
) {
    let delta = plot_ui.pointer_coordinate_drag_delta();
    let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
    let calc_area_range = app.get_calc_end(*gas_type) - app.get_calc_start(*gas_type);

    if is_left && app.get_calc_start(*gas_type) > app.close_idx && dragged {
        // do nothing if at min length and trying to make it smaller
        if calc_area_range <= app.min_calc_area_range && delta.x > 0. {
            let diff = app.min_calc_area_range - calc_area_range;
            app.decrement_calc_start(*gas_type, diff);
            return;
        }
        app.increment_calc_start(*gas_type, delta.x as f64);
    } else if !is_left && app.get_calc_end(*gas_type) < app.open_idx && dragged {
        // do nothing if at min length and trying to make it smaller
        if calc_area_range < app.min_calc_area_range && delta.x < 0. {
            let diff = app.min_calc_area_range - calc_area_range;
            app.increment_calc_end(*gas_type, diff);
            return;
        }
        app.increment_calc_end(*gas_type, delta.x as f64);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_polygon(
    start_x: f64,
    end_x: f64,
    min_y: f64,
    max_y: f64,
    color: Color32,
    stroke: Color32,
    id: &str,
    idd: Id,
) -> Polygon {
    Polygon::new(PlotPoints::from(vec![
        [start_x, min_y],
        [start_x, max_y],
        [end_x, max_y],
        [end_x, min_y],
        [start_x, min_y], // Close the polygon
    ]))
    .name(id)
    .fill_color(color)
    .stroke(Stroke::new(2.0, stroke))
    .allow_hover(true)
}

// TableApp struct
#[derive(Default)]
pub struct TableApp {
    table_names: Vec<String>,
    selected_table: Option<String>,
    column_names: Vec<String>,
    data: Vec<Vec<String>>,
    current_page: usize,
}

impl TableApp {
    pub fn new(db_path: &str) -> Self {
        let conn = Connection::open(db_path).expect("Failed to open database");
        // let table_names = Self::fetch_table_names(&conn).unwrap_or_default();
        let table_names = Vec::new();
        let current_page = 0;

        Self {
            table_names,
            current_page,
            selected_table: None,
            column_names: Vec::new(),
            data: Vec::new(),
        }
    }

    fn fetch_table_names(&mut self, conn: &Connection) {
        let mut stmt = match conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
            Ok(stmt) => stmt,
            Err(err) => {
                eprintln!("Error preparing statement: {}", err);
                self.table_names.clear();
                return;
            },
        };

        let tables = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .and_then(|rows| rows.collect::<Result<Vec<String>, _>>());

        match tables {
            Ok(names) => self.table_names = names,
            Err(err) => {
                eprintln!("Error fetching table names: {}", err);
                self.table_names.clear();
            },
        }
    }
    fn fetch_table_data(&mut self, table_name: &str) {
        self.column_names.clear();
        self.data.clear();
        self.current_page = 0; // Reset page when switching tables

        let conn = Connection::open("fluxrs.db").expect("Failed to open database");

        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name)).unwrap();
        self.column_names = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<String>, _>>()
            .unwrap_or_default();

        let mut stmt = conn.prepare(&format!("SELECT * FROM {}", table_name)).unwrap();
        let column_count = stmt.column_count();

        let rows = stmt.query_map([], |row: &Row| {
            let mut values = Vec::new();
            for i in 0..column_count {
                let value = match row.get_ref(i) {
                    Ok(ValueRef::Null) => "NULL".to_string(),
                    Ok(ValueRef::Integer(i)) => i.to_string(),
                    Ok(ValueRef::Real(f)) => f.to_string(),
                    Ok(ValueRef::Text(s)) => String::from_utf8_lossy(s).to_string(),
                    Ok(ValueRef::Blob(_)) => "[BLOB]".to_string(), // Handle BLOBs gracefully
                    Err(_) => "[ERROR]".to_string(),               //   Handle row errors explicitly
                };
                values.push(value);
            }
            Ok(values)
        });

        self.data = rows.unwrap().filter_map(|res| res.ok()).collect(); //   Collect valid rows only
    }
    pub fn table_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.heading("Database Table Viewer");
        if self.table_names.is_empty() {
            let conn = Connection::open("fluxrs.db").expect("Failed to open database");
            self.fetch_table_names(&conn);
        }

        if !self.table_names.is_empty() {
            egui::ComboBox::from_label("Select a table")
                .selected_text(
                    self.selected_table.clone().unwrap_or_else(|| "Choose a table".to_string()),
                )
                .show_ui(ui, |ui| {
                    for table in &self.table_names.clone() {
                        if ui
                            .selectable_label(self.selected_table.as_deref() == Some(table), table)
                            .clicked()
                        {
                            self.selected_table = Some(table.clone());
                            self.fetch_table_data(table);
                        }
                    }
                });
        } else {
            ui.label("No tables found in the database.");
        }

        ui.separator();

        if let Some(_selected) = &self.selected_table {
            // Determine which rows to display for pagination
            let rows_per_page = 100;
            let start_idx = self.current_page * rows_per_page;
            let end_idx = (start_idx + rows_per_page).min(self.data.len());
            ui.horizontal(|ui| {
                // Previous Page Button
                if self.current_page > 0 && ui.button("⬅ Previous").clicked() {
                    self.current_page -= 1;
                }

                ui.label(format!(
                    "Page {}/{}",
                    self.current_page + 1,
                    self.data.len().div_ceil(rows_per_page)
                ));

                // Next Page Button
                if end_idx < self.data.len() && ui.button("Next ➡").clicked() {
                    self.current_page += 1;
                }
            });
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("data_table").striped(true).show(ui, |ui| {
                    for col in &self.column_names {
                        ui.label(col); // show headers as-is
                    }
                    ui.end_row();
                    for row in &self.data[start_idx..end_idx] {
                        for (i, value) in row.iter().enumerate() {
                            let col_name = &self.column_names[i];
                            let display = if col_name == "datetime" || col_name == "start_time" {
                                if let Ok(ts) = value.parse::<i64>() {
                                    if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
                                        dt.format("%Y-%m-%d %H:%M:%S").to_string()
                                    } else {
                                        format!("Invalid timestamp: {}", ts)
                                    }
                                } else {
                                    format!("Invalid value: {}", value)
                                }
                            } else {
                                value.to_string()
                            };

                            ui.label(display);
                        }
                        ui.end_row();
                    }
                });
            });
        }
    }
}

async fn run_processing(
    times: TimeData,
    gas_data: HashMap<String, GasData>,
    meteo_data: MeteoData,
    volume_data: VolumeData,
    project: String,
    conn: Arc<Mutex<rusqlite::Connection>>,
) {
    if times.start_time.is_empty() || gas_data.is_empty() {
        println!("Empty data — skipping");
        return;
    }

    let gas_data_arc = Arc::new(gas_data);
    let all_dates: Vec<String> = times
        .start_time
        .iter()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .collect::<HashSet<_>>() // remove duplicates
        .into_iter()
        .collect();

    let num_chunks = 10;
    let date_chunks = chunk_dates(all_dates, num_chunks);

    let mut tasks = Vec::new();

    for date_group in date_chunks {
        let dates_set: HashSet<String> = date_group.iter().cloned().collect();
        let filtered_times = filter_time_data_by_dates(&times, &dates_set);
        let meteo_clone = meteo_data.clone();
        let volume_clone = volume_data.clone();
        let project_clone = project.clone();

        let mut gas_data_for_thread = HashMap::new();
        for date in &date_group {
            if let Some(day_data) = gas_data_arc.get(date) {
                gas_data_for_thread.insert(date.clone(), day_data.clone());
            }
        }

        let task = tokio::task::spawn_blocking(move || {
            process_cycles(
                &filtered_times,
                &gas_data_for_thread,
                &meteo_clone,
                &volume_clone,
                project_clone,
            )
        });

        tasks.push(task);
    }

    let results = futures::future::join_all(tasks).await;

    let mut all_cycles: Vec<Cycle> = Vec::new();

    for result in results {
        match result {
            Ok(Ok(mut cycles)) => all_cycles.append(&mut cycles),
            Ok(Err(e)) => eprintln!("Error processing cycles: {}", e),
            Err(e) => eprintln!("Thread join error: {}", e),
        }
    }

    if all_cycles.is_empty() {
        println!("NO CYCLES WITH DATA FOUND");
    } else {
        match insert_fluxes_ignore_duplicates(&mut conn.lock().unwrap(), &all_cycles, project) {
            Ok(_) => println!("Fluxes inserted successfully!"),
            Err(e) => eprintln!("Error inserting fluxes: {}", e),
        }
    }
}

fn chunk_dates(dates: Vec<String>, num_chunks: usize) -> Vec<Vec<String>> {
    let mut chunks = vec![vec![]; num_chunks];
    for (i, date) in dates.into_iter().enumerate() {
        chunks[i % num_chunks].push(date);
    }
    chunks
}

fn filter_time_data_by_dates(times: &TimeData, dates: &HashSet<String>) -> TimeData {
    let mut indices = Vec::new();

    for (i, dt) in times.start_time.iter().enumerate() {
        if dates.contains(&dt.format("%Y-%m-%d").to_string()) {
            indices.push(i);
        }
    }

    TimeData {
        chamber_id: indices.iter().map(|&i| times.chamber_id[i].clone()).collect(),
        start_time: indices.iter().map(|&i| times.start_time[i]).collect(),
        close_offset: indices.iter().map(|&i| times.close_offset[i]).collect(),
        open_offset: indices.iter().map(|&i| times.open_offset[i]).collect(),
        end_offset: indices.iter().map(|&i| times.end_offset[i]).collect(),
        project: indices.iter().map(|&i| times.project[i].clone()).collect(),
    }
}
