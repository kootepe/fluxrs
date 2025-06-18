use crate::app_plotting::{
    init_attribute_plot, init_gas_plot, init_lag_plot, init_residual_bars,
    init_standardized_residuals_plot,
};
use crate::csv_parse;
use crate::cycle::{
    insert_flux_results, insert_fluxes_ignore_duplicates, load_cycles, process_cycles,
    update_fluxes,
};
use crate::cycle_navigator::CycleNavigator;
use crate::errorcode::ErrorCode;
use crate::flux::FluxKind;
use crate::fluxes_schema::{make_select_all_fluxes, OTHER_COLS};
use crate::gasdata::query_gas_async;
use crate::gasdata::{insert_measurements, GasData};
use crate::insert_cycles;
use crate::instruments::InstrumentType;
use crate::instruments::{GasType, InstrumentConfig};
use crate::keybinds::{Action, KeyBindings};
use crate::meteodata::{insert_meteo_data, query_meteo_async, MeteoData};
use crate::processevent::{InsertEvent, ProcessEvent, ProgressEvent, QueryEvent, ReadEvent};
use crate::project_app::Project;
use crate::timedata::{query_cycles_async, TimeData};
use crate::volumedata::{insert_volume_data, query_volume, query_volume_async, VolumeData};
use crate::Cycle;
use crate::EqualLen;
use std::str::FromStr;
use tokio::sync::mpsc;

use eframe::egui::{Color32, Context, Label, Stroke, TextWrapMode, Ui};
use egui_file::FileDialog;
use egui_plot::{LineStyle, MarkerShape, PlotPoints, Polygon, VLine};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use csv::Writer;
use rusqlite::{params, types::ValueRef, Connection, Result, Row};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Clone, PartialEq)]
pub enum DataType {
    Gas,
    Cycle,
    Meteo,
    Volume,
}
impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DataType::Gas => write!(f, "Gas Data"),
            DataType::Cycle => write!(f, "Cycle data"),
            DataType::Meteo => write!(f, "Meteo data"),
            DataType::Volume => write!(f, "Volume data"),
        }
    }
}
// logs which item on the plot is being dragged
pub enum Adjuster {
    Left,
    Main,
    Right,
    OpenLag,
    CloseLag,
}

// how to find the flux calculation area
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Mode {
    AfterDeadband,
    BestPearsonsR,
}

impl FromStr for Mode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "deadband" => Ok(Mode::AfterDeadband),
            "pearsons" => Ok(Mode::BestPearsonsR),
            "bestr" => Ok(Mode::BestPearsonsR),
            _ => Err(()),
        }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Self::BestPearsonsR
    }
}

impl Mode {
    pub fn as_int(&self) -> u8 {
        match self {
            Mode::AfterDeadband => 1,
            Mode::BestPearsonsR => 2,
        }
    }
    pub fn from_int(i: u8) -> Option<Mode> {
        match i {
            1 => Some(Mode::AfterDeadband),
            2 => Some(Mode::BestPearsonsR),
            _ => None,
        }
    }
}
// Display trait for nicer labels in the ComboBox
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::AfterDeadband => write!(f, "After Deadband"),
            Mode::BestPearsonsR => write!(f, "Best Pearson's R"),
        }
    }
}
impl std::fmt::Debug for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::AfterDeadband => write!(f, "After Deadband"),
            Mode::BestPearsonsR => write!(f, "Best Pearson's R"),
        }
    }
}
type LoadResult = Arc<Mutex<Option<Result<Vec<Cycle>, rusqlite::Error>>>>;
type ProgReceiver = Option<tokio::sync::mpsc::UnboundedReceiver<ProcessEvent>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GasKey {
    pub gas_type: GasType,
    pub label: String,
}
impl fmt::Display for GasKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}, {}", self.gas_type, self.label)
    }
}
impl GasKey {
    /// Creates a new `GasKey`.
    pub fn new(gas_type: GasType, label: impl Into<String>) -> Self {
        Self { gas_type, label: label.into() }
    }

    /// Returns a reference to the gas type.
    pub fn gas_type(&self) -> &GasType {
        &self.gas_type
    }

    /// Returns a reference to the label.
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl From<(&GasType, &str)> for GasKey {
    fn from(tuple: (&GasType, &str)) -> Self {
        Self { gas_type: tuple.0.clone(), label: tuple.1.to_string() }
    }
}
pub struct ValidationApp {
    pub runtime: tokio::runtime::Runtime,
    pub init_enabled: bool,
    pub init_in_progress: bool,
    cycles_progress: usize,
    cycles_state: Option<(usize, usize)>,
    query_in_progress: bool,
    pub load_result: LoadResult,
    progress_receiver: ProgReceiver,
    pub task_done_sender: Sender<()>,
    pub task_done_receiver: Receiver<()>,
    pub enabled_gases: BTreeSet<GasKey>,
    pub enabled_calc_r: BTreeSet<GasKey>,

    pub enabled_lin_fluxes: BTreeSet<GasKey>,
    pub enabled_poly_fluxes: BTreeSet<GasKey>,
    pub enabled_roblin_fluxes: BTreeSet<GasKey>,

    pub enabled_lin_adj_r2: BTreeSet<GasKey>,
    pub enabled_lin_p_val: BTreeSet<GasKey>,
    pub enabled_lin_sigma: BTreeSet<GasKey>,
    pub enabled_lin_rmse: BTreeSet<GasKey>,
    pub enabled_lin_aic: BTreeSet<GasKey>,

    pub enabled_roblin_adj_r2: BTreeSet<GasKey>,
    pub enabled_roblin_sigma: BTreeSet<GasKey>,
    pub enabled_roblin_rmse: BTreeSet<GasKey>,
    pub enabled_roblin_aic: BTreeSet<GasKey>,
    //
    pub enabled_poly_sigma: BTreeSet<GasKey>,
    pub enabled_poly_adj_r2: BTreeSet<GasKey>,
    pub enabled_poly_rmse: BTreeSet<GasKey>,
    pub enabled_poly_aic: BTreeSet<GasKey>,

    // pub enabled_aic_diff: BTreeSet<GasKey>,
    pub enabled_measurement_rs: BTreeSet<GasKey>,
    pub enabled_conc_t0: BTreeSet<GasKey>,

    pub p_val_thresh: f32,
    pub rmse_thresh: f32,
    pub r2_thresh: f32,
    pub t0_thresh: f32,
    pub cycles: Vec<Cycle>,
    pub cycle_nav: CycleNavigator,
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
    pub keep_calc_constant_deadband: bool,
    pub projects: Vec<Project>,
    pub initiated: bool,
    pub selected_project: Option<Project>,
    pub show_linfit: bool,
    pub show_polyfit: bool,
    pub show_roblinfit: bool,
    pub calc_area_color: Color32,
    pub calc_area_adjust_color: Color32,
    pub calc_area_stroke_color: Color32,
    pub selected_model: FluxKind,
    pub keybinds: KeyBindings,
    pub awaiting_rebind: Option<Action>,
    pub show_cycle_details: bool,
    pub show_residuals: bool,
    pub show_standardized_residuals: bool,
    pub show_legend: bool,
    pub show_plot_widths: bool,
    pub toggled_gas: Option<GasKey>,
    pub dragging: Option<Adjuster>,
    pub mode: Mode,
    pub current_delta: f64,
    pub current_z_delta: f64,
    pub current_ydelta: f64,
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
            enabled_gases: BTreeSet::new(),
            enabled_lin_fluxes: BTreeSet::new(),
            enabled_lin_p_val: BTreeSet::new(),
            enabled_lin_sigma: BTreeSet::new(),
            enabled_lin_adj_r2: BTreeSet::new(),
            enabled_lin_aic: BTreeSet::new(),
            enabled_lin_rmse: BTreeSet::new(),
            enabled_roblin_fluxes: BTreeSet::new(),
            enabled_roblin_sigma: BTreeSet::new(),
            enabled_roblin_adj_r2: BTreeSet::new(),
            enabled_roblin_aic: BTreeSet::new(),
            enabled_roblin_rmse: BTreeSet::new(),
            enabled_poly_fluxes: BTreeSet::new(),
            enabled_poly_sigma: BTreeSet::new(),
            enabled_poly_adj_r2: BTreeSet::new(),
            enabled_poly_aic: BTreeSet::new(),
            enabled_poly_rmse: BTreeSet::new(),
            // enabled_aic_diff: BTreeSet::new(),
            enabled_measurement_rs: BTreeSet::new(),
            enabled_calc_r: BTreeSet::new(),
            enabled_conc_t0: BTreeSet::new(),

