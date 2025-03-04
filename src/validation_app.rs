use crate::app_plotting::{create_gas_plot, init_flux_plot, init_lag_plot};
use crate::csv_parse;
use crate::index::Index;
use crate::instruments::{GasType, Li7810};
use crate::query_cycles;
use crate::query_gas;
use crate::structs::{Cycle, EqualLen, GasData, TimeData};
use crate::{insert_cycles, insert_measurements, process_cycles};
use chrono::{DateTime, NaiveDateTime, Utc};
use eframe::egui::{
    Button, Color32, Context, Id, PointerButton, Pos2, Rect, RichText, Sense, Stroke, Ui,
    WidgetInfo, WidgetText, WidgetType,
};
use egui_file::FileDialog;
use egui_plot::{PlotPoints, PlotUi, Polygon};
use rusqlite::{params, Connection, Result};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
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
    FileInit,
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
    empty_panel: EmptyPanel,
    index: usize,
}
impl MainApp {
    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            // We give the ui a label so we can easily enumerate all demos in the tests
            // The actual accessibility benefit is questionable considering the plot itself isn't
            // accessible at all
            let container_response = ui.response();
            container_response
                .widget_info(|| WidgetInfo::labeled(WidgetType::RadioGroup, true, "Select Demo"));

            // TODO(lucasmerlin): The parent ui should ideally be automatically set as AccessKit parent
            // or at least, with an opt in via UiBuilder, making this much more readable
            // See https://github.com/emilk/egui/issues/5674
            ui.ctx()
                .clone()
                .with_accessibility_parent(container_response.id, || {
                    ui.selectable_value(
                        &mut self.current_panel,
                        Panel::Validation,
                        "Validate measurements",
                    );
                    ui.selectable_value(
                        &mut self.current_panel,
                        Panel::DataInit,
                        "Initiate measurements",
                    );
                    ui.selectable_value(
                        &mut self.current_panel,
                        Panel::FileInit,
                        "Upload files to db",
                    );
                    ui.selectable_value(&mut self.current_panel, Panel::Empty, "Empty panel");
                });
        });
        ui.separator();

        match self.current_panel {
            Panel::Validation => {
                self.validation_panel.ui(ui, ctx);
            }
            Panel::DataInit => {
                self.validation_panel.init_ui(ui, ctx);
            }
            Panel::FileInit => {
                self.validation_panel.file_ui(ui, ctx);
            }
            Panel::Empty => {
                self.empty_panel.ui(ui);
            }
        }
    }
}

// #[derive(Default)]
pub struct ValidationApp {
    pub r_lim: f32,
    pub enabled_gases: HashSet<GasType>, // Stores which gases are enabled for plotting
    pub enabled_fluxes: HashSet<GasType>, // Stores which gases are enabled for plotting
    pub cycles: Vec<Cycle>,
    pub gases: Vec<GasType>,
    pub lag_plot: Vec<[f64; 2]>, // Add a vecxy tor of values to your struct
    pub lag_idx: f64,            // Add a vecxy tor of values to your struct
    pub close_idx: f64,
    pub open_offset: f64,
    pub close_offset: f64,
    pub open_idx: f64,
    pub start_time_idx: f64,
    pub end_time_idx: f64,
    pub calc_range_start: HashMap<GasType, f64>,
    pub calc_range_end: HashMap<GasType, f64>,
    pub max_y: HashMap<GasType, f64>,
    pub min_y: HashMap<GasType, f64>,
    pub drag_panel_width: f64,
    pub calc_area_color: Color32,
    pub calc_area_adjust_color: Color32,
    pub calc_area_stroke_color: Color32,
    pub min_calc_area_range: f64,
    pub index: Index,
    pub lag_vec: Vec<f64>,
    pub start_vec: Vec<f64>,
    pub selected_point: Option<[f64; 2]>,
    pub dragged_point: Option<[f64; 2]>,
    pub chamber_colors: HashMap<String, Color32>, // Stores colors per chamber
    pub visible_traces: HashMap<String, bool>,    // Stores colors per chamber
    pub start_date: String,
    pub end_date: String,
    // pub dialog: FileDialog,
    pub opened_files: Option<Vec<PathBuf>>,
    pub open_file_dialog: Option<FileDialog>,
    pub initial_path: Option<PathBuf>,
    pub selected_data_type: Option<DataType>,
    pub log_messages: Vec<String>,
    pub should_upload: bool,
}

