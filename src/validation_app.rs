use crate::app_plotting::{init_attribute_plot, init_gas_plot, init_lag_plot};
use crate::archiverecord::ArchiveRecord;
use crate::constants::MIN_CALC_AREA_RANGE;
use crate::csv_parse;
use crate::cycle::{insert_fluxes_ignore_duplicates, load_fluxes, update_fluxes};
use crate::cycle_navigator::CycleNavigator;
use crate::errorcode::ErrorCode;
use crate::fluxes_schema::{make_select_all_fluxes, OTHER_COLS};
use crate::gasdata::query_gas_async;
use crate::gasdata::{insert_measurements, GasData};
use crate::instruments::InstrumentType;
use crate::instruments::{GasType, Li7810};
use crate::meteodata::{insert_meteo_data, query_meteo_async, MeteoData};
use crate::timedata::{query_cycles_async, TimeData};
use crate::volumedata::{insert_volume_data, query_volume, query_volume_async, VolumeData};
use crate::Cycle;
use crate::EqualLen;
use crate::ProcessEvent;
use crate::{insert_cycles, process_cycles};
use egui::FontFamily;
use tokio::sync::mpsc;

use eframe::egui::{Color32, Context, Id, Key, Stroke, Ui, WidgetInfo, WidgetType};
use egui_file::FileDialog;
use egui_plot::{LineStyle, PlotPoints, PlotUi, Polygon, VLine};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use csv::Writer;
use rusqlite::{types::ValueRef, Connection, Result, Row};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
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
    ProjInit,
    DataTable,
    DownloadData,
    Empty,
}

impl Default for Panel {
    fn default() -> Self {
        Self::Empty
    }
}

#[derive(Default)]
pub struct MainApp {
    live_panel: Panel,
    pub validation_panel: ValidationApp,
    table_panel: TableApp,
    empty_panel: EmptyPanel,
}
impl MainApp {
    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.apply_font_size(ctx, self.validation_panel.font_size);
        for (_text_style, font_id) in ui.style_mut().text_styles.iter_mut() {
            // font_id.size = self.validation_panel.font_size;
            font_id.family = FontFamily::Monospace;
        }
        ui.horizontal_wrapped(|ui| {
            let container_response = ui.response();
            container_response
                .widget_info(|| WidgetInfo::labeled(WidgetType::RadioGroup, true, "Select panel"));

            let panel_switching_allowed = !self.validation_panel.init_in_progress;
            ui.ctx().clone().with_accessibility_parent(container_response.id, || {
                ui.add_enabled(panel_switching_allowed, |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::Validation,
                            "Validate measurements",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::DataLoad,
                            "Load measurements",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::DataInit,
                            "Initiate measurements",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::FileInit,
                            "Upload files to db",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::ProjInit,
                            "Initiate project",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::DataTable,
                            "View data in db",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::DownloadData,
                            "Download data",
                        );
                    })
                    .response
                });
            });
        });
        ui.separator();
        if !self.validation_panel.initiated {
            self.validation_panel.load_projects_from_db().unwrap();
        }

        match self.live_panel {
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
            Panel::DownloadData => {
                self.validation_panel.dl_ui(ui, ctx);
            },
            Panel::ProjInit => {
                self.validation_panel.proj_ui(ui);
            },
            Panel::Empty => {
                self.empty_panel.ui(ui);
            },
        }
    }
    fn apply_font_size(&self, ctx: &egui::Context, font_size: f32) {
        use egui::{FontId, TextStyle};

        let mut style = (*ctx.style()).clone();

        // Update font sizes for the main text styles
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(font_size + 6.0)),
            (TextStyle::Body, FontId::proportional(font_size)),
            (TextStyle::Monospace, FontId::monospace(font_size)),
            (TextStyle::Button, FontId::proportional(font_size)),
            (TextStyle::Small, FontId::proportional(font_size - 2.0)),
        ]
        .into();

        ctx.set_style(style);
    }
}

// #[derive(Default)]
pub struct ValidationApp {
    pub runtime: tokio::runtime::Runtime,
    init_enabled: bool,
    init_in_progress: bool,
    cycles_progress: usize,
    cycles_state: Option<(usize, usize)>,
    query_in_progress: bool,
    pub load_result: Arc<Mutex<Option<Result<Vec<Cycle>, rusqlite::Error>>>>,
    progress_receiver: Option<tokio::sync::mpsc::UnboundedReceiver<ProcessEvent>>,
    pub task_done_sender: Sender<()>,
    pub task_done_receiver: Receiver<()>,
    pub instrument_serial: String,
    pub enabled_gases: HashSet<GasType>, // Stores which gases are enabled for plotting
    pub enabled_fluxes: HashSet<GasType>, // Stores which fluxes are enabled for plotting
    pub enabled_calc_rs: HashSet<GasType>, // Stores which r values are enabled for plotting
    pub enabled_measurement_rs: HashSet<GasType>, // Stores which gases are enabled for plotting
    pub enabled_conc_t0: HashSet<GasType>, // Stores which gases are enabled for plotting
    pub cycles: Vec<Cycle>,
    pub cycle_nav: CycleNavigator,
    archive_record: Option<(usize, ArchiveRecord)>,
    pub lag_plot_w: f32,
    pub lag_plot_h: f32,
    pub gas_plot_w: f32,
    pub gas_plot_h: f32,
    pub flux_plot_w: f32,
    pub flux_plot_h: f32,
    pub font_size: f32,
    pub measurement_r_plot_w: f32,
    pub measurement_r_plot_h: f32,
    pub calc_r_plot_w: f32,
    pub calc_r_plot_h: f32,
    pub conc_t0_plot_w: f32,
    pub conc_t0_plot_h: f32,
    pub dirty_cycles: HashSet<usize>,
    pub zoom_to_measurement: u8,
    pub should_reset_bounds: bool,
    pub drag_panel_width: f64,
    pub min_calc_area_range: f64,
    pub selected_point: Option<[f64; 2]>,
    pub dragged_point: Option<[f64; 2]>,
    pub chamber_colors: HashMap<String, Color32>, // Stores colors per chamber
    pub visible_traces: HashMap<String, bool>,
    pub all_traces: HashSet<String>,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub opened_files: Option<Vec<PathBuf>>,
    pub open_file_dialog: Option<FileDialog>,
    pub initial_path: Option<PathBuf>,
    pub selected_data_type: Option<DataType>,
    pub log_messages: VecDeque<String>,
    pub show_valids: bool,
    pub show_invalids: bool,
    pub show_bad: bool,
    pub project_name: String,
    pub main_gas: Option<GasType>,
    pub instrument: InstrumentType,
    pub projects: Vec<String>,
    pub initiated: bool,
    pub selected_project: Option<String>,
    pub proj_serial: String,
    pub show_linfit: bool,
    pub calc_area_color: Color32,
    pub calc_area_adjust_color: Color32,
    pub calc_area_stroke_color: Color32,
}