            p_val_thresh: 0.05,
            rmse_thresh: 25.,
            r2_thresh: 0.98,
            t0_thresh: 50000.,
            cycles: Vec::new(),
            cycle_nav: CycleNavigator::new(),
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
            selected_point: None,
            dragged_point: None,
            chamber_colors: HashMap::new(),
            visible_traces: HashMap::new(),
            all_traces: HashSet::new(),
            start_date: NaiveDate::from_ymd_opt(2022, 8, 14)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            end_date: NaiveDate::from_ymd_opt(2024, 8, 30)
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
            projects: Vec::new(),
            initiated: false,
            selected_project: None,
            show_linfit: true,
            show_polyfit: true,
            show_roblinfit: true,
            keep_calc_constant_deadband: true,
            calc_area_color: Color32::BLACK,
            calc_area_adjust_color: Color32::BLACK,
            calc_area_stroke_color: Color32::BLACK,
            selected_model: FluxKind::Linear,
            keybinds: KeyBindings::default(),
            awaiting_rebind: None,
            show_residuals: false,
            show_standardized_residuals: false,
            show_legend: true,
            show_cycle_details: true,
            show_plot_widths: true,
            toggled_gas: None,
            dragging: None,
            mode: Mode::default(),
            current_delta: 0.,
            current_z_delta: 0.,
            current_ydelta: 0.,
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

        // egui::Window::new("Select visible traces").max_width(50.).show(ctx, |ui| {
        if self.show_legend {
            egui::Window::new("Legend").title_bar(false).resizable(false).show(ctx, |ui| {
                self.render_legend(ui, &self.chamber_colors.clone());
            });
        }

        if self.show_cycle_details {
            egui::Window::new("Current Cycle details").show(ctx, |ui| {
                if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
                    let errors = ErrorCode::from_mask(cycle.error_code.0);
                    let error_messages: Vec<String> =
                        errors.iter().map(|error| error.to_string()).collect();

                    let main_gas = if let Some(gas) = self.get_project().main_gas {
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
                            if let Some(first_val) = cycle
                                .dt_v
                                .get(&self.selected_project.as_ref().unwrap().instrument_serial)
                                .unwrap()
                                .first()
                            {
                                ui.label(format!("{}", first_val.to_owned()));
                            } else {
                                ui.label("None");
                            }
                            ui.end_row();
                            ui.label("Last TS:");
                            if let Some(last_val) = cycle
                                .dt_v
                                .get(&self.selected_project.as_ref().unwrap().instrument_serial)
                                .unwrap()
                                .last()
                            {
                                ui.label(format!("{}", last_val.to_owned()));
                            } else {
                                ui.label("None");
                            }
                            ui.end_row();
                            ui.label("Close Offset:");
                            ui.label(cycle.close_offset.to_string());
                            ui.end_row();
                            ui.label("Close lag:");
                            ui.label(cycle.close_lag_s.to_string());
                            ui.end_row();
                            ui.label("Open Offset:");
                            ui.label(cycle.open_offset.to_string());
                            ui.end_row();
                            ui.label("Open lag:");
                            ui.label(cycle.open_lag_s.to_string());
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
                            ui.label(
                                match cycle.measurement_r2.get(
                                    &(GasKey::from((&main_gas, cycle.instrument_serial.as_str()))),
                                ) {
                                    Some(r) => format!("{:.6}", r),
                                    None => "N/A".to_string(),
                                },
                            );
                            ui.end_row();

                            if !error_messages.is_empty() {
                                ui.label("Errors:");
                                ui.label(error_messages.join("\n"));
                                ui.end_row();
                            }
                        });
                    });
                    ui.separator();