impl Default for ValidationApp {
    fn default() -> Self {
        Self {
            r_lim: 0.0,
            enabled_gases: HashSet::new(),
            enabled_fluxes: HashSet::new(),
            cycles: Vec::new(),
            gases: Vec::new(),
            lag_plot: Vec::new(),
            lag_idx: 0.0,
            close_idx: 0.0,
            open_offset: 0.0,
            close_offset: 0.0,
            open_idx: 0.0,
            start_time_idx: 0.0,
            end_time_idx: 0.0,
            calc_range_start: HashMap::new(),
            calc_range_end: HashMap::new(),
            max_y: HashMap::new(),
            min_y: HashMap::new(),
            drag_panel_width: 200.0, // Default width for UI panel
            calc_area_color: Color32::from_gray(100), // Default gray color
            calc_area_adjust_color: Color32::from_gray(150),
            calc_area_stroke_color: Color32::from_gray(200),
            min_calc_area_range: 0.0,
            index: Index::default(), // Assuming Index implements Default
            lag_vec: Vec::new(),
            start_vec: Vec::new(),
            selected_point: None,
            dragged_point: None,
            chamber_colors: HashMap::new(),
            visible_traces: HashMap::new(),
            start_date: String::new(),
            end_date: String::new(),
            opened_files: None,
            open_file_dialog: None,
            initial_path: Some(PathBuf::from("/home/eerokos/code/rust/fluxrs/")),
            selected_data_type: None,
            log_messages: Vec::new(),
            should_upload: false,
        }
    }
}
impl ValidationApp {
    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.cycles.len() == 0 {
            println!("No cycles");
            return;
        }
        ui.heading("Plot selection");