impl Default for ValidationApp {
    fn default() -> Self {
        let (task_done_sender, task_done_receiver) = std::sync::mpsc::channel();
        Self {
            runtime: tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap(),
            progress_receiver: None,
            dirty_cycles: HashSet::new(),
            task_done_sender,
            task_done_receiver,
            cycles_progress: 0,
            cycles_state: None,
            query_in_progress: false,
            init_enabled: true,
            init_in_progress: false,
            load_result: Arc::new(Mutex::new(None)),
            instrument_serial: String::new(),
            proj_serial: String::new(),
            enabled_gases: HashSet::new(),
            enabled_fluxes: HashSet::new(),
            enabled_measurement_rs: HashSet::new(),
            enabled_calc_rs: HashSet::new(),
            enabled_conc_t0: HashSet::new(),
            cycles: Vec::new(),
            cycle_nav: CycleNavigator::new(),
            archive_record: None,
            font_size: 14.,
            lag_plot_w: 600.,
            lag_plot_h: 350.,
            gas_plot_w: 600.,
            gas_plot_h: 350.,
            flux_plot_w: 600.,
            flux_plot_h: 350.,
            calc_r_plot_w: 600.,
            calc_r_plot_h: 350.,
            conc_t0_plot_w: 600.,
            conc_t0_plot_h: 350.,
            measurement_r_plot_w: 600.,
            measurement_r_plot_h: 350.,
            zoom_to_measurement: 0,
            should_reset_bounds: false,
            drag_panel_width: 40.0, // Default width for UI panel
            min_calc_area_range: MIN_CALC_AREA_RANGE,
            selected_point: None,
            dragged_point: None,
            chamber_colors: HashMap::new(),
            visible_traces: HashMap::new(),
            all_traces: HashSet::new(),
            start_date: NaiveDate::from_ymd_opt(2024, 5, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            end_date: NaiveDate::from_ymd_opt(2024, 5, 25)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            opened_files: None,
            open_file_dialog: None,
            initial_path: Some(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            selected_data_type: None,
            log_messages: VecDeque::new(),
            show_invalids: true,
            show_valids: true,
            show_bad: false,
            project_name: String::new(),
            main_gas: None,
            instrument: InstrumentType::Li7810,
            projects: Vec::new(),
            initiated: false,
            selected_project: None,
            show_linfit: true,
            calc_area_color: Color32::BLACK,
            calc_area_adjust_color: Color32::BLACK,
            calc_area_stroke_color: Color32::BLACK,
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
        // self.print_stats();

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
        let longest_label = "Measurement r plots";
        let galley = ui.fonts(|f| {
            f.layout_no_wrap(
                longest_label.to_string(),
                egui::FontId::monospace(self.font_size),
                egui::Color32::WHITE,
            )
        });
        let label_width = galley.size().x;
        egui::Window::new("Select displayed plots").show(ctx, |ui| {
            if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
                let gases = cycle.gases.clone(); // Clone gases early!

                let mut main_gases: Vec<(GasType, bool)> =
                    gases.iter().map(|gas| (*gas, self.is_gas_enabled(gas))).collect();

                let mut flux_gases: Vec<(GasType, bool)> =
                    gases.iter().map(|gas| (*gas, self.is_flux_enabled(gas))).collect();

                let mut calc_r_gases: Vec<(GasType, bool)> =
                    gases.iter().map(|gas| (*gas, self.is_calc_r_enabled(gas))).collect();

                let mut measurement_r_gases: Vec<(GasType, bool)> =
                    gases.iter().map(|gas| (*gas, self.is_measurement_r_enabled(gas))).collect();

                let mut conc_t0_gases: Vec<(GasType, bool)> =
                    gases.iter().map(|gas| (*gas, self.is_conc_t0_enabled(gas))).collect();

                let min_width = 100.;
                ui.horizontal(|ui| {
                    ui.group(|ui| {
                        ui.set_min_width(min_width); // Enforce group width here
                        ui.vertical(|ui| {
                            ui.label("main gas plots");
                            for (gas, mut is_enabled) in &mut main_gases {
                                if ui.checkbox(&mut is_enabled, format!("{:?}", gas)).changed() {
                                    if is_enabled {
                                        self.enabled_gases.insert(*gas);
                                    } else {
                                        self.enabled_gases.remove(gas);
                                    }
                                }
                            }
                        });
                    });

                    ui.group(|ui| {
                        ui.set_min_width(min_width); // Enforce group width here
                        ui.vertical(|ui| {
                            ui.label("Flux plots");
                            for (gas, mut is_enabled) in &mut flux_gases {
                                if ui.checkbox(&mut is_enabled, format!("{:?}", gas)).changed() {
                                    if is_enabled {
                                        self.enabled_fluxes.insert(*gas);
                                    } else {
                                        self.enabled_fluxes.remove(gas);
                                    }
                                }
                            }
                        });
                    });

                    ui.group(|ui| {
                        ui.set_min_width(min_width); // Enforce group width here
                        ui.vertical(|ui| {
                            ui.label("Calc r plots");
                            for (gas, mut is_enabled) in &mut calc_r_gases {
                                if ui.checkbox(&mut is_enabled, format!("{:?}", gas)).changed() {
                                    if is_enabled {
                                        self.enabled_calc_rs.insert(*gas);
                                    } else {
                                        self.enabled_calc_rs.remove(gas);
                                    }
                                }
                            }
                        });
                    });

                    ui.group(|ui| {
                        ui.set_min_width(min_width); // Enforce group width here
                        ui.vertical(|ui| {
                            ui.label("Measurement r plots");
                            for (gas, mut is_enabled) in &mut measurement_r_gases {
                                if ui.checkbox(&mut is_enabled, format!("{:?}", gas)).changed() {
                                    if is_enabled {
                                        self.enabled_measurement_rs.insert(*gas);
                                    } else {
                                        self.enabled_measurement_rs.remove(gas);
                                    }
                                }
                            }
                        });
                    });
                    ui.group(|ui| {
                        ui.set_min_width(min_width); // Enforce group width here
                        ui.vertical(|ui| {
                            ui.label("Concentration at t0");
                            for (gas, mut is_enabled) in &mut conc_t0_gases {
                                if ui.checkbox(&mut is_enabled, format!("{:?}", gas)).changed() {
                                    if is_enabled {
                                        self.enabled_conc_t0.insert(*gas);
                                    } else {
                                        self.enabled_conc_t0.remove(gas);
                                    }
                                }
                            }
                        });
                    });
                });
            } else {
                ui.label("No current cycle available to select gases from.");
            }
        });

        egui::Window::new("Current Cycle details").show(ctx, |ui| {
            if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
                let errors = ErrorCode::from_mask(cycle.error_code.0);
                let error_messages: Vec<String> =
                    errors.iter().map(|error| error.to_string()).collect();

                let main_gas = if let Some(gas) = self.main_gas {
                    gas
                } else {
                    eprintln!("No main gas selected!");
                    return;
                };

                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Button,
                    egui::FontId::new(18.0, eframe::epaint::FontFamily::Monospace),
                );

                ui.add_space(5.0);

                ui.collapsing("Cycle details", |ui| {
                    egui::Grid::new("cycle_details_grid").striped(true).show(ui, |ui| {
                        ui.label("Model:");
                        ui.label(format!("{}", cycle.instrument_model));
                        ui.end_row();
                        ui.label("Serial:");
                        ui.label(&cycle.instrument_serial);
                        ui.end_row();
                        ui.label("Chamber:");
                        ui.label(cycle.chamber_id.to_string());
                        ui.end_row();
                        ui.label("Start Time:");
                        ui.label(cycle.start_time.to_string());
                        ui.end_row();
                        ui.label("Epoch:");
                        ui.label(cycle.start_time.timestamp().to_string());
                        ui.end_row();
                        ui.label("Epoch End:");
                        ui.label((cycle.start_time.timestamp() + cycle.end_offset).to_string());
                        ui.end_row();
                        ui.label("First TS:");
                        if let Some(first_val) = cycle.dt_v.first() {
                            ui.label(format!("{}", first_val.to_owned()));
                        } else {
                            ui.label("None");
                        }
                        ui.end_row();
                        ui.label("Last TS:");
                        if let Some(last_val) = cycle.dt_v.last() {
                            ui.label(format!("{}", last_val.to_owned()));
                        } else {
                            ui.label("None");
                        }
                        ui.end_row();
                        ui.label("Close Offset:");
                        ui.label(cycle.close_offset.to_string());
                        ui.end_row();
                        ui.label("Open Offset:");
                        ui.label(cycle.open_offset.to_string());
                        ui.end_row();
                        ui.label("End Offset:");
                        ui.label(cycle.end_offset.to_string());
                        ui.end_row();
                        ui.label("Current Index:");
                        ui.label(self.cycle_nav.current_index().unwrap().to_string());
                        ui.end_row();
                        ui.label("Is Valid:");
                        ui.label(cycle.is_valid.to_string());
                        ui.end_row();
                        ui.label("Manual Valid:");
                        ui.label(format!("{:?}", cycle.manual_valid));
                        ui.end_row();
                        ui.label("Override:");
                        ui.label(format!("{:?}", cycle.override_valid));
                        ui.end_row();
                        ui.label("Error Code:");
                        ui.label(format!("{:?}", cycle.error_code.0));
                        ui.end_row();
                        ui.label("Visible Cycles:");
                        ui.label(format!(
                            "{}/{}",
                            self.cycle_nav.visible_count(),
                            self.cycles.len()
                        ));
                        ui.end_row();
                        ui.label("Measurement R²:");
                        ui.label(match cycle.measurement_r2.get(&main_gas) {
                            Some(r) => format!("{:.6}", r),
                            None => "N/A".to_string(),
                        });
                        ui.end_row();

                        if !error_messages.is_empty() {
                            ui.label("Errors:");
                            ui.label(error_messages.join("\n"));
                            ui.end_row();
                        }
                    });
                });
                ui.separator();

                // Optional: Grid for enabled gases
                egui::Grid::new("gas_values_grid").striped(true).show(ui, |ui| {
                    ui.label("Gas");
                    ui.label("calc_r2");
                    ui.label("flux");
                    ui.end_row();

                    for gas in &self.enabled_gases {
                        let calc_r2 = match cycle.calc_r2.get(gas) {
                            Some(r) => format!("{:.6}", r),
                            None => "N/A".to_string(),
                        };

                        let flux = match cycle.flux.get(gas) {
                            Some(f) => format!("{:.6}", f),
                            None => "N/A".to_string(),
                        };

                        ui.label(format!("{}", gas));
                        ui.label(calc_r2);
                        ui.label(flux);
                        ui.end_row();
                    }
                });
            } else {
                ui.label("No cycle selected.");
            }
        });

        let mut prev_clicked = false;
        let mut next_clicked = false;
        let mut highest_r = false;
        let mut reset_cycle = false;
        let mut toggle_valid = false;
        let mut add_to_end = false;
        let mut add_to_start = false;
        let mut remove_from_end = false;
        let mut remove_from_start = false;
        let mut mark_bad = false;
        let mut show_valids_clicked = false;
        let mut show_invalids_clicked = false;
        let mut show_bad = false;
        let mut show_linear_model = true;
        let mut reload_gas = false;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            prev_clicked = ui.add(egui::Button::new("Prev measurement")).clicked();
            next_clicked = ui.add(egui::Button::new("Next measurement")).clicked();
        });

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                show_valids_clicked = ui.checkbox(&mut self.show_valids, "Show valids").clicked();
                show_invalids_clicked =
                    ui.checkbox(&mut self.show_invalids, "Show invalids").clicked();
                show_bad = ui.checkbox(&mut self.show_bad, "Show bad measurements").clicked();
            });
            show_linear_model = ui.checkbox(&mut self.show_linfit, "Show linear model").clicked();
        });
        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            highest_r = ui.add(egui::Button::new("Find highest r")).clicked();
            reset_cycle = ui.add(egui::Button::new("Reset cycle")).clicked();
            mark_bad = ui.add(egui::Button::new("Mark as bad")).clicked();
            toggle_valid = ui.add(egui::Button::new("Toggle validity")).clicked();
            add_to_end = ui.add(egui::Button::new("+2min to end")).clicked();
            remove_from_end = ui.add(egui::Button::new("-2min to end")).clicked();
            add_to_start = ui.add(egui::Button::new("+2min to start")).clicked();
            remove_from_start = ui.add(egui::Button::new("-2min to start")).clicked();
            reload_gas = ui.add(egui::Button::new("Reload gas data")).clicked();
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
                        if self.zoom_to_measurement == 2 {
                            self.zoom_to_measurement = 0
                        } else {
                            self.zoom_to_measurement += 1;
                        }
                    }
                }
                if let egui::Event::Key { key: Key::R, pressed, .. } = event {
                    if *pressed {
                        reset_cycle = true;
                    }
                }

                if let egui::Event::Key { key: Key::S, pressed, .. } = event {
                    if *pressed {
                        if let Some(current_visible_idx) = self.cycle_nav.current_index() {
                            if current_visible_idx > 0 {
                                // First copy chamber_id (clone!) to a new local String
                                let chamber_id =
                                    self.cycles[current_visible_idx].chamber_id.clone();

                                // Now safe to mutate!
                                let (before, after) = self.cycles.split_at_mut(current_visible_idx);
                                let current_cycle = &mut after[0];

                                if let Some(previous_cycle) =
                                    before.iter().rev().find(|cycle| cycle.chamber_id == chamber_id)
                                {
                                    current_cycle.set_open_lag(previous_cycle.open_lag_s);

                                    let target = current_cycle.start_time
                                        + chrono::TimeDelta::seconds(current_cycle.open_offset)
                                        + chrono::TimeDelta::seconds(
                                            current_cycle.open_lag_s as i64,
                                        );

                                    let Some(main_gas) = self.main_gas else {
                                        eprintln!("No main gas selected!");
                                        return;
                                    };

                                    current_cycle
                                        .get_peak_near_timestamp(main_gas, target.timestamp());

                                    self.mark_dirty();
                                    self.update_plots();
                                }
                            }
                        }
                    }
                }

                if let egui::Event::Key { key: Key::ArrowDown, pressed, .. } = event {
                    if *pressed {
                        self.mark_dirty();
                        if self.zoom_to_measurement == 1 || self.zoom_to_measurement == 0 {
                            self.increment_open_lag(-1.);
                        }
                        if self.zoom_to_measurement == 2 {
                            self.increment_close_lag(-1.);
                        }
                        self.update_plots();
                    }
                }
                if let egui::Event::Key { key: Key::ArrowUp, pressed, .. } = event {
                    if *pressed {
                        self.mark_dirty();
                        if self.zoom_to_measurement == 1 || self.zoom_to_measurement == 0 {
                            self.increment_open_lag(1.);
                        }
                        if self.zoom_to_measurement == 2 {
                            self.increment_close_lag(1.);
                        }
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
        if show_bad {
            self.update_plots();
        }
        if reload_gas {
            self.reload_gas();
        }

        if toggle_valid {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.toggle_manual_valid();
                self.update_plots();
            }
        }

        if mark_bad {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.toggle_manual_valid();
                cycle.error_code.toggle(ErrorCode::BadOpenClose);

                self.update_plots();
            }
        }
        if reset_cycle {
            self.mark_dirty();
            // NOTE: hitting reset on a cycle that has no changes, will cause it to be archived
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.reset();
                self.update_plots();
            }
        }

        if highest_r {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.recalc_r();
                self.update_plots();
            }
        }
        if add_to_end {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.set_end_lag_s(cycle.end_lag_s + 120.);
                // cycle.reload_gas_data();
                self.update_plots();
            }
        }
        if remove_from_end {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.set_end_lag_s(cycle.end_lag_s - 120.);
                self.update_plots();
            }
        }
        if add_to_start {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.set_start_lag_s(cycle.start_lag_s - 120.);
                // cycle.reload_gas_data();
                self.update_plots();
            }
        }
        if remove_from_start {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.set_start_lag_s(cycle.start_lag_s + 120.);
                // cycle.reload_gas_data();
                self.update_plots();
            }
        }

        if prev_clicked {
            self.commit_current_cycle();
            self.cycle_nav.step_back(); // Step to previous visible cycle
            if let Some(_index) = self.cycle_nav.current_index() {
                self.update_plots();
            }
        }

        if next_clicked {
            self.commit_current_cycle();
            self.cycle_nav.step_forward(); // Step to next visible cycle
            if let Some(_index) = self.cycle_nav.current_index() {
                self.update_plots();
            }
        }
        let main_gas = if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.main_gas
        } else {
            if self.cycles.is_empty() {
                ui.label("No cycles loaded");
            }
            if self.cycle_nav.visible_count() == 0 {
                ui.label("All cycles hidden.");
            }
            return;
        };
        if self.enabled_gases.is_empty() {
            self.enabled_gases.insert(main_gas);
        }

        let lag_s = if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.open_lag_s
        } else {
            return;
        };

        let drag_panel_width = 40.;
        if ctx.style().visuals.dark_mode {
            self.calc_area_color = Color32::from_rgba_unmultiplied(255, 255, 255, 1);
            self.calc_area_adjust_color = Color32::from_rgba_unmultiplied(255, 255, 255, 20);
            self.calc_area_stroke_color = Color32::from_rgba_unmultiplied(255, 255, 255, 60);
        } else {
            self.calc_area_color = Color32::from_rgba_unmultiplied(0, 0, 0, 10);
            self.calc_area_adjust_color = Color32::from_rgba_unmultiplied(0, 0, 20, 20);
            self.calc_area_stroke_color = Color32::from_rgba_unmultiplied(0, 0, 0, 90);
        }

        // let close_line_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
        let left_id = Id::new("left_test");
        let main_id = Id::new("main_area");
        let right_id = Id::new("right_area");

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    if self.zoom_to_measurement == 2 {
                        self.should_reset_bounds = true;
                    }
                    for gas_type in self.enabled_gases.clone() {
                        if self.is_gas_enabled(&gas_type) {
                            let gas_plot = init_gas_plot(
                                &gas_type,
                                self.get_start(),
                                self.get_end(),
                                self.gas_plot_w,
                                self.gas_plot_h,
                            );
                            let response = gas_plot.show(ui, |plot_ui| {
                                self.render_gas_plot_ui(
                                    plot_ui, gas_type, main_id, left_id, right_id,
                                )
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                // Hide cursor
                                // println!("Gas plot is hovered!");
                            }
                        }
                    }
                    if self.should_reset_bounds && self.zoom_to_measurement == 0 {
                        self.should_reset_bounds = false;
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    for gas in self.enabled_fluxes.clone() {
                        let measurement_r_plot = init_attribute_plot(
                            "flux".to_owned(),
                            &gas,
                            self.flux_plot_w,
                            self.flux_plot_h,
                        );
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
                        let measurement_r_plot = init_attribute_plot(
                            "measurement r2".to_owned(),
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
                        let calc_r_plot = init_attribute_plot(
                            "calc r2".to_owned(),
                            &gas,
                            self.calc_r_plot_w,
                            self.calc_r_plot_h,
                        );
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
                ui.vertical(|ui| {
                    for gas in self.enabled_conc_t0.clone() {
                        let conc_plot = init_attribute_plot(
                            "Concentration t0".to_owned(),
                            &gas,
                            self.conc_t0_plot_w,
                            self.conc_t0_plot_h,
                        );
                        // ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                        let response = conc_plot.show(ui, |plot_ui| {
                            self.render_attribute_plot(
                                plot_ui,
                                &gas,
                                |cycle, gas_type| {
                                    *cycle.concentration_t0.get(gas_type).unwrap_or(&0.0)
                                },
                                "Conc t0",
                            );
                        });
                        if response.response.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            // Hide cursor
                            // println!("Gas plot is hovered!");
                        }
                    }
                })
            });
        });
        //
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

    fn date_picker(&mut self, ui: &mut egui::Ui) {
        let mut picker_start = self.start_date.date_naive();
        let mut picker_end = self.end_date.date_naive();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                // Start Date Picker
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
                            .push_front("Start date can't be after end date.".to_string());
                    } else {
                        self.start_date = pick;
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
                            .push_front("End date can't be before start date.".to_string());
                    } else {
                        self.end_date = pick;
                    }
                }
            });
        });

        if self.start_date > self.end_date {
            self.log_messages.push_front("End date can't be before start date.".to_owned());
        }

        let delta_days = self.end_date - self.start_date;
        let days = delta_days.to_std().unwrap().as_secs() / 86400;

        if ui.button(format!("Next {} days", days)).clicked() {
            self.start_date += delta_days;
            self.end_date += delta_days;
        }
        if ui.button(format!("Previous {} days", days)).clicked() {
            self.start_date -= delta_days;
            self.end_date -= delta_days;
        }
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
        self.handle_progress_messages();
        if self.task_done_receiver.try_recv().is_ok() {
            self.init_in_progress = false;
            self.init_enabled = true;

            if let Ok(mut result_lock) = self.load_result.lock() {
                if let Some(result) = result_lock.take() {
                    match result {
                        Ok(cycles) => {
                            self.cycles = cycles;
                            self.log_messages.push_front("Successfully loaded cycles.".to_string());
                        },
                        Err(e) => {
                            eprintln!("Failed to load cycles: {:?}", e);
                            self.log_messages.push_front(format!("Error: {}", e));
                        },
                    }
                }
            }
            self.update_plots();
        }
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }

        if self.init_in_progress || !self.init_enabled {
            ui.add(egui::Spinner::new());
            ui.label("Loading fluxes from db...");
            // return; // optionally stop drawing the rest of the UI while loading
        } else {
            self.date_picker(ui);

            if ui.button("Init from db").clicked() {
                self.commit_all_dirty_cycles();
                let sender = self.task_done_sender.clone();
                let result_slot = self.load_result.clone();
                let start_date = self.start_date;
                let end_date = self.end_date;
                let project = self.selected_project.as_ref().unwrap().clone();
                let serial = self.instrument_serial.clone();
                let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
                self.progress_receiver = Some(progress_receiver);

                self.init_enabled = false;
                self.init_in_progress = true;

                self.runtime.spawn(async move {
                    let result = match Connection::open("fluxrs.db") {
                        Ok(mut conn) => load_fluxes(
                            &mut conn,
                            start_date,
                            end_date,
                            project,
                            serial,
                            progress_sender,
                        ),
                        Err(e) => Err(e),
                    };

                    if let Ok(mut slot) = result_slot.lock() {
                        *slot = Some(result);
                    }

                    let _ = sender.send(()); // Notify UI
                });
            }
        }
        self.log_display(ui);
    }

    pub fn init_ui(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        // Check if background task has finished
        if self.task_done_receiver.try_recv().is_ok() {
            self.init_in_progress = false;
            self.init_enabled = true;
        }

        // Show info if no project selected
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        self.handle_progress_messages();

        // Show spinner if processing is ongoing
        if self.init_in_progress || !self.init_enabled {
            ui.add(egui::Spinner::new());
            if self.query_in_progress {
                ui.label("Querying data, this can take a while for large time ranges.");
            } else if let Some((_, total)) = self.cycles_state {
                // ui.label(format!("Processed {}/{} cycles...", self.cycles_progress, total));
                let pb =
                    egui::widgets::ProgressBar::new(self.cycles_progress as f32 / total as f32)
                        .desired_width(200.)
                        .corner_radius(1)
                        .show_percentage()
                        .text(format!("{}/{}", self.cycles_progress, total));
                ui.add(pb);
            } else {
                ui.label("Processing cycles...");
            }
            self.log_display(ui);
            return;
        }

        // Main UI layout
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                self.date_picker(ui);
                // Date navigation buttons

                // Trigger processing with selected date range
                if ui
                    .add_enabled(
                        self.init_enabled && !self.init_in_progress,
                        egui::Button::new("Use range"),
                    )
                    .clicked()
                {
                    self.commit_all_dirty_cycles();
                    self.init_enabled = false;
                    self.init_in_progress = true;
                    self.query_in_progress = true;

                    let start_date = self.start_date;
                    let end_date = self.end_date;
                    let project = self.selected_project.as_ref().unwrap().clone();
                    let instrument_serial = self.instrument_serial.clone();

                    let conn = match Connection::open("fluxrs.db") {
                        Ok(conn) => conn,
                        Err(e) => {
                            eprintln!("Failed to open database: {}", e);
                            return;
                        },
                    };
                    let arc_conn = Arc::new(Mutex::new(conn));
                    let sender = self.task_done_sender.clone();
                    let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
                    self.progress_receiver = Some(progress_receiver);

                    self.runtime.spawn(async move {
                        let cycles_result = query_cycles_async(
                            arc_conn.clone(),
                            start_date,
                            end_date,
                            project.clone(),
                        )
                        .await;
                        let gas_result = query_gas_async(
                            arc_conn.clone(),
                            start_date,
                            end_date,
                            project.clone(),
                            instrument_serial,
                        )
                        .await;
                        let meteo_result = query_meteo_async(
                            arc_conn.clone(),
                            start_date,
                            end_date,
                            project.clone(),
                        )
                        .await;
                        let volume_result = query_volume_async(
                            arc_conn.clone(),
                            start_date,
                            end_date,
                            project.clone(),
                        )
                        .await;

                        match (cycles_result, gas_result, meteo_result, volume_result) {
                            (Ok(times), Ok(gas_data), Ok(meteo_data), Ok(volume_data)) => {
                                let _ = progress_sender.send(ProcessEvent::QueryComplete);
                                if !times.start_time.is_empty() && !gas_data.is_empty() {
                                    run_processing_dynamic(
                                        times,
                                        gas_data,
                                        meteo_data,
                                        volume_data,
                                        project,
                                        arc_conn.clone(),
                                        progress_sender,
                                    )
                                    .await;
                                    let _ = sender.send(());
                                }
                            },
                            e => eprintln!("Failed to query database: {:?}", e),
                        }
                    });
                }
            });
            if self.start_date > self.end_date {
                self.log_messages.push_front("End date can't be before start date.".to_string());
            }

            ui.separator();
            render_recalculate_ui(
                ui,
                &self.runtime,
                self.start_date,
                self.end_date,
                self.project_name.clone(),
                self.instrument_serial.clone(),
                &mut self.log_messages,
            );
        });
        // Handle messages from background processing

        // Display log messages
        self.log_display(ui);
    }
    pub fn file_ui(&mut self, ui: &mut Ui, ctx: &Context) {
        self.handle_progress_messages();
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
    pub fn handle_progress_messages(&mut self) {
        if let Some(receiver) = &mut self.progress_receiver {
            while let Ok(msg) = receiver.try_recv() {
                match msg {
                    ProcessEvent::QueryComplete => {
                        self.query_in_progress = false;
                    },
                    ProcessEvent::Progress(c) => {
                        self.cycles_state = Some(c);
                        let (current, _) = c;
                        self.cycles_progress += current;
                    },
                    ProcessEvent::Error(e) => {
                        self.log_messages.push_front(format!("Error: {}", e));
                    },
                    ProcessEvent::Done => {
                        self.log_messages.push_front("All processing finished.".to_string());
                        self.cycles_progress = 0;
                    },
                    ProcessEvent::NoGasData(start_time) => {
                        self.log_messages
                            .push_front(format!("No gas data found for cycle at {}", start_time));
                    },
                    ProcessEvent::ReadFile(filename) => {
                        self.log_messages.push_front(format!("Read file: {}", filename));
                    },
                    ProcessEvent::ReadFileRows(filename, rows) => {
                        self.log_messages
                            .push_front(format!("Read file: {} with {} rows", filename, rows));
                    },
                    ProcessEvent::ReadFileFail(filename, e) => {
                        self.log_messages
                            .push_front(format!("Failed to read file {}, error: {}", filename, e));
                    },
                    ProcessEvent::InsertOk(rows) => {
                        self.log_messages.push_front(format!("Inserted {} rows", rows));
                    },
                    ProcessEvent::InsertOkSkip(rows, duplicates) => {
                        self.log_messages.push_front(format!(
                            "Inserted {} rows, skipped {} duplicates.",
                            rows, duplicates
                        ));
                    },
                    ProcessEvent::InsertFail(e) => {
                        self.log_messages.push_front(format!("Failed to insert rows: {}", e));
                    },
                    ProcessEvent::ProgressDay(date) => {
                        self.log_messages.push_front(format!("Loaded cycles from {}", date));
                    },
                    _ => self.log_messages.push_front("Unformatted Processing Message".to_owned()),
                }
            }
        }
    }
    fn upload_cycle_data(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
        self.log_messages.push_front("Uploading cycle data...".to_string());

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

                        self.log_messages.push_front(format!("Successfully read {:?}", path));
                    } else {
                        self.log_messages
                            .push_front(format!("Skipped file {:?}: Invalid data length", path));
                    }
                },
                Err(e) => {
                    self.log_messages.push_front(format!("Failed to read file {:?}: {}", path, e));
                },
            }
        }
        match insert_cycles(conn, &all_times, self.selected_project.as_ref().unwrap().clone()) {
            Ok((row_count, duplicates)) => {
                self.log_messages.push_front(format!(
                    "Successfully inserted {} rows into DB. Skipped {}.",
                    row_count, duplicates
                ));
            },
            Err(e) => {
                self.log_messages
                    .push_front(format!("Failed to insert cycle data to db.Error {}", e));
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
                    self.log_messages.push_front(format!("Failed to read file {:?}: {}", path, e));
                },
            }
        }
        match insert_meteo_data(conn, &self.selected_project.as_ref().unwrap().clone(), &meteos) {
            Ok(row_count) => {
                self.log_messages
                    .push_front(format!("Successfully inserted {} rows into DB.", row_count));
            },
            Err(e) => {
                self.log_messages
                    .push_front(format!("Failed to insert cycle data to db.Error {}", e));
            },
        }
        self.log_messages.push_front("Uploading meteo data...".to_string());
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
                    self.log_messages.push_front(format!("Failed to read file {:?}: {}", path, e));
                },
            }
        }
        match insert_volume_data(conn, &self.selected_project.as_ref().unwrap().clone(), &volumes) {
            Ok(row_count) => {
                self.log_messages
                    .push_front(format!("Successfully inserted {} rows into DB.", row_count));
            },
            Err(e) => {
                self.log_messages
                    .push_front(format!("Failed to insert cycle data to db.Error {}", e));
            },
        }
        self.log_messages.push_front("Uploading meteo data...".to_string());
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

                    let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
                    self.progress_receiver = Some(progress_receiver);
                    let arc_msgs = Arc::new(Mutex::new(self.log_messages.clone()));
                    if !selected_paths.is_empty() {
                        self.opened_files = Some(selected_paths.clone());
                        // self.log_messages
                        //     .push_front(format!("Selected files: {:?}", selected_paths));
                        self.process_files_async(
                            selected_paths,
                            self.selected_data_type.clone(),
                            self.project_name.clone(),
                            arc_msgs,
                            progress_sender.clone(),
                            &self.runtime,
                        );
                    }

                    self.open_file_dialog = None; //   Close the dialog
                },
                egui_file::State::Cancelled | egui_file::State::Closed => {
                    self.log_messages.push_front("File selection cancelled.".to_string());
                    self.open_file_dialog = None;
                },
                _ => {}, // Do nothing if still open
            }
        }
    }

    fn process_gas_files(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
        self.log_messages.push_front("Uploading gas data...".to_owned());
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
                            all_gas.gas.entry(gas_type).or_default().extend(values);
                        }
                        self.log_messages.push_front(format!(
                            "Succesfully read file {:?} with {} rows.",
                            path, rows
                        ));
                    }
                },
                Err(e) => {
                    self.log_messages.push_front(format!("Failed to read file {:?}: {}", path, e));
                },
            }
        }
        match insert_measurements(conn, &all_gas, self.selected_project.as_ref().unwrap().clone()) {
            Ok((row_count, duplicates)) => {
                self.log_messages.push_front(format!(
                    "Successfully inserted {} rows into DB. Skipped {} rows.",
                    row_count, duplicates
                ));
            },
            Err(_) => {
                self.log_messages.push_front("Failed to insert gas data to db.".to_owned());
            },
        }
    }

    pub fn process_files_async(
        &self,
        path_list: Vec<PathBuf>,
        data_type: Option<DataType>,
        project: String,
        log_messages: Arc<Mutex<VecDeque<String>>>,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
        runtime: &tokio::runtime::Runtime,
    ) {
        runtime.spawn(async move {
            tokio::task::spawn_blocking(move || match Connection::open("fluxrs.db") {
                Ok(mut conn) => {
                    if let Some(data_type) = data_type {
                        match data_type {
                            DataType::Gas => process_gas_files_async(
                                path_list,
                                &mut conn,
                                project,
                                progress_sender,
                            ),
                            DataType::Cycle => {
                                upload_cycle_data_async(path_list, &mut conn, project, log_messages)
                            },
                            DataType::Meteo => {
                                upload_meteo_data_async(path_list, &mut conn, project, log_messages)
                            },
                            DataType::Volume => upload_volume_data_async(
                                path_list,
                                &mut conn,
                                project,
                                log_messages,
                            ),
                        }
                    }
                },
                Err(e) => {
                    let mut logs = log_messages.lock().unwrap();
                    logs.push_front(format!("❌ Failed to connect to database: {}", e));
                },
            })
            .await
            .unwrap(); // handle join error if needed
        });
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
                self.log_messages.push_front(format!("❌ Failed to connect to database: {}", e));
            },
        }
    }

    fn load_projects_from_db(&mut self) -> Result<()> {
        let conn = Connection::open("fluxrs.db")?;

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
                self.main_gas = main_gas.parse::<GasType>().ok();
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

    pub fn dl_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.heading("Data downloader");
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        if ui.button("Download all calculated fluxes for current project.").clicked() {
            match export_sqlite_to_csv(
                "fluxrs.db",
                "fluxrs.csv",
                self.selected_project.clone().unwrap(),
            ) {
                Ok(_) => println!("Succesfully downloaded csv."),
                Err(e) => println!("Failed to download csv. Error: {}", e),
            }
        }
    }
    pub fn _dl_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        // WIP dl_ui function
        ui.heading("Data downloader");
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        if ui.button("Download all calculated fluxes for current project.").clicked() {
            match export_sqlite_to_csv(
                "fluxrs.db",
                "fluxrs.csv",
                self.selected_project.clone().unwrap(),
            ) {
                Ok(_) => println!("Succesfully downloaded csv."),
                Err(e) => println!("Failed to download csv. Error: {}", e),
            }
        }
        let mut checked = false;
        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.instrument.available_gases() {
                        ui.checkbox(&mut checked, gas.flux_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.instrument.available_gases() {
                        ui.checkbox(&mut checked, gas.r2_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.instrument.available_gases() {
                        ui.checkbox(&mut checked, gas.measurement_r2_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.instrument.available_gases() {
                        ui.checkbox(&mut checked, gas.calc_range_start_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.instrument.available_gases() {
                        ui.checkbox(&mut checked, gas.calc_range_end_col());
                    }
                });
            });
        });
        ui.group(|ui| {
            ui.vertical(|ui| {
                for col in OTHER_COLS {
                    ui.checkbox(&mut checked, *col);
                }
            });
        });
    }
    /// Find the most recent previous cycle with matching chamber_id
    fn _find_previous_cycle(&self, chamber_id: &str) -> Option<&Cycle> {
        if let Some(current_visible_idx) = self.cycle_nav.current_index() {
            if current_visible_idx > 0 {
                let (before, _) = self.cycles.split_at(current_visible_idx);
                return before.iter().rev().find(|cycle| cycle.chamber_id == chamber_id);
            }
        }
        None
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

pub fn handle_drag_polygon(
    plot_ui: &mut PlotUi,
    app: &mut ValidationApp,
    is_left: bool,
    gas_type: &GasType,
) {
    let mut dx = plot_ui.pointer_coordinate_drag_delta().x as f64;

    let calc_start = app.get_calc_start(*gas_type);
    let calc_end = app.get_calc_end(*gas_type);
    let calc_range = calc_end - calc_start;

    let close_time = app.get_measurement_start();
    let open_time = app.get_measurement_end();
    let at_min_range = calc_range <= app.min_calc_area_range;

    if is_left {
        let can_move_left = calc_start >= close_time;
        let not_shrinking = !at_min_range || dx < 0.0;

        if can_move_left && not_shrinking {
            app.increment_calc_start(*gas_type, dx);
        }
    } else {
        let can_move_right = calc_end <= open_time;
        let not_shrinking = !at_min_range || dx > 0.0;

        if can_move_right && not_shrinking {
            app.increment_calc_end(*gas_type, dx);
        }
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
    Polygon::new(
        id,
        PlotPoints::from(vec![
            [start_x, min_y],
            [start_x, max_y],
            [end_x, max_y],
            [end_x, min_y],
            [start_x, min_y], // Close the polygon
        ]),
    )
    .fill_color(color)
    .stroke(Stroke::new(2.0, stroke))
    .allow_hover(true)
}

pub fn create_vline(x: f64, color: Color32, style: LineStyle, id: &str) -> VLine {
    VLine::new(id, x).allow_hover(true).style(style).stroke(Stroke::new(2.0, color))
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

        let mut index = None;
        if table_name == "project" {
            index = None
        }
        if table_name == "fluxes" {
            index = Some("start_time")
        }
        if table_name == "fluxes_history" {
            index = Some("start_time")
        }
        if table_name == "cycles" {
            index = Some("start_time")
        }
        if table_name == "measurements" {
            index = Some("datetime")
        }
        if table_name == "meteo" {
            index = Some("datetime")
        }
        if table_name == "volume" {
            index = Some("datetime")
        }
        let query = match index {
            None => &format!("SELECT * FROM {}", table_name),
            Some(val) => &format!("SELECT * FROM {} ORDER BY {}", table_name, val),
        };
        let mut stmt = conn.prepare(query).unwrap();
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
        if self.selected_table == Some("measurements".to_owned()) {
            ui.label("Viewing measurements is disabled for now, too much data.");
            ui.label(
                "Need to implement selecting a time range because of the massive amount of data.",
            );
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
                            if table == "measurements" {
                                return;
                            }
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
const MAX_CONCURRENT_TASKS: usize = 10;

pub async fn run_processing_dynamic(
    times: TimeData,
    gas_data: HashMap<String, GasData>,
    meteo_data: MeteoData,
    volume_data: VolumeData,
    project: String,
    conn: Arc<Mutex<rusqlite::Connection>>,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    if times.start_time.is_empty() || gas_data.is_empty() {
        let _ = progress_sender.send(ProcessEvent::Error("No data available".into()));
        return;
    }

    let total_cycles = times.start_time.len();
    let gas_data_arc = Arc::new(gas_data);
    let mut time_chunks = VecDeque::from(times.chunk()); // ⬅️ chunk into ~250 cycles
    let mut active_tasks = Vec::new();

    let mut processed = 0;
    while !time_chunks.is_empty() || !active_tasks.is_empty() {
        // Fill up active tasks
        while active_tasks.len() < MAX_CONCURRENT_TASKS && !time_chunks.is_empty() {
            let chunk = time_chunks.pop_front().unwrap();
            let dates: HashSet<_> =
                chunk.start_time.iter().map(|dt| dt.format("%Y-%m-%d").to_string()).collect();

            let mut chunk_gas_data = HashMap::new();
            for date in &dates {
                if let Some(day_data) = gas_data_arc.get(date) {
                    chunk_gas_data.insert(date.clone(), day_data.clone());
                }
            }

            let meteo = meteo_data.clone();
            let volume = volume_data.clone();
            let project_clone = project.clone();
            let progress_sender = progress_sender.clone();

            let task = tokio::task::spawn_blocking(move || {
                match process_cycles(
                    &chunk,
                    &chunk_gas_data,
                    &meteo,
                    &volume,
                    project_clone,
                    progress_sender.clone(),
                ) {
                    Ok(result) => {
                        if processed >= total_cycles {
                            let _ = progress_sender.send(ProcessEvent::Done);
                        }
                        let count = result.iter().flatten().count();
                        let _ = progress_sender.send(ProcessEvent::Progress((count, total_cycles)));
                        Ok(result)
                    },
                    Err(e) => {
                        let _ = progress_sender.send(ProcessEvent::Error(e.to_string()));
                        Err(e)
                    },
                }
            });

            active_tasks.push(task);
        }

        // Wait for one task to finish
        let (result, i, remaining_tasks) = futures::future::select_all(active_tasks).await;
        active_tasks = remaining_tasks; // assign back for next loop

        match result {
            Ok(Ok(cycles)) => {
                if cycles.is_empty() {
                } else {
                    match insert_fluxes_ignore_duplicates(
                        &mut conn.lock().unwrap(),
                        &cycles,
                        project.clone(),
                    ) {
                        Ok((_, _)) => {
                            // println!("{} Fluxes inserted successfully!", pushed);
                            // println!("{} cycles skipped.", skipped);
                        },
                        Err(e) => eprintln!("Error inserting fluxes: {}", e),
                    }
                }
                // handle your successful cycles
            },
            Ok(Err(e)) => eprintln!("Cycle error: {e}"),
            Err(e) => eprintln!("Join error: {e}"),
        }
    }

    // Final insert (if you're collecting cycles earlier)
    let _ = progress_sender.send(ProcessEvent::Done);
}
async fn run_processing(
    times: TimeData,
    gas_data: HashMap<String, GasData>,
    meteo_data: MeteoData,
    volume_data: VolumeData,
    project: String,
    conn: Arc<Mutex<rusqlite::Connection>>,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    println!("Running cycle processing threads.");
    if times.start_time.is_empty() || gas_data.is_empty() {
        println!("Empty data — skipping");
        return;
    }

    let total_cycles = times.start_time.len();
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
        let progress_sender = progress_sender.clone();
        let task = tokio::task::spawn_blocking(move || {
            match process_cycles(
                &filtered_times,
                &gas_data_for_thread,
                &meteo_clone,
                &volume_clone,
                project_clone,
                progress_sender.clone(),
            ) {
                Ok(result) => {
                    println!("sent message");
                    let count = result.iter().flatten().count();
                    let _ = progress_sender.send(ProcessEvent::Progress((count, total_cycles)));
                    Ok(result)
                },
                Err(e) => {
                    let _ = progress_sender.send(ProcessEvent::Error(format!("{}", e)));
                    Err(e)
                },
            }
        });

        tasks.push(task);
    }

    let results = futures::future::join_all(tasks).await;

    let mut all_cycles: Vec<Option<Cycle>> = Vec::new();

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
            Ok((pushed, skipped)) => {
                println!("{} Fluxes inserted successfully!", pushed);
                println!("{} cycles skipped.", skipped);
            },
            Err(e) => eprintln!("Error inserting fluxes: {}", e),
        }
    }
    // drop(results);
    // drop(gas_data)
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

pub fn export_sqlite_to_csv(
    db_path: &str,
    csv_path: &str,
    project: String,
) -> Result<(), Box<dyn Error>> {
    let conn = Connection::open(db_path)?;

    let query = make_select_all_fluxes();
    let mut stmt = conn.prepare(&query)?;
    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let column_count = column_names.len();

    let rows = stmt.query_map([&project], {
        let column_names = column_names.clone();
        move |row| {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let col_name = &column_names[i];
                let val = match row.get_ref(i)? {
                    ValueRef::Null => "".to_string(),
                    ValueRef::Integer(ts) => {
                        if col_name == "start_time" {
                            if let Some(dt) = DateTime::<Utc>::from_timestamp(ts, 0) {
                                dt.format("%Y-%m-%d %H:%M:%S").to_string()
                            } else {
                                ts.to_string()
                            }
                        } else {
                            ts.to_string()
                        }
                    },
                    ValueRef::Real(f) => f.to_string(),
                    ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                    ValueRef::Blob(_) => "[BLOB]".to_string(),
                };
                values.push(val);
            }
            Ok(values)
        }
    })?;

    let file = File::create(Path::new(csv_path))?;
    let mut wtr = Writer::from_writer(file);

    wtr.write_record(&column_names)?;

    for row in rows {
        wtr.write_record(&row?)?;
    }

    wtr.flush()?;
    Ok(())
}

fn render_recalculate_ui(
    ui: &mut Ui,
    runtime: &tokio::runtime::Runtime,
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
    project: String,
    instrument_serial: String,
    log_messages: &mut VecDeque<String>,
) {
    ui.vertical(|ui| {
        ui.label("Compare the current chamber volume of all calculated fluxes and recalculate if a new one is found.");
        ui.label("Only changes the fluxes and volume, no need to redo manual validation.");

        if ui.button("Recalculate.").clicked() {

            let mut conn = match Connection::open("fluxrs.db") {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to open database: {}", e);
                    log_messages.push_front("Failed to open database.".to_string());
                    return;
                },
            };

            let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
            match (
                load_fluxes(&mut conn, start_date, end_date, project.clone(), instrument_serial.clone(), progress_sender),
                query_volume(&conn, start_date, end_date,project.clone()),
            ) {
                (Ok(mut cycles), Ok(volumes)) => {
                    println!("{}", volumes.volume.len());
                    if volumes.volume.is_empty() {
                        log_messages.push_front("No volume data loaded.".to_owned());
                        return;
                    }
                    runtime.spawn_blocking(move || {
                        for c in &mut cycles {
                            c.chamber_volume = volumes
                                .get_nearest_previous_volume(c.start_time.timestamp(), &c.chamber_id)
                                .unwrap_or(1.0);
                            c.compute_all_fluxes();
                        }

                        if let Ok(mut conn) = Connection::open("fluxrs.db") {
                            if let Err(e) = update_fluxes(&mut conn, &cycles, project) {
                                eprintln!("Flux update error: {}", e);
                            }
                        }
                    });
                },
                (Err(rusqlite::Error::InvalidQuery), Err(_)) => {
                    log_messages.push_front("No cycles found in db, have you initiated the data?".to_owned());
                },
                e => {
                    eprintln!("Error processing cycles: {:?}", e);
                    log_messages.push_front("Error processing cycles. Do you have cycles initiated?".to_string());
                }
            }
        }
    });
}

pub fn process_gas_files_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: String,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    // {
    //     let mut logs = log_messages.lock().unwrap();
    //     logs.push_front("Uploading gas data...".to_string());
    //
    // }
    // let _ progress_sender.send)

    let mut all_gas = GasData::new();

    for path in &selected_paths {
        let instrument = Li7810::default();
        match instrument.read_data_file(path) {
            Ok(data) => {
                if data.validate_lengths() && !data.any_col_invalid() {
                    let rows = data.datetime.len();
                    all_gas.datetime.extend(data.datetime);
                    all_gas.diag.extend(data.diag);
                    all_gas.instrument_model = data.instrument_model;
                    all_gas.instrument_serial = data.instrument_serial;

                    for (gas_type, values) in data.gas {
                        all_gas.gas.entry(gas_type).or_default().extend(values);
                    }

                    let _ = progress_sender
                        .send(ProcessEvent::ReadFileRows(path.to_str().unwrap().to_owned(), rows));
                }
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::ReadFileFail(
                    path.to_str().unwrap().to_owned(),
                    format!("{}", e),
                ));
            },
        }
    }

    match insert_measurements(conn, &all_gas, project) {
        Ok((count, duplicates)) => {
            let _ = progress_sender.send(ProcessEvent::InsertOkSkip(count, duplicates));
        },
        Err(e) => {
            let _ = progress_sender.send(ProcessEvent::InsertFail(format!("{}", e)));
        },
    }
}
fn upload_cycle_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: String,
    log_messages: Arc<Mutex<VecDeque<String>>>,
) {
    let mut log_msgs = log_messages.lock().unwrap();
    log_msgs.push_front("Uploading cycle data...".to_string());

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

                    log_msgs.push_front(format!("Successfully read {:?}", path));
                } else {
                    log_msgs.push_front(format!("Skipped file {:?}: Invalid data length", path));
                }
            },
            Err(e) => {
                log_msgs.push_front(format!("Failed to read file {:?}: {}", path, e));
            },
        }
    }
    match insert_cycles(conn, &all_times, project) {
        Ok((row_count, duplicates)) => {
            log_msgs.push_front(format!(
                "Successfully inserted {} rows into DB. Skipped {}.",
                row_count, duplicates
            ));
        },
        Err(e) => {
            log_msgs.push_front(format!("Failed to insert cycle data to db.Error {}", e));
        },
    }
}