                    for model in &[FluxKind::Linear, FluxKind::Poly, FluxKind::RobLin] {
                        ui.heading(model.label()); // Or .to_string() if you don’t have label()

                        egui::Grid::new(format!("gas_values_grid_{:?}", model)).striped(true).show(
                            ui,
                            |ui| {
                                ui.label("Gas");
                                ui.label("Flux");
                                ui.label("Adj R²");
                                ui.label("p-value");
                                ui.label("Sigma");
                                ui.label("RMSE");
                                ui.label("AIC");
                                ui.end_row();

                                for gas in &self.enabled_gases {
                                    let flux = cycle
                                        .get_flux(gas.clone(), *model)
                                        .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                    let r2 = cycle
                                        .get_adjusted_r2(gas.clone(), *model)
                                        .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                    let p_val = cycle
                                        .get_p_value(gas.clone(), *model)
                                        .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                    let sigma = cycle
                                        .get_sigma(gas.clone(), *model)
                                        .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                    let rmse = cycle
                                        .get_rmse(gas.clone(), *model)
                                        .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                    let aic = cycle
                                        .get_aic(gas.clone(), *model)
                                        .map_or("N/A".to_string(), |v| format!("{:.6}", v));

                                    ui.label(format!("{}", gas.gas_type));
                                    ui.label(flux);
                                    ui.label(r2);
                                    ui.label(p_val);
                                    ui.label(sigma);
                                    ui.label(rmse);
                                    ui.label(aic);
                                    ui.end_row();
                                }
                            },
                        );

                        ui.separator();
                    }
                } else {
                    ui.label("No cycle selected.");
                }
            });
        }
        if self.show_plot_widths {
            egui::Window::new("Adjust plot widths").show(ctx, |ui| {
            ui.label("Drag boxes right/left or down/up to adjust plot sizes.");
            ui.label("Unfinished, flux plot dimensions also adjust all plots that are not gas or lag plot");
            egui::Grid::new("plots").show(ui, |ui| {
                ui.label("Lag plot width: ");
                ui.add(egui::DragValue::new(&mut self.lag_plot_w).speed(1.).range(150.0..=1920.0));
                ui.label("Flux plot width:");
                ui.add(egui::DragValue::new(&mut self.flux_plot_w).speed(1.).range(150.0..=1920.0));
                ui.label("Gas plot width:");
                ui.add(egui::DragValue::new(&mut self.gas_plot_w).speed(1.).range(150.0..=1920.0));
                ui.end_row();
                ui.label("Lag plot height:");
                ui.add(egui::DragValue::new(&mut self.lag_plot_h).speed(1.).range(150.0..=1920.0));
                ui.label("Flux plot height:");
                ui.add(egui::DragValue::new(&mut self.flux_plot_h).speed(1.).range(150.0..=1920.0));
                ui.label("Gas plot height:");
                ui.add(egui::DragValue::new(&mut self.gas_plot_h).speed(1.).range(150.0..=1920.0));
            });
        });
        }
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
        let mut show_poly_model = true;
        let mut show_roblin_model = true;
        let mut reload_gas = false;
        let mut keep_calc_area_constant_with_deadband = false;

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
                keep_calc_area_constant_with_deadband = ui
                    .checkbox(
                        &mut self.keep_calc_constant_deadband,
                        "Keep calculation area constant when incrementing deadband",
                    )
                    .clicked();
            });
            ui.vertical(|ui| {
                show_linear_model =
                    ui.checkbox(&mut self.show_linfit, "Show linear model").clicked();
                show_poly_model =
                    ui.checkbox(&mut self.show_polyfit, "Show polynomial model").clicked();
                show_roblin_model =
                    ui.checkbox(&mut self.show_roblinfit, "Show robust linear model").clicked();
            });

            ui.vertical(|ui| {
                if let Some(current_cycle) = self.cycle_nav.current_cycle(&self.cycles) {
                    for key in current_cycle.gases.clone() {
                        // let (target_gas, target_serial) = &key;
                        // Check if any model for this gas is valid (to toggle meaningfully)
                        let any_valid = current_cycle
                            .fluxes
                            .iter()
                            .any(|((g, _s), record)| g == &key && record.is_valid);
                        // .any(|(&(g, s, _), record)| (g, s) == key && record.is_valid);

                        let label = if any_valid {
                            format!("Invalidate {} measurement", key)
                        } else {
                            format!("Revalidate {} measurement", key)
                        };

                        if ui.button(label).clicked() {
                            self.toggled_gas = Some(key);
                        }
                    }
                }
            });

            // Toggle validity for all models of the selected gas type
            if let Some(gas_type) = &self.toggled_gas {
                if let Some(current_cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                    for ((g, _), record) in current_cycle.fluxes.iter_mut() {
                        if *g == *gas_type {
                            record.is_valid = !record.is_valid;
                        }
                    }
                }
                self.toggled_gas = None;
                self.mark_dirty();
            }
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

        if !ui.ctx().wants_keyboard_input() {
            ui.input(|i| {
                let modifiers = i.modifiers;
                for event in &i.raw.events {
                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::ToggleShowInvalids,
                        modifiers,
                    ) {
                        self.show_invalids = !self.show_invalids;
                        show_invalids_clicked = true;
                    }
                    if keybind_triggered(event, &self.keybinds, Action::ToggleShowValids, modifiers)
                    {
                        self.show_valids = !self.show_valids;
                        show_valids_clicked = true;
                    }
                    if keybind_triggered(event, &self.keybinds, Action::ToggleShowBad, modifiers) {
                        self.show_bad = !self.show_bad;
                        show_bad = true;
                    }
                    if keybind_triggered(event, &self.keybinds, Action::ToggleShowLegend, modifiers)
                    {
                        self.show_legend = !self.show_legend;
                    }
                    if keybind_triggered(event, &self.keybinds, Action::ToggleValidity, modifiers) {
                        toggle_valid = true;
                    }
                    if keybind_triggered(event, &self.keybinds, Action::NextCycle, modifiers) {
                        next_clicked = true;
                    }
                    if keybind_triggered(event, &self.keybinds, Action::PreviousCycle, modifiers) {
                        prev_clicked = true;
                    }
                    if keybind_triggered(event, &self.keybinds, Action::ToggleBad, modifiers) {
                        mark_bad = true;
                    }
                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::TogglePlotWidthsWindow,
                        modifiers,
                    ) {
                        self.show_plot_widths = !self.show_plot_widths;
                    }
                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::ZoomToMeasurement,
                        modifiers,
                    ) {
                        if self.zoom_to_measurement == 2 {
                            self.zoom_to_measurement = 0
                        } else {
                            self.zoom_to_measurement += 1;
                        }
                    }
                    if keybind_triggered(event, &self.keybinds, Action::ResetCycle, modifiers) {
                        reset_cycle = true;
                    }
                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::ToggleShowDetails,
                        modifiers,
                    ) {
                        self.show_cycle_details = !self.show_cycle_details
                    }
                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::ToggleShowResiduals,
                        modifiers,
                    ) {
                        self.show_residuals = !self.show_residuals
                    }
                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::ToggleShowStandResiduals,
                        modifiers,
                    ) {
                        self.show_standardized_residuals = !self.show_standardized_residuals
                    }

                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::ToggleCH4Validity,
                        modifiers,
                    ) {
                        if let Some(current_cycle) =
                            self.cycle_nav.current_cycle_mut(&mut self.cycles)
                        {
                            for ((g, _), record) in current_cycle.fluxes.iter_mut() {
                                if g.gas_type == GasType::CH4 {
                                    record.is_valid = !record.is_valid;
                                }
                            }
                            self.mark_dirty();
                        }
                    }
                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::IncrementDeadband,
                        modifiers,
                    ) {
                        self.mark_dirty();
                        if self.keep_calc_constant_deadband {
                            self.increment_deadband_constant_calc(1.);
                        } else {
                            self.increment_deadband(1.);
                        }
                        self.update_plots();
                    }
                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::DecrementDeadband,
                        modifiers,
                    ) {
                        self.mark_dirty();
                        if self.keep_calc_constant_deadband {
                            self.increment_deadband_constant_calc(-1.);
                        } else {
                            self.increment_deadband(-1.);
                        }
                        self.update_plots();
                    }
                    if keybind_triggered(event, &self.keybinds, Action::DecrementLag, modifiers) {
                        self.mark_dirty();
                        if self.zoom_to_measurement == 1 || self.zoom_to_measurement == 0 {
                            self.increment_open_lag(-1.);
                        }
                        if self.zoom_to_measurement == 2 {
                            self.increment_close_lag(-1.);
                        }
                        if self.mode_pearsons() {
                            self.set_all_calc_range_to_best_r();
                        }
                        self.update_plots();
                    }
                    if keybind_triggered(event, &self.keybinds, Action::IncrementLag, modifiers) {
                        self.mark_dirty();
                        if self.zoom_to_measurement == 1 || self.zoom_to_measurement == 0 {
                            self.increment_open_lag(1.);
                        }
                        if self.zoom_to_measurement == 2 {
                            self.increment_close_lag(1.);
                        }
                        if self.mode_pearsons() {
                            self.set_all_calc_range_to_best_r();
                        }
                        self.update_plots();
                    }

                    if keybind_triggered(event, &self.keybinds, Action::SearchLag, modifiers) {
                        self.mark_dirty();
                        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                            cycle.search_new_open_lag(GasKey::from((
                                &cycle.main_gas,
                                cycle.instrument_serial.as_str(),
                            )));
                            self.update_plots();
                        }
                    }

                    if keybind_triggered(
                        event,
                        &self.keybinds,
                        Action::SearchLagPrevious,
                        modifiers,
                    ) {
                        if let Some(current_visible_idx) = self.cycle_nav.current_index() {
                            if current_visible_idx > 0 {
                                let chamber_id =
                                    self.cycles[current_visible_idx].chamber_id.clone();
                                let (before, after) = self.cycles.split_at_mut(current_visible_idx);
                                let current_cycle = &mut after[0];

                                // find previous cycle which is valid and has the same chamber id
                                if let Some(previous_cycle) = before
                                    .iter()
                                    .rev()
                                    .find(|cycle| cycle.chamber_id == chamber_id && cycle.is_valid)
                                {
                                    let target = current_cycle.start_time
                                        + chrono::TimeDelta::seconds(current_cycle.open_offset)
                                        + chrono::TimeDelta::seconds(
                                            previous_cycle.open_lag_s as i64,
                                        );

                                    let Some(main_gas) =
                                        self.selected_project.as_ref().unwrap().main_gas
                                    else {
                                        eprintln!("No main gas selected!");
                                        return;
                                    };

                                    current_cycle.get_peak_near_timestamp(
                                        &GasKey::from((
                                            &main_gas,
                                            current_cycle.instrument_serial.as_str(),
                                        )),
                                        target.timestamp(),
                                    );

                                    self.mark_dirty();
                                    self.update_plots();
                                }
                            }
                        }
                    }
                }
            });
        }
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
            self.reset_cycle();
            self.update_plots();
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

        if self.cycles.is_empty() {
            ui.label("No cycles loaded");
            return;
        }

        if self.cycle_nav.visible_count() == 0 {
            ui.label("All cycles hidden.");
            return;
        }
        let main_key = if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            (GasKey::from((&cycle.main_gas, cycle.instrument_serial.as_str())))
        } else {
            return;
        };

        if self.enabled_gases.is_empty() {
            self.enabled_gases.insert(main_key.clone());
        }

        if ctx.style().visuals.dark_mode {
            self.calc_area_color = Color32::from_rgba_unmultiplied(255, 255, 255, 1);
            self.calc_area_adjust_color = Color32::from_rgba_unmultiplied(255, 255, 255, 20);
            self.calc_area_stroke_color = Color32::from_rgba_unmultiplied(255, 255, 255, 60);
        } else {
            self.calc_area_color = Color32::from_rgba_unmultiplied(0, 0, 0, 10);
            self.calc_area_adjust_color = Color32::from_rgba_unmultiplied(0, 0, 20, 20);
            self.calc_area_stroke_color = Color32::from_rgba_unmultiplied(0, 0, 0, 90);
        }

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    if self.zoom_to_measurement == 2 {
                        self.should_reset_bounds = true;
                    }
                    for key in self.enabled_gases.clone() {
                        if self.is_gas_enabled(&key) {
                            let gas_plot = init_gas_plot(
                                &key,
                                self.get_start(),
                                self.get_end(),
                                self.gas_plot_w,
                                self.gas_plot_h,
                            );
                            let response =
                                gas_plot.show(ui, |plot_ui| self.render_gas_plot_ui(plot_ui, &key));
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
                if !self.enabled_lin_fluxes.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_lin_fluxes.clone() {
                            let flux_plot = init_attribute_plot(
                                "flux".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let selected_model = self.selected_model;
                            let response = flux_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), selected_model))
                                            .and_then(|model| model.model.flux())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", selected_model.label()),
                                    Some(MarkerShape::Circle),
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                // Hide cursor
                                // println!("Gas plot is hovered!");
                            }
                        }
                    });
                }
                if !self.enabled_poly_fluxes.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_poly_fluxes.clone() {
                            let poly_flux_plot = init_attribute_plot(
                                "Poly Flux".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = poly_flux_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Poly))
                                            .and_then(|model| model.model.flux())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Poly.label()),
                                    Some(MarkerShape::Square),
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                // Hide cursor
                                // println!("Gas plot is hovered!");
                            }
                        }
                    });
                }
                if !self.enabled_roblin_fluxes.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_roblin_fluxes.clone() {
                            let roblin_flux_plot = init_attribute_plot(
                                "RobLin Flux".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = roblin_flux_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::RobLin))
                                            .and_then(|model| model.model.flux())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::RobLin.label()),
                                    Some(MarkerShape::Diamond),
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                // Hide cursor
                                // println!("Gas plot is hovered!");
                            }
                        }
                    });
                }
                if !self.enabled_lin_p_val.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_lin_p_val.clone() {
                            let lin_p_val_plot = init_attribute_plot(
                                "Linear p-value".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = lin_p_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Linear))
                                            .and_then(|model| model.model.p_value())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Linear.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                // Hide cursor
                                // println!("Gas plot is hovered!");
                            }
                        }
                    });
                }
                if !self.enabled_measurement_rs.is_empty() {
                    ui.vertical(|ui| {
                        for key in self.enabled_measurement_rs.clone() {
                            let measurement_r_plot = init_attribute_plot(
                                "Measurement r2".to_owned(),
                                &key,
                                self.measurement_r_plot_w,
                                self.measurement_r_plot_h,
                            );
                            // ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            let response = measurement_r_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    |cycle, gas_type| {
                                        *cycle.measurement_r2.get(gas_type).unwrap_or(&0.0)
                                    },
                                    "Measurement r",
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                // Hide cursor
                                // println!("Gas plot is hovered!");
                            }
                        }
                    });
                }
                if !self.enabled_calc_r.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_calc_r.clone() {
                            let selected_model = self.selected_model;
                            let calc_r_plot = init_attribute_plot(
                                format!("{} r2", selected_model),
                                key,
                                self.calc_r_plot_w,
                                self.calc_r_plot_h,
                            );
                            let response = calc_r_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), selected_model))
                                            .and_then(|model| model.model.r2())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", selected_model.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                // Hide cursor
                                // println!("Gas plot is hovered!");
                            }
                        }
                    });
                }
                if !self.enabled_conc_t0.is_empty() {
                    ui.vertical(|ui| {
                        for key in self.enabled_conc_t0.clone() {
                            let conc_plot = init_attribute_plot(
                                "Concentration t0".to_owned(),
                                &key,
                                self.conc_t0_plot_w,
                                self.conc_t0_plot_h,
                            );
                            let response = conc_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    |cycle, gas_type| {
                                        *cycle.t0_concentration.get(gas_type).unwrap_or(&0.0)
                                    },
                                    "Conc t0",
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_lin_adj_r2.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_lin_adj_r2.clone() {
                            let adj_r2_val_plot = init_attribute_plot(
                                "Lin adjusted r2".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = adj_r2_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Linear))
                                            .and_then(|model| model.model.adj_r2())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Linear.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_lin_sigma.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_lin_sigma.clone() {
                            let sigma_plot = init_attribute_plot(
                                "Lin sigma".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = sigma_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Linear))
                                            .and_then(|model| model.model.sigma())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Linear.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_lin_aic.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_lin_aic.clone() {
                            let lin_aic_plot = init_attribute_plot(
                                "Lin AIC".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = lin_aic_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Linear))
                                            .and_then(|model| model.model.aic())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Linear.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_lin_rmse.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_lin_rmse.clone() {
                            let lin_rmse_plot = init_attribute_plot(
                                "Lin RMSE".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = lin_rmse_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Linear))
                                            .and_then(|model| model.model.rmse())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Linear.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_poly_adj_r2.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_poly_adj_r2.clone() {
                            let adj_r2_val_plot = init_attribute_plot(
                                "Poly adjusted r2".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = adj_r2_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Poly))
                                            .and_then(|model| model.model.adj_r2())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Poly.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_poly_sigma.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_poly_sigma.clone() {
                            let sigma_plot = init_attribute_plot(
                                "Poly sigma".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = sigma_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Poly))
                                            .and_then(|model| model.model.sigma())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Poly.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_poly_aic.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_poly_aic.clone() {
                            let poly_aic_plot = init_attribute_plot(
                                "Poly AIC".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = poly_aic_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Poly))
                                            .and_then(|model| model.model.aic())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Poly.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_poly_rmse.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_poly_rmse.clone() {
                            let poly_rmse_plot = init_attribute_plot(
                                "Poly RMSE".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = poly_rmse_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Poly))
                                            .and_then(|model| model.model.rmse())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Poly.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_roblin_adj_r2.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_roblin_adj_r2.clone() {
                            let adj_r2_val_plot = init_attribute_plot(
                                "Roblin Adjusted r2".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = adj_r2_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::RobLin))
                                            .and_then(|model| model.model.adj_r2())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::RobLin.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_roblin_sigma.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_roblin_sigma.clone() {
                            let sigma_plot = init_attribute_plot(
                                "RobLin sigma".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = sigma_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::RobLin))
                                            .and_then(|model| model.model.sigma())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::RobLin.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_roblin_aic.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_roblin_aic.clone() {
                            let roblin_aic_plot = init_attribute_plot(
                                "RobLin AIC".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = roblin_aic_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::RobLin))
                                            .and_then(|model| model.model.aic())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::RobLin.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.enabled_roblin_rmse.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_roblin_rmse.clone() {
                            let roblin_rmse_plot = init_attribute_plot(
                                "RobLin RMSE".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = roblin_rmse_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    &key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::RobLin))
                                            .and_then(|model| model.model.rmse())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::RobLin.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }

                // if !self.enabled_aic_diff.is_empty() {
                //     ui.vertical(|ui| {
                //         for key in &self.enabled_aic_diff.clone() {
                //             let aic_diff_plot = init_attribute_plot(
                //                 "".to_owned(),
                //                 key,
                //                 self.flux_plot_w,
                //                 self.flux_plot_h,
                //             );
                //             let response = aic_diff_plot.show(ui, |plot_ui| {
                //                 self.render_attribute_plot(
                //                     plot_ui,
                //                     &key,
                //                     move |cycle, key| {
                //                         cycle
                //                             .fluxes
                //                             .get(&(key.clone(), FluxKind::RobLin))
                //                             .and_then(|model| model.model.aic())
                //                             .unwrap_or(0.0)
                //                             - cycle
                //                                 .fluxes
                //                                 .get(&(key.clone(), FluxKind::Linear))
                //                                 .and_then(|model| model.model.aic())
                //                                 .unwrap_or(0.0)
                //                     },
                //                     &format!("Flux ({})", FluxKind::Poly.label()),
                //                     None,
                //                 );
                //             });
                //             if response.response.hovered() {
                //                 ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                //             }
                //         }
                //     });
                // }
            });
        });
        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            ui.horizontal(|ui| {
                let lag_plot = init_lag_plot(&main_key, self.lag_plot_w, self.lag_plot_h);
                let response = lag_plot.show(ui, |plot_ui| {
                    self.render_lag_plot(plot_ui);
                });
                for gas_type in self.enabled_gases.clone() {
                    let flux_plot = init_attribute_plot(
                        "Best flux".to_owned(),
                        &gas_type,
                        self.flux_plot_w,
                        self.flux_plot_h,
                    );
                    let response2 = flux_plot.show(ui, |plot_ui| {
                        self.render_best_flux_plot(plot_ui, &gas_type, |cycle, gas| {
                            cycle.best_flux_by_aic(gas).unwrap_or(f64::NAN)
                        });
                    });
                    if response.response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                    }
                    if response2.response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                    }
                }
            });
        });
        if self.show_residuals {
            egui::Window::new("Residual bar plots").show(ctx, |ui| {
                ui.vertical(|ui| {
                    for model in FluxKind::all() {
                        ui.horizontal(|ui| {
                            for gas in &self.enabled_gases {
                                let residual_bars = init_residual_bars(gas, *model, 250., 145.);
                                let response = residual_bars.show(ui, |plot_ui| {
                                    self.render_residual_bars(plot_ui, gas, *model);
                                });
                                if response.response.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                }
                            }
                        });
                    }
                });
            });
        }

        if self.show_standardized_residuals {
            egui::Window::new("Standardized Residuals").show(ctx, |ui| {
                ui.vertical(|ui| {
                    for model in &[FluxKind::Linear, FluxKind::Poly, FluxKind::RobLin] {
                        ui.horizontal(|ui| {
                            for gas in &self.enabled_gases {
                                let residual_plot_stand =
                                    init_standardized_residuals_plot(gas, *model, 250., 145.);
                                let response = residual_plot_stand.show(ui, |plot_ui| {
                                    self.render_standardized_residuals_plot(plot_ui, gas, *model);
                                });
                                if response.response.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                                }
                            }
                        });
                    }
                });
            });
        }
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
                let project = self.get_project().clone();
                let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
                self.progress_receiver = Some(progress_receiver);

                self.init_enabled = false;
                self.init_in_progress = true;

                self.runtime.spawn(async move {
                    let result = match Connection::open("fluxrs.db") {
                        Ok(conn) => {
                            load_cycles(&conn, &project, start_date, end_date, progress_sender)
                        },
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
                    let project = self.get_project().clone();
                    let instrument_serial = self.get_project().instrument_serial.clone();

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
                                let _ = progress_sender
                                    .send(ProcessEvent::Query(QueryEvent::QueryComplete));
                                if !times.start_time.is_empty() && !gas_data.is_empty() {
                                    run_processing_dynamic(
                                        times,
                                        gas_data,
                                        meteo_data,
                                        volume_data,
                                        project.clone(),
                                        arc_conn.clone(),
                                        progress_sender,
                                    )
                                    .await;
                                    let _ = sender.send(());
                                } else {
                                    // let _ = progress_sender.send(ProcessEvent::Query(
                                    //     QueryEvent::NoGasData("No data available".into()),
                                    // ));
                                    let _ = progress_sender.send(ProcessEvent::Done(Err(
                                        "No data available.".to_owned(),
                                    )));
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
                self.get_project().clone(),
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
        if self.init_in_progress || !self.init_enabled {
            ui.add(egui::Spinner::new());
            ui.label("Reading files.");
        }
        if let Some(project) = self.selected_project.as_mut() {
            let current_value = project.upload_from.unwrap_or(project.instrument); // fallback display value

            egui::ComboBox::from_label("Instrument")
                .selected_text(current_value.to_string())
                .show_ui(ui, |ui| {
                    for instrument in InstrumentType::available_instruments() {
                        let selected = Some(instrument) == project.upload_from;
                        if ui.selectable_label(selected, instrument.to_string()).clicked() {
                            project.upload_from = Some(instrument);
                        }
                    }
                });
        }

        let btns_disabled = self.init_enabled && !self.init_in_progress;
        ui.add_enabled(btns_disabled, |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                if ui.button("Select Gas Files").clicked() {
                    self.selected_data_type = Some(DataType::Gas);
                    self.open_file_dialog("Select Gas Files");
                }
                if ui.button("Select Cycle Files").clicked() {
                    self.selected_data_type = Some(DataType::Cycle);
                    self.open_file_dialog("Select Cycle Files");
                }
                if ui.button("Select Meteo Files").clicked() {
                    self.selected_data_type = Some(DataType::Meteo);
                    self.open_file_dialog("Select Meteo Files");
                }
                if ui.button("Select Volume Files").clicked() {
                    self.selected_data_type = Some(DataType::Volume);
                    self.open_file_dialog("Select Volume Files");
                }
            })
            .response
        });

        // Handle file selection
        self.handle_file_selection(ctx);

        self.log_display(ui);
    }

    pub fn handle_progress_messages(&mut self) {
        if let Some(receiver) = &mut self.progress_receiver {
            while let Ok(msg) = receiver.try_recv() {
                match msg {
                    ProcessEvent::Query(query_event) => match query_event {
                        QueryEvent::InitStarted => {
                            self.init_in_progress = true;
                        },
                        QueryEvent::InitEnded => {
                            self.init_in_progress = false;
                        },
                        QueryEvent::QueryComplete => {
                            self.query_in_progress = false;
                            self.log_messages.push_front("Finished queries.".to_owned());
                        },
                        QueryEvent::NoGasData(start_time) => {
                            self.log_messages.push_front(format!(
                                "No gas data found for cycle at {}",
                                start_time
                            ));
                        },
                        QueryEvent::NoGasDataDay(day) => {
                            self.log_messages
                                .push_front(format!("No gas data found for day {}", day));
                        },
                    },

                    ProcessEvent::Progress(progress_event) => match progress_event {
                        ProgressEvent::Rows(current, total) => {
                            self.cycles_state = Some((current, total));
                            self.cycles_progress += current;
                        },
                        ProgressEvent::Day(date) => {
                            self.log_messages.push_front(format!("Loaded cycles from {}", date));
                        },
                        ProgressEvent::NoGas(msg) => {
                            self.log_messages.push_front(format!("Gas missing: {}", msg));
                        },
                    },

                    ProcessEvent::Read(read_event) => match read_event {
                        ReadEvent::File(filename) => {
                            self.log_messages.push_front(format!("Read file: {}", filename));
                        },
                        ReadEvent::FileRows(filename, rows) => {
                            self.log_messages
                                .push_front(format!("Read file: {} with {} rows", filename, rows));
                        },
                        ReadEvent::FileFail(filename, e) => {
                            self.log_messages.push_front(format!(
                                "Failed to read file {}, error: {}",
                                filename, e
                            ));
                        },
                    },

                    ProcessEvent::Insert(insert_event) => match insert_event {
                        InsertEvent::Ok(rows) => {
                            self.log_messages.push_front(format!("Inserted {} rows", rows));
                        },
                        InsertEvent::OkSkip(rows, duplicates) => {
                            self.log_messages.push_front(format!(
                                "Inserted {} rows, skipped {} duplicates.",
                                rows, duplicates
                            ));
                        },
                        InsertEvent::Fail(e) => {
                            self.log_messages.push_front(format!("Failed to insert rows: {}", e));
                        },
                    },

                    // ProcessEvent::Error(e) | ProcessEvent::NoGasError(e) => {
                    //     self.log_messages.push_front(format!("Error: {}", e));
                    // },
                    ProcessEvent::Done(result) => {
                        match result {
                            Ok(()) => {
                                self.log_messages.push_front("All processing finished.".into());
                            },
                            Err(e) => {
                                self.log_messages
                                    .push_front(format!("Processing finished with error: {}", e));
                            },
                        }
                        self.cycles_progress = 0;
                        self.init_in_progress = false;
                        self.init_enabled = true;
                        self.query_in_progress = false;
                    },
                }
            }
        }
    }

    pub fn get_project(&self) -> &Project {
        self.selected_project.as_ref().unwrap()
    }
    pub fn get_project_mode(&self) -> Mode {
        self.selected_project.as_ref().unwrap().mode
    }
    pub fn mode_after_deadband(&self) -> bool {
        self.selected_project.as_ref().unwrap().mode == Mode::AfterDeadband
    }
    pub fn mode_pearsons(&self) -> bool {
        self.selected_project.as_ref().unwrap().mode == Mode::BestPearsonsR
    }

    fn open_file_dialog(&mut self, title: &str) {
        let mut dialog = FileDialog::open_file(self.initial_path.clone())
            .title(title)
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
                        self.process_files_async(
                            selected_paths,
                            self.selected_data_type.clone(),
                            self.get_project(),
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

    pub fn process_files_async(
        &self,
        path_list: Vec<PathBuf>,
        data_type: Option<DataType>,
        project: &Project,
        log_messages: Arc<Mutex<VecDeque<String>>>,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
        runtime: &tokio::runtime::Runtime,
    ) {
        let log_messages_clone = Arc::clone(&log_messages); // clone Arc for move
        let sender_clone = progress_sender.clone();
        let project_clone = project.clone();
        runtime.spawn(async move {
            let join_result =
                tokio::task::spawn_blocking(move || match Connection::open("fluxrs.db") {
                    Ok(mut conn) => {
                        if let Some(data_type) = data_type {
                            match data_type {
                                DataType::Gas => {
                                    let _ = progress_sender
                                        .send(ProcessEvent::Query(QueryEvent::InitStarted));
                                    upload_gas_data_async(
                                        path_list,
                                        &mut conn,
                                        &project_clone,
                                        progress_sender,
                                    )
                                },
                                DataType::Cycle => upload_cycle_data_async(
                                    path_list,
                                    &mut conn,
                                    &project_clone,
                                    progress_sender,
                                ),
                                DataType::Meteo => upload_meteo_data_async(
                                    path_list,
                                    &mut conn,
                                    &project_clone,
                                    progress_sender,
                                ),
                                DataType::Volume => upload_volume_data_async(
                                    path_list,
                                    &mut conn,
                                    &project_clone,
                                    progress_sender,
                                ),
                            }
                        }
                    },
                    Err(e) => {
                        let mut logs = log_messages.lock().unwrap();
                        logs.push_front(format!("Failed to connect to database: {}", e));
                    },
                })
                .await;
            if let Err(e) = join_result {
                let mut logs = log_messages_clone.lock().unwrap();

                let _ =
                    sender_clone.send(ProcessEvent::Done(Err("Thread join failure".to_owned())));
                logs.push_front(format!("Join error: {}", e));
            }
        });
    }

    pub fn dl_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.heading("Data downloader");
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        if ui.button("Download all calculated fluxes for current project.").clicked() {
            match export_sqlite_to_csv("fluxrs.db", "fluxrs.csv", self.get_project().name.clone()) {
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
            match export_sqlite_to_csv("fluxrs.db", "fluxrs.csv", self.get_project().name.clone()) {
                Ok(_) => println!("Succesfully downloaded csv."),
                Err(e) => println!("Failed to download csv. Error: {}", e),
            }
        }
        let mut checked = false;
        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.flux_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.r2_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.measurement_r2_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.calc_range_start_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
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
    pub fn enable_floaters(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label("Floating windows");
                ui.checkbox(&mut self.show_legend, "Show legend");
                ui.checkbox(&mut self.show_cycle_details, "Show cycle details");
                ui.checkbox(&mut self.show_plot_widths, "Show plot with adjustment");
                ui.checkbox(&mut self.show_residuals, "Show residuals distribution");
                ui.checkbox(&mut self.show_standardized_residuals, "Show standardized residuals");
            });
        });
    }
    pub fn render_measurement_plots(&mut self, ui: &mut egui::Ui) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let gases = cycle.gases.clone(); // Clone gases early!

            let mut main_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|key| (key.clone(), self.is_gas_enabled(key))).collect();

            let mut measurement_r_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|key| (key.clone(), self.is_measurement_r_enabled(key))).collect();

            let mut conc_t0_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|key| (key.clone(), self.is_conc_t0_enabled(key))).collect();

            let min_width = 100.;
            ui.vertical(|ui| {
                ui.label("General measurement plots");
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.label("Enable gases");
                    ui.vertical(|ui| {
                        for (gas, mut is_enabled) in &mut main_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_gases.insert(gas.clone());
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
                        ui.label("Cycle r2");
                        for (gas, mut is_enabled) in &mut measurement_r_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_measurement_rs.insert(gas.clone());
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
                        ui.label("t0 concentration");
                        for (gas, mut is_enabled) in &mut conc_t0_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_conc_t0.insert(gas.clone());
                                } else {
                                    self.enabled_conc_t0.remove(gas);
                                }
                            }
                        }
                    });
                });
                // ui.group(|ui| {
                //     ui.set_min_width(min_width); // Enforce group width here
                //     ui.vertical(|ui| {
                //         ui.label("AIC diff");
                //         for (gas, mut is_enabled) in &mut aic_diff_gases {
                //             if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                //                 if is_enabled {
                //                     self.enabled_aic_diff.insert(gas.clone());
                //                 } else {
                //                     self.enabled_aic_diff.remove(gas);
                //                 }
                //             }
                //         }
                //     });
                // });
            });
        } else {
            ui.label("Load data ");
        }
    }
    pub fn render_lin_plot_selection(&mut self, ui: &mut egui::Ui) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let gases = cycle.gases.clone(); // Clone gases early!

            let mut lin_flux_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_lin_flux_enabled(gas))).collect();
            let mut lin_p_val_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_lin_p_val_enabled(gas))).collect();
            let mut lin_adj_r2_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_lin_adj_r2_enabled(gas))).collect();
            let mut lin_sigma_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_lin_sigma_enabled(gas))).collect();
            let mut lin_rmse_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_lin_rmse_enabled(gas))).collect();
            let mut lin_aic_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_lin_aic_enabled(gas))).collect();

            let min_width = 150.;
            ui.vertical(|ui| {
                ui.add(Label::new("Linear model plots").wrap_mode(TextWrapMode::Truncate));
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Flux");
                        for (gas, mut is_enabled) in &mut lin_flux_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_lin_fluxes.insert(gas.clone());
                                } else {
                                    self.enabled_lin_fluxes.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Adjusted r2");
                        for (gas, mut is_enabled) in &mut lin_adj_r2_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_lin_adj_r2.insert(gas.clone());
                                } else {
                                    self.enabled_lin_adj_r2.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Sigma");
                        for (gas, mut is_enabled) in &mut lin_sigma_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_lin_sigma.insert(gas.clone());
                                } else {
                                    self.enabled_lin_sigma.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("AIC");
                        for (gas, mut is_enabled) in &mut lin_aic_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_lin_aic.insert(gas.clone());
                                } else {
                                    self.enabled_lin_aic.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("RMSE");
                        for (gas, mut is_enabled) in &mut lin_rmse_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_lin_rmse.insert(gas.clone());
                                } else {
                                    self.enabled_lin_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("p-value");
                        for (gas, mut is_enabled) in &mut lin_p_val_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_lin_p_val.insert(gas.clone());
                                } else {
                                    self.enabled_lin_p_val.remove(gas);
                                }
                            }
                        }
                    });
                });
            });
        } else {
            ui.label("No cycles.");
        }
    }
    pub fn render_roblin_plot_selection(&mut self, ui: &mut egui::Ui) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let gases = cycle.gases.clone(); // Clone gases early!

            let mut roblin_flux_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_roblin_flux_enabled(gas))).collect();
            let mut roblin_adj_r2_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_roblin_adj_r2_enabled(gas))).collect();
            let mut roblin_sigma_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_roblin_sigma_enabled(gas))).collect();
            let mut roblin_rmse_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_roblin_rmse_enabled(gas))).collect();
            let mut roblin_aic_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_roblin_aic_enabled(gas))).collect();

            let min_width = 150.;
            ui.vertical(|ui| {
                ui.add(Label::new("RobLin model plots").wrap_mode(TextWrapMode::Truncate));
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Flux");
                        for (gas, mut is_enabled) in &mut roblin_flux_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_roblin_fluxes.insert(gas.clone());
                                } else {
                                    self.enabled_roblin_fluxes.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Adjusted r2");
                        for (gas, mut is_enabled) in &mut roblin_adj_r2_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_roblin_adj_r2.insert(gas.clone());
                                } else {
                                    self.enabled_roblin_adj_r2.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Sigma");
                        for (gas, mut is_enabled) in &mut roblin_sigma_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_roblin_sigma.insert(gas.clone());
                                } else {
                                    self.enabled_roblin_sigma.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("AIC");
                        for (gas, mut is_enabled) in &mut roblin_aic_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_roblin_aic.insert(gas.clone());
                                } else {
                                    self.enabled_roblin_aic.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("RMSE");
                        for (gas, mut is_enabled) in &mut roblin_rmse_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_roblin_rmse.insert(gas.clone());
                                } else {
                                    self.enabled_roblin_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });
            });
        } else {
            ui.label("No cycles.");
        }
    }
    pub fn render_poly_plot_selection(&mut self, ui: &mut egui::Ui) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let gases = cycle.gases.clone(); // Clone gases early!

            let mut poly_flux_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_poly_flux_enabled(gas))).collect();
            let mut poly_adj_r2_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_poly_adj_r2_enabled(gas))).collect();
            let mut poly_sigma_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_poly_sigma_enabled(gas))).collect();
            let mut poly_rmse_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_poly_rmse_enabled(gas))).collect();
            let mut poly_aic_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_poly_aic_enabled(gas))).collect();

            let min_width = 150.;
            ui.vertical(|ui| {
                ui.group(|ui| {
                    ui.add(Label::new("Poly model plots").wrap_mode(TextWrapMode::Truncate));
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Flux");
                        for (gas, mut is_enabled) in &mut poly_flux_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_poly_fluxes.insert(gas.clone());
                                } else {
                                    self.enabled_poly_fluxes.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Adjusted r2");
                        for (gas, mut is_enabled) in &mut poly_adj_r2_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_poly_adj_r2.insert(gas.clone());
                                } else {
                                    self.enabled_poly_adj_r2.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Sigma");
                        for (gas, mut is_enabled) in &mut poly_sigma_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_poly_sigma.insert(gas.clone());
                                } else {
                                    self.enabled_poly_sigma.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("AIC");
                        for (gas, mut is_enabled) in &mut poly_aic_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_poly_aic.insert(gas.clone());
                                } else {
                                    self.enabled_poly_aic.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("RMSE");
                        for (gas, mut is_enabled) in &mut poly_rmse_gases {
                            if ui.checkbox(&mut is_enabled, format!("{}", gas)).changed() {
                                if is_enabled {
                                    self.enabled_poly_rmse.insert(gas.clone());
                                } else {
                                    self.enabled_poly_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });
            });
        } else {
            ui.label("No cycles.");
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

#[allow(clippy::too_many_arguments)]
pub fn create_polygon(
    start_x: f64,
    end_x: f64,
    min_y: f64,
    max_y: f64,
    color: Color32,
    stroke: Color32,
    id: &str,
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
    project: Project,
    conn: Arc<Mutex<rusqlite::Connection>>,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    let all_empty = gas_data.values().all(|g| g.datetime.is_empty());
    if all_empty {
        let _ = progress_sender.send(ProcessEvent::Done(Err("No data available".to_owned())));
        return;
    }

    let total_cycles = times.start_time.len();
    let gas_data_arc = Arc::new(gas_data);
    let mut time_chunks = VecDeque::from(times.chunk()); // ⬅️ chunk into ~250 cycles
    let mut active_tasks = Vec::new();

    let processed = 0;
    while !time_chunks.is_empty() || !active_tasks.is_empty() {
        // Fill up active tasks
        while active_tasks.len() < MAX_CONCURRENT_TASKS && !time_chunks.is_empty() {
            let chunk = time_chunks.pop_front().unwrap();
            let mut chunk_gas_data = HashMap::new();
            let mut missing_dates = Vec::new();

            for dt in &chunk.start_time {
                let date_str = dt.format("%Y-%m-%d").to_string();
                if let Some(data) = gas_data_arc.get(&date_str) {
                    chunk_gas_data.insert(date_str, data.clone());
                } else {
                    missing_dates.push(date_str);
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
                            let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
                        }
                        let count = result.iter().flatten().count();
                        let _ = progress_sender
                            .send(ProcessEvent::Progress(ProgressEvent::Rows(count, total_cycles)));
                        Ok(result)
                    },
                    Err(e) => {
                        let _ = progress_sender.send(ProcessEvent::Done(Err(e.to_string())));
                        Err(e)
                    },
                }
            });

            active_tasks.push(task);
        }

        // Wait for one task to finish
        let (result, _i, remaining_tasks) = futures::future::select_all(active_tasks).await;
        active_tasks = remaining_tasks; // assign back for next loop

        match result {
            Ok(Ok(cycles)) => {
                if !cycles.is_empty() {
                    let mut conn = conn.lock().unwrap();
                    match insert_fluxes_ignore_duplicates(&mut conn, &cycles, project.name.clone())
                    {
                        Ok((_, _)) => {
                            for cycle_opt in cycles.into_iter().flatten() {
                                // Lookup the inserted flux ID to associate flux results
                                let cycle_id: i64 = conn.query_row(
                                "SELECT id FROM fluxes
                                 WHERE instrument_serial = ?1 AND start_time = ?2 AND project_id = ?3",
                                params![
                                    cycle_opt.instrument_serial,
                                    cycle_opt.start_time.timestamp(),
                                    cycle_opt.project_name
                                ],
                                |row| row.get(0),
                            ).unwrap_or(-1);

                                if cycle_id >= 0 {
                                    if let Err(e) =
                                        insert_flux_results(&mut conn, cycle_id, cycle_opt.fluxes)
                                    {
                                        eprintln!("Error inserting flux results: {}", e);
                                    }
                                }
                            }
                        },
                        Err(e) => eprintln!("Error inserting fluxes: {}", e),
                    }
                }
            },
            Ok(Err(e)) => eprintln!("Cycle error: {e}"),
            Err(e) => eprintln!("Join error: {e}"),
        }
    }

    // Final insert (if you're collecting cycles earlier)
    let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
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
            for (i, col_name) in column_names.iter().enumerate() {
                let val = match row.get_ref(i)? {
                    ValueRef::Null => "".to_string(),
                    ValueRef::Integer(ts) => {
                        if col_name == "start_time" {
                            if let Some(dt) = DateTime::<Utc>::from_timestamp(ts, 0) {
                                dt.format("%Y-%m-%d %H:%M:%S").to_string()
                            } else {
                                ts.to_string()
                            }
                        } else if col_name == "gas" {
                            if let Some(gas) = GasType::from_int(ts as usize) {
                                format!("{}", gas)
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
    project: Project,
    log_messages: &mut VecDeque<String>,
) {
    ui.vertical(|ui| {
        ui.label("Compare the current chamber volume of all calculated fluxes and recalculate if a new one is found.");
        ui.label("Only changes the fluxes and volume, no need to redo manual validation.");

        if ui.button("Recalculate.").clicked() {

            let conn = match Connection::open("fluxrs.db") {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to open database: {}", e);
                    log_messages.push_front("Failed to open database.".to_string());
                    return;
                },
            };

            let (progress_sender, _progress_receiver) = mpsc::unbounded_channel();
            match (
                load_cycles(&conn, &project, start_date, end_date, progress_sender),
                query_volume(&conn, start_date, end_date,project.name.clone()),
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

pub fn upload_gas_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    for path in &selected_paths {
        let mut instrument = match project.instrument {
            InstrumentType::LI7810 => Some(InstrumentConfig::li7810()),
            InstrumentType::LI7820 => Some(InstrumentConfig::li7820()),
            InstrumentType::Other => None,
        };
        if let Some(upload_type) = project.upload_from {
            instrument = match upload_type {
                InstrumentType::LI7810 => Some(InstrumentConfig::li7810()),
                InstrumentType::LI7820 => Some(InstrumentConfig::li7820()),
                InstrumentType::Other => None,
            };
        }
        match instrument.clone().unwrap().read_data_file(path) {
            Ok(data) => {
                if data.validate_lengths() && !data.any_col_invalid() {
                    let rows = data.datetime.len();
                    println!("Loaded: {} from {}", path.to_string_lossy(), instrument.unwrap());
                    match insert_measurements(conn, &data, &project) {
                        Ok((count, duplicates)) => {
                            let _ = progress_sender
                                .send(ProcessEvent::Insert(InsertEvent::OkSkip(count, duplicates)));
                            let _ =
                                progress_sender.send(ProcessEvent::Query(QueryEvent::InitEnded));
                        },
                        Err(e) => {
                            println!("{}", e);
                            let _ = progress_sender
                                .send(ProcessEvent::Insert(InsertEvent::Fail(format!("{}", e))));
                        },
                    }

                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileRows(
                        path.to_str().unwrap().to_owned(),
                        rows,
                    )));
                }
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                    path.to_str().unwrap().to_owned(),
                    format!("{}", e),
                )));
            },
        }
    }
}
pub fn upload_cycle_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
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

                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::File(
                        path.to_string_lossy().to_string(),
                    )));
                } else {
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                        path.to_string_lossy().to_string(),
                        "Skipped, invalid data length".to_owned(),
                    )));
                }
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                    path.to_string_lossy().to_string(),
                    "Failed to read file {:?}: {}".to_owned(),
                )));
            },
        }
    }
    match insert_cycles(conn, &all_times, &project.name) {
        Ok((row_count, duplicates)) => {
            let _ = progress_sender
                .send(ProcessEvent::Insert(InsertEvent::OkSkip(row_count, duplicates)));
        },
        Err(e) => {
            let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(e.to_string())));
        },
    }
}