        ui.horizontal(|ui| {
            ui.label("main gas plots");
            for &gas in &self.gases {
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
        ui.horizontal(|ui| {
            ui.label("Flux plots");
            for &gas in &self.gases {
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
        ui.separator();
        let ch_id = format!("Chamber: {}", self.cycles[*self.index].chamber_id.clone());
        ui.label(ch_id);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                for gas in &self.enabled_gases {
                    let r_val = match self.cycles[*self.index].calc_r.get(gas) {
                        Some(r) => format!("calc_r {} : {:.6}", gas, r),
                        None => "flux: N/A".to_string(), // Handle missing data
                    };
                    ui.label(r_val);
                }
            });

            // NOTE: BAD CLONE
            ui.vertical(|ui| {
                for gas in &self.enabled_gases {
                    let flux = match self.cycles[*self.index].flux.get(gas) {
                        Some(r) => format!("flux {} : {:.6}", gas, r),
                        None => "flux: N/A".to_string(), // Handle missing data
                    };
                    ui.label(flux);
                }
            });
        });
        let main_gas = self.cycles[*self.index].main_gas;
        let measurement_r = match self.cycles[*self.index].measurement_r.get(&main_gas) {
            Some(r) => format!("measurement r: {:.6}", r),
            None => "calc r: N/A".to_string(), // Handle missing data
        };
        // );
        ui.label(measurement_r);
        // let flux = format!("flux: {:.6}", self.cycles[*self.index].flux);

        let datetime = format!("datetime: {}", self.cycles[*self.index].start_time);
        ui.label(datetime);

        // egui::SidePanel::left("my_left_panel").show(ctx, |ui| {});

        let main_gas = self.cycles[*self.index].main_gas;
        ui.style_mut().text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(14.0, eframe::epaint::FontFamily::Proportional),
        );

        // let gas_plot = self.create_gas_plot();
        // let lag_plot = self.create_lag_plot();

        let mut prev_clicked = false;
        let mut next_clicked = false;
        let mut highest_r = false;
        let mut find_lag = false;
        let mut find_bad = false;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            prev_clicked = ui.add(egui::Button::new("Prev measurement")).clicked();
            next_clicked = ui.add(egui::Button::new("Next measurement")).clicked();
        });

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            highest_r = ui.add(egui::Button::new("Find r")).clicked();
            find_lag = ui.add(egui::Button::new("Find lag")).clicked();
            find_bad = ui.add(egui::Button::new("Find bad")).clicked();
        });

        ui.add_space(10.);

        if find_bad {
            self.find_bad_measurement(main_gas);
        }

        if find_lag {
            self.cycles[*self.index].reset();
            self.update_cycle();
        }

        if highest_r {
            self.cycles[*self.index].recalc_r();
            self.update_cycle();
        }

        // if prev_clicked {
        //     if *self.index == 0 {
        //         self.index.set(self.cycles.len());
        //     }
        //     self.index.decrement();
        //     self.update_cycle();
        // }

        if prev_clicked {
            let mut new_index = *self.index;

            // Loop until a visible trace is found, or we cycle back to the starting index
            let mut attempts = 0;
            while attempts < self.cycles.len() {
                // Decrement index cyclically
                if new_index == 0 {
                    new_index = self.cycles.len() - 1; // Wrap around to the last index
                } else {
                    new_index -= 1;
                }

                let chamber_id = &self.cycles[new_index].chamber_id;
                if self.visible_traces.get(chamber_id).copied().unwrap_or(true) {
                    self.index.set(new_index);
                    self.update_cycle();
                    break;
                }
                attempts += 1;
            }
        }
        if next_clicked {
            let mut new_index = *self.index;

            // Loop until a visible trace is found, or we cycle back to the starting index
            let mut attempts = 0;
            while attempts < self.cycles.len() {
                new_index = (new_index + 1) % self.cycles.len(); // Increment index cyclically

                let chamber_id = &self.cycles[new_index].chamber_id;
                if self.visible_traces.get(chamber_id).copied().unwrap_or(true) {
                    self.index.set(new_index);
                    self.update_cycle();
                    break;
                }
                attempts += 1;
            }
        }
        // if next_clicked && self.index + 1 < self.cycles.len() {
        // if next_clicked {
        //     self.index.increment();
        //     if *self.index >= self.cycles.len() {
        //         self.index.set(0)
        //     }
        //     self.update_cycle();
        // }

        let mut lag_s = self.cycles[*self.index].lag_s;

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

        let close_line_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
        let left_id = Id::new("left_test");
        let main_id = Id::new("main_area");
        let right_id = Id::new("right_area");

        ui.horizontal(|ui| {
            for gas_type in self.enabled_gases.clone() {
                if self.is_gas_enabled(&gas_type) {
                    // let x_range = (self.end_Lime_idx - self.start_time_idx) * 0.05;
                    // let y_range =
                    //     (self.get_max_y(&gas_type) - self.get_min_y(&gas_type)) * 0.05;
                    // let x_min = self.start_time_idx - x_range;
                    // let x_max = self.end_time_idx + x_range;
                    // let y_min = self.get_min_y(&gas_type) - y_range;
                    // let y_max = self.get_max_y(&gas_type) + y_range;
                    let gas_plot =
                        create_gas_plot(&gas_type, self.start_time_idx, self.end_time_idx);
                    // .include_x(x_min)
                    // .include_x(x_max)
                    // .include_y(y_min)
                    // .include_y(y_max);
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
                        ui.ctx().set_cursor_icon(egui::CursorIcon::None); // Hide cursor
                                                                          // println!("Gas plot is hovered!");
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            // let gas_type = GasType::CH4;
            self.render_legend(ui, &self.chamber_colors.clone());
            for gas in self.enabled_fluxes.clone() {
                let flux_plot = init_flux_plot(&gas);
                // ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                let response = flux_plot.show(ui, |plot_ui| {
                    self.render_flux_plot(plot_ui, gas);
                });
                if response.response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None); // Hide cursor
                                                                      // println!("Gas plot is hovered!");
                }
            }
        });
        ui.horizontal(|ui| {
            let lag_plot = init_lag_plot(&main_gas);
            let response = lag_plot.show(ui, |plot_ui| {
                self.render_lag_plot(plot_ui);
            });
            if response.response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None); // Hide cursor
                                                                  // println!("Gas plot is hovered!");
            }
        });
    }

    fn parse_dates(&mut self) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
        let naive_start = match NaiveDateTime::parse_from_str(&self.start_date, "%Y-%m-%dT%H:%M:%S")
        {
            Ok(dt) => dt,
            Err(e) => {
                return Err(format!(
                    "Failed to parse start date: {}, {}\nUse format YYYY-MM-DDTHH:MM:SS",
                    self.start_date, e
                ));
            }
        };

        let naive_end = match NaiveDateTime::parse_from_str(&self.end_date, "%Y-%m-%dT%H:%M:%S") {
            Ok(dt) => dt,
            Err(e) => {
                return Err(format!(
                    "Failed to parse end date: {}, {}\nUse format YYYY-MM-DDTHH:MM:SS",
                    self.end_date, e
                ));
            }
        };

        let start = DateTime::<Utc>::from_utc(naive_start, Utc);
        let end = DateTime::<Utc>::from_utc(naive_end, Utc);

        Ok((start, end))
    }
    pub fn log_display(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label("**Log Messages:**");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for message in &self.log_messages {
                ui.label(message);
            }
        });
    }
    pub fn init_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.text_edit_singleline(&mut self.start_date);
        ui.text_edit_singleline(&mut self.end_date);
        if ui.button("Use range").clicked() {
            let (start, end) = match self.parse_dates() {
                Ok(dates) => dates,
                Err(e) => {
                    self.log_messages.push(e); // Log error in UI
                    return;
                }
            };

            let conn = match Connection::open("fluxrs.db") {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to open database: {}", e);
                    return; // Exit early if connection fails
                }
            };

            match (
                query_cycles(&conn, start, end),
                query_gas(&conn, start, end),
            ) {
                (Ok(times), Ok(gas_data)) => {
                    if times.start_time.is_empty() || gas_data.is_empty() {
                        self.cycles = Vec::new();
                    } else {
                        match process_cycles(&times, &gas_data) {
                            Ok(cycle_vec) => {
                                println!("Successfully processed cycles!");
                                println!("Start Date: {}", start);
                                println!("End Date: {}", end);
                                self.cycles = cycle_vec;
                                self.update_cycle();
                            }
                            Err(e) => eprintln!("Error processing cycles: {}", e),
                        }
                    }
                }
                _ => eprintln!("Failed to query database."),
            }
        }
        self.log_display(ui);
        // ui.separator();
        // ui.label("**Log Messages:**");
        // egui::ScrollArea::vertical().show(ui, |ui| {
        //     for message in &self.log_messages {
        //         ui.label(message);
        //     }
        // });
    }
    pub fn file_ui(&mut self, ui: &mut Ui, ctx: &Context) {
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
                // self.open_file_dialog();
            }
            if ui.button("Select Volume Files").clicked() {
                self.selected_data_type = Some(DataType::Volume);
                // self.open_file_dialog();
            }
        });

        // Handle file selection
        self.handle_file_selection(ctx);

        self.log_display(ui);
        if ui.button("Clear Log").clicked() {
            self.log_messages.clear();
        }
    }

    // fn upload_cycle_data(&mut self) {
    fn upload_cycle_data(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
        self.log_messages
            .push("Uploading cycle data...".to_string());

        let mut all_times = TimeData::new();

        for path in &selected_paths {
            match csv_parse::read_time_csv(path) {
                // ✅ Pass `path` directly
                Ok(res) => {
                    if res.validate_lengths() {
                        all_times.chamber_id.extend(res.chamber_id);
                        all_times.start_time.extend(res.start_time);
                        all_times.close_offset.extend(res.close_offset);
                        all_times.open_offset.extend(res.open_offset);
                        all_times.end_offset.extend(res.end_offset);

                        self.log_messages
                            .push(format!("Successfully read {:?}", path));
                    } else {
                        self.log_messages
                            .push(format!("Skipped file {:?}: Invalid data length", path));
                    }
                }
                Err(e) => {
                    self.log_messages
                        .push(format!("Failed to read file {:?}: {}", path, e));
                }
            }
        }
        match insert_cycles(conn, &all_times) {
            Ok(row_count) => {
                self.log_messages
                    .push(format!("Successfully inserted {} rows into DB.", row_count));
            }
            Err(_) => {
                self.log_messages
                    .push("Failed to insert cycle data to db.".to_owned());
            }
        }
    }
    fn upload_meteo_data(&mut self) {
        self.log_messages
            .push("Uploading meteo data...".to_string());
        for path in &self.opened_files {
            self.log_messages
                .push(format!("Successfully uploaded {:?}", path));
        }
    }

    fn upload_volume_data(&mut self) {
        self.log_messages
            .push("Uploading volume data...".to_string());
        for path in &self.opened_files {
            self.log_messages
                .push(format!("Successfully uploaded {:?}", path));
        }
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
                    let selected_paths: Vec<PathBuf> = dialog
                        .selection()
                        .into_iter()
                        .map(|p: &Path| p.to_path_buf())
                        .collect();

                    if !selected_paths.is_empty() {
                        self.opened_files = Some(selected_paths.clone()); // ✅ Store files properly
                        self.log_messages
                            .push(format!("Selected files: {:?}", selected_paths));
                        self.process_files(selected_paths);
                    }

                    self.open_file_dialog = None; // ✅ Close the dialog
                }
                egui_file::State::Cancelled | egui_file::State::Closed => {
                    self.log_messages
                        .push("File selection cancelled.".to_string());
                    self.open_file_dialog = None;
                }
                _ => {} // Do nothing if still open
            }
        }
    }

    fn process_gas_files(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
        self.log_messages.push("Uploading gas data...".to_owned());
        let mut all_gas = GasData::new();
        for path in &selected_paths {
            // self.log_messages
            //     .push(format!("📂 Reading file: {:?}", path));

            let instrument = Li7810::default(); // Assuming you have a default instrument
            match instrument.read_data_file(path) {
                Ok(data) => {
                    if data.validate_lengths() && !data.any_col_invalid() {
                        let rows = data.diag.len();
                        all_gas.datetime.extend(data.datetime);
                        all_gas.diag.extend(data.diag);

                        // Merge gas values correctly
                        for (gas_type, values) in data.gas {
                            all_gas
                                .gas
                                .entry(gas_type)
                                .or_insert_with(Vec::new)
                                .extend(values);
                        }
                        self.log_messages.push(format!(
                            "Succesfully read file {:?} with {} rows.",
                            path, rows
                        ));
                    }
                }
                Err(e) => {
                    self.log_messages
                        .push(format!("Failed to read file {:?}: {}", path, e));
                }
            }
        }
        match insert_measurements(conn, &all_gas) {
            Ok(row_count) => {
                self.log_messages
                    .push(format!("Successfully inserted {} rows into DB.", row_count));
            }
            Err(_) => {
                self.log_messages
                    .push("Failed to insert gas data to db.".to_owned());
            }
        }
    }
    fn process_files(&mut self, selected_paths: Vec<PathBuf>) {
        match Connection::open("fluxrs.db") {
            Ok(mut conn) => {
                if let Some(data_type) = self.selected_data_type.as_ref() {
                    match data_type {
                        DataType::Gas => self.process_gas_files(selected_paths, &mut conn),
                        DataType::Cycle => self.upload_cycle_data(selected_paths, &mut conn),
                        DataType::Meteo => self.upload_meteo_data(),
                        DataType::Volume => self.upload_volume_data(),
                    }
                }
            }
            Err(e) => {
                self.log_messages
                    .push(format!("❌ Failed to connect to database: {}", e));
            }
        }
    }
}