fn upload_meteo_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: String,
    log_messages: Arc<Mutex<VecDeque<String>>>,
) {
    let mut log_msgs = log_messages.lock().unwrap();
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
                log_msgs.push_front(format!("Failed to read file {:?}: {}", path, e));
            },
        }
    }
    match insert_meteo_data(conn, &project, &meteos) {
        Ok(row_count) => {
            log_msgs.push_front(format!("Successfully inserted {} rows into DB.", row_count));
        },
        Err(e) => {
            log_msgs.push_front(format!("Failed to insert cycle data to db.Error {}", e));
        },
    }
    log_msgs.push_front("Uploading meteo data...".to_string());
}
// fn upload_volume_data_async(&mut self, selected_paths: Vec<PathBuf>, conn: &mut Connection) {
fn upload_volume_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: String,
    log_messages: Arc<Mutex<VecDeque<String>>>,
) {
    let mut log_msgs = log_messages.lock().unwrap();
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
                log_msgs.push_front(format!("Failed to read file {:?}: {}", path, e));
            },
        }
    }
    match insert_volume_data(conn, &project, &volumes) {
        Ok(row_count) => {
            log_msgs.push_front(format!("Successfully inserted {} rows into DB.", row_count));
        },
        Err(e) => {
            log_msgs.push_front(format!("Failed to insert cycle data to db.Error {}", e));
        },
    }
    log_msgs.push_front("Uploading meteo data...".to_string());
}