pub fn upload_meteo_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
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
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                    path.to_string_lossy().to_string(),
                    "Skipped, invalid data length".to_owned(),
                )));
            },
        }
    }
    match insert_meteo_data(conn, &project.name, &meteos) {
        Ok(row_count) => {
            let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Ok(row_count)));
        },
        Err(e) => {
            let msg = format!("Failed to insert cycle data to db.Error {}", e);
            let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(msg)));
        },
    }
}
pub fn upload_volume_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
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
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                    path.to_string_lossy().to_string(),
                    "Failed to read file {:?}: {}".to_owned(),
                )));
            },
        }
    }
    match insert_volume_data(conn, &project.name, &volumes) {
        Ok(row_count) => {
            let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Ok(row_count)));
        },
        Err(e) => {
            let msg = format!("Failed to insert cycle data to db.Error {}", e);
            let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(msg)));
        },
    }
}

pub fn keybind_triggered(
    event: &egui::Event,
    keybinds: &KeyBindings,
    action: Action,
    modifiers: egui::Modifiers,
) -> bool {
    if let Some(expected) = keybinds.key_for(action) {
        if let egui::Event::Key { key, pressed: true, .. } = event {
            return *key == expected.key
                && modifiers.ctrl == expected.ctrl
                && modifiers.shift == expected.shift
                && modifiers.alt == expected.alt;
        }
    }
    false
}