#[derive(Default, PartialEq)]
struct EmptyPanel {}

impl EmptyPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui) {}
}
#[derive(Default)]
struct InitApp {
    parent: ValidationApp,
}

impl InitApp {
    pub fn init_ui(&mut self, ui: &mut egui::Ui, parent: ValidationApp) {
        ui.text_edit_singleline(&mut self.parent.start_date);
        ui.text_edit_singleline(&mut self.parent.end_date);
        if ui.button("Use range").clicked() {
            let naive_start =
                NaiveDateTime::parse_from_str(&self.parent.start_date, "%Y-%m-%dT%H:%M:%S")
                    .unwrap();
            let start = DateTime::<Utc>::from_utc(naive_start, Utc);
            let naive_end =
                NaiveDateTime::parse_from_str(&self.parent.end_date, "%Y-%m-%dT%H:%M:%S").unwrap();

            let end = DateTime::<Utc>::from_utc(naive_end, Utc);
            // let conn = Connection::open("fluxrs.db");
            let conn = match Connection::open("fluxrs.db") {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to open database: {}", e);
                    return; // Exit early if connection fails
                }
            };

            match (
                query_cycles(&conn, start, end),
                query_gas(&conn, start, end),
            ) {
                (Ok(times), Ok(gas_data)) => {
                    if times.start_time.is_empty() || gas_data.is_empty() {
                        self.parent.cycles = Vec::new();
                    } else {
                        match process_cycles(&times, &gas_data) {
                            Ok(cycle_vec) => {
                                println!("Successfully processed cycles!");
                                println!("Start Date: {}", start);
                                println!("End Date: {}", end);
                                self.parent.cycles = cycle_vec;
                                self.parent.update_cycle();
                            }
                            Err(e) => eprintln!("Error processing cycles: {}", e),
                        }
                    }
                }
                _ => eprintln!("Failed to query database."),
            }
        }
    }
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
    app.min_calc_area_range = 40.;
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
        let diff = (app.cycles[*app.index]
            .calc_range_end
            .get(gas_type)
            .unwrap_or(&0.0)
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
