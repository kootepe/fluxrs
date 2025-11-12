use crate::appview::AppState;
use crate::cycle::cycle::{
    insert_flux_results, insert_fluxes_ignore_duplicates, load_cycles_sync, process_cycles,
    update_fluxes,
};
use crate::cycle_navigator::CycleNavigator;
use crate::errorcode::ErrorCode;
use crate::flux::{FluxKind, FluxUnit};
use crate::ui::manage_proj::project_ui::get_or_insert_instrument;
use crate::ui::plotting_ui::{
    init_attribute_plot, init_gas_plot, init_lag_plot, init_residual_bars,
    init_standardized_residuals_plot,
};
use crate::ui::recalc::RecalculateApp;
use crate::ui::tz_picker::TimezonePickerState;
use crate::utils::{bad_message, ensure_utf8, good_message, warn_message};

use crate::data_formats::chamberdata::{
    insert_chamber_metadata, read_chamber_metadata, ChamberShape,
};
use crate::data_formats::gasdata::{insert_measurements, GasData};
use crate::data_formats::heightdata::{
    insert_height_data, query_height, read_height_csv, HeightData,
};
use crate::data_formats::meteodata::{insert_meteo_data, read_meteo_csv, MeteoData};
use crate::data_formats::timedata::{insert_cycles, try_all_formats, TimeData};
use crate::gastype::GasType;

use crate::cycle::cycle::Cycle;
use crate::instruments::instruments::InstrumentConfig;
use crate::instruments::instruments::InstrumentType;
use crate::keybinds::{Action, KeyBindings};
use crate::processevent::{
    self, InsertEvent, ProcessEvent, ProcessEventSink, ProgressEvent, QueryEvent, ReadEvent,
};
use crate::EqualLen;
use crate::Project;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver, UnboundedSender};

use eframe::egui::{Color32, Context, Label, RichText, Stroke, TextWrapMode, Ui};
use egui_file::FileDialog;
use egui_plot::{LineStyle, MarkerShape, PlotPoints, Polygon, VLine};

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Utc};
use chrono_tz::{Tz, UTC};
use rusqlite::{params, Connection, Result};
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

pub type InstrumentSerial = String;
pub type InstrumentId = i64;
// times: TimeData,
// gas_data:
// meteo_data: MeteoData,
// height_data: HeightData,
// chamber_data: HashMap<String, ChamberShape>,

#[derive(Clone, PartialEq, Debug)]
pub enum DataType {
    Gas,
    Cycle,
    Meteo,
    Height,
    Chamber,
}
impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DataType::Gas => write!(f, "Gas Data"),
            DataType::Cycle => write!(f, "Cycle data"),
            DataType::Meteo => write!(f, "Meteo data"),
            DataType::Height => write!(f, "Height data"),
            DataType::Chamber => write!(f, "Chamber metadat"),
        }
    }
}

impl DataType {
    pub fn type_str(&self) -> &'static str {
        match self {
            DataType::Gas => "gas",
            DataType::Cycle => "cycle",
            DataType::Meteo => "meteo",
            DataType::Height => "height",
            DataType::Chamber => "chamber_meta",
        }
    }
}
// logs which item on the plot is being dragged
pub enum Adjuster {
    None,
    Left,
    Main,
    Right,
    OpenLag,
    CloseLag,
}

impl Adjuster {
    pub fn is_dragged(&self) -> bool {
        !matches! {self, Adjuster::None}
    }
}
impl fmt::Display for Adjuster {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Adjuster::None => write!(f, "none"),
            Adjuster::Left => write!(f, "left"),
            Adjuster::Main => write!(f, "main"),
            Adjuster::Right => write!(f, "right"),
            Adjuster::OpenLag => write!(f, "open lag"),
            Adjuster::CloseLag => write!(f, "close lag"),
        }
    }
}

impl Default for Adjuster {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug)]
pub struct ParseModeError(String);

impl fmt::Display for ParseModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ParseModeError {}

// how to find the flux calculation area
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Mode {
    AfterDeadband,
    BestPearsonsR,
}

impl FromStr for Mode {
    type Err = ParseModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "deadband" => Ok(Mode::AfterDeadband),
            "pearsons" => Ok(Mode::BestPearsonsR),
            "bestr" => Ok(Mode::BestPearsonsR),
            other => Err(ParseModeError(format!("invalid mode: {other}"))),
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
type ProgReceiver = UnboundedReceiver<ProcessEvent>;
pub type ProgSender = UnboundedSender<ProcessEvent>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GasKey {
    pub gas_type: GasType,
    pub id: InstrumentId,
}
impl fmt::Display for GasKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}, {}", self.gas_type, self.id)
    }
}
impl GasKey {
    /// Creates a new `GasKey`.
    pub fn new(gas_type: GasType, id: impl Into<i64>) -> Self {
        Self { gas_type, id: id.into() }
    }

    /// Returns a reference to the gas type.
    pub fn gas_type(&self) -> &GasType {
        &self.gas_type
    }

    /// Returns a reference to the label.
    pub fn id(&self) -> &i64 {
        &self.id
    }
}
impl From<(&GasType, &i64)> for GasKey {
    fn from(tuple: (&GasType, &i64)) -> Self {
        Self { gas_type: *tuple.0, id: *tuple.1 }
    }
}
pub struct ValidationApp {
    pub runtime: tokio::runtime::Runtime,
    pub prog_sender: ProgSender,
    pub prog_receiver: Option<ProgReceiver>,
    pub recalc: RecalculateApp,
    pub init_enabled: bool,
    pub init_in_progress: bool,
    pub cycles_progress: usize,
    pub cycles_state: Option<(usize, usize)>,
    pub query_in_progress: bool,
    pub load_result: LoadResult,
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
    pub enabled_lin_cv: BTreeSet<GasKey>,
    pub enabled_lin_aic: BTreeSet<GasKey>,

    pub enabled_roblin_adj_r2: BTreeSet<GasKey>,
    pub enabled_roblin_sigma: BTreeSet<GasKey>,
    pub enabled_roblin_rmse: BTreeSet<GasKey>,
    pub enabled_roblin_cv: BTreeSet<GasKey>,
    pub enabled_roblin_aic: BTreeSet<GasKey>,
    //
    pub enabled_poly_sigma: BTreeSet<GasKey>,
    pub enabled_poly_adj_r2: BTreeSet<GasKey>,
    pub enabled_poly_rmse: BTreeSet<GasKey>,
    pub enabled_poly_cv: BTreeSet<GasKey>,
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
    pub start_date: DateTime<Tz>,
    pub end_date: DateTime<Tz>,
    pub opened_files: Option<Vec<PathBuf>>,
    pub open_file_dialog: Option<FileDialog>,
    pub initial_path: Option<PathBuf>,
    pub selected_data_type: Option<DataType>,
    pub log_messages: VecDeque<RichText>,
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
    pub dragging: Adjuster,
    pub mode: Mode,
    pub current_delta: f64,
    pub current_z_delta: f64,
    pub current_ydelta: f64,
    pub tz_prompt_open: bool,
    pub tz_state: TimezonePickerState,
    pub tz_for_files: Option<Tz>,
    pub flux_unit: FluxUnit,
}

impl Default for ValidationApp {
    fn default() -> Self {
        let (task_done_sender, task_done_receiver) = std::sync::mpsc::channel();
        let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
        Self {
            runtime: tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap(),
            recalc: RecalculateApp::new(),
            prog_receiver: Some(progress_receiver),
            prog_sender: progress_sender,
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
            enabled_lin_cv: BTreeSet::new(),
            enabled_roblin_fluxes: BTreeSet::new(),
            enabled_roblin_sigma: BTreeSet::new(),
            enabled_roblin_adj_r2: BTreeSet::new(),
            enabled_roblin_aic: BTreeSet::new(),
            enabled_roblin_rmse: BTreeSet::new(),
            enabled_roblin_cv: BTreeSet::new(),
            enabled_poly_fluxes: BTreeSet::new(),
            enabled_poly_sigma: BTreeSet::new(),
            enabled_poly_adj_r2: BTreeSet::new(),
            enabled_poly_aic: BTreeSet::new(),
            enabled_poly_rmse: BTreeSet::new(),
            enabled_poly_cv: BTreeSet::new(),
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
            start_date: UTC.with_ymd_and_hms(2024, 9, 30, 0, 0, 0).unwrap(),
            end_date: UTC.with_ymd_and_hms(2024, 9, 30, 23, 59, 59).unwrap(),
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
            dragging: Adjuster::default(),
            mode: Mode::default(),
            current_delta: 0.,
            current_z_delta: 0.,
            current_ydelta: 0.,
            tz_prompt_open: false,
            tz_state: TimezonePickerState::default(),
            tz_for_files: Some(UTC), // sensible default
            flux_unit: FluxUnit::default(),
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
                            ui.label(format!("{}", cycle.instrument.model));
                            ui.end_row();
                            ui.label("Serial:");
                            ui.label(&cycle.instrument.serial);
                            ui.end_row();
                            ui.label("Chamber:");
                            ui.label(cycle.chamber_id.to_string());
                            ui.end_row();
                            ui.label("Chamber height:");
                            ui.label(format!("{}", cycle.chamber_height));
                            ui.end_row();
                            ui.label("Chamber dimensions:");
                            ui.label(format!("{}", cycle.chamber));
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
                                .get(
                                    &self.selected_project.as_ref().unwrap().instrument.id.unwrap(),
                                )
                                .unwrap_or(&vec![])
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
                                .get(
                                    &self.selected_project.as_ref().unwrap().instrument.id.unwrap(),
                                )
                                .unwrap_or(&vec![])
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
                                    &(GasKey::from((&main_gas, &cycle.instrument.id.unwrap()))),
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
                                ui.label(format!("Flux {}", self.flux_unit));
                                ui.label("R²");
                                ui.label("CV");
                                ui.label("Sigma");
                                ui.label("RMSE");
                                ui.label("AIC");
                                ui.end_row();

                                for gas in &self.enabled_gases {
                                    let flux = if let Some(raw_flux) =
                                        cycle.get_flux(gas.clone(), *model)
                                    {
                                        let converted_flux =
                                            self.flux_unit.from_umol_m2_s(raw_flux, gas.gas_type);

                                        format!("{:.6}", converted_flux)
                                    } else {
                                        "N/A".to_string()
                                    };
                                    let r2 = cycle
                                        .get_r2(gas.clone(), *model)
                                        .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                    let cv = cycle
                                        .get_cv(gas.clone(), *model)
                                        .map_or("N/A".to_string(), |v| format!("{:.6}", v * 100.));
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
                                    ui.label(cv);
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
        egui::ComboBox::from_label("Select flux unit")
            .selected_text(format!("{}", self.flux_unit))
            .show_ui(ui, |ui| {
                for unit in FluxUnit::all() {
                    if ui.selectable_label(self.flux_unit == *unit, unit.to_string()).clicked() {
                        self.flux_unit = *unit;
                    }
                }
            });

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
                    // Group keys by their label
                    let mut label_map: BTreeMap<i64, Vec<_>> = BTreeMap::new();

                    for key in current_cycle.gases.clone() {
                        label_map.entry(key.id).or_default().push(key);
                    }

                    // Use a horizontal layout to make columns per label
                    ui.horizontal(|ui| {
                        for (_, keys) in label_map {
                            ui.vertical(|ui| {
                                for key in keys {
                                    let any_valid = current_cycle
                                        .fluxes
                                        .iter()
                                        .any(|((g, _s), record)| g == &key && record.is_valid);

                                    let button_label = if any_valid {
                                        format!(
                                            "Invalidate {} {}",
                                            key.gas_type,
                                            current_cycle.instruments.get(&key.id).unwrap().serial
                                        )
                                    } else {
                                        format!(
                                            "Revalidate {} {}",
                                            key.gas_type,
                                            current_cycle.instruments.get(&key.id).unwrap().serial
                                        )
                                    };

                                    if ui.button(button_label).clicked() {
                                        self.toggled_gas = Some(key.clone());
                                    }
                                }
                            });
                        }
                    });
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
                        let delta = -1.0;

                        match self.zoom_to_measurement {
                            0 => {
                                if self.mode_after_deadband() {
                                    self.increment_open_lag(delta);
                                }
                            },
                            1 => {
                                self.increment_open_lag(delta);
                            },
                            2 => {
                                self.increment_close_lag(delta);
                            },
                            _ => {},
                        }

                        self.stick_calc_to_range_start_for_all();

                        if self.mode_pearsons() {
                            self.set_all_calc_range_to_best_r();
                        }
                        self.update_plots();
                    }

                    // BUG: calc area doesnt stick to deadband when incrementing
                    if keybind_triggered(event, &self.keybinds, Action::IncrementLag, modifiers) {
                        self.mark_dirty();
                        let delta = 1.0;

                        match self.zoom_to_measurement {
                            1 => {
                                self.increment_open_lag(delta);
                            },
                            2 => {
                                self.increment_close_lag(delta);
                            },
                            0 => {
                                self.increment_open_lag(delta);
                            },
                            _ => {},
                        }

                        self.stick_calc_to_range_start_for_all();

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
                                &cycle.instrument.id.unwrap(),
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
                                            &current_cycle.instrument.id.unwrap(),
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
                // BUG: Marking as bads adds bad correctly but also marks the measurement as valid
                // cycle.toggle_manual_valid();
                cycle.error_code.toggle(ErrorCode::FailedMeasurement);

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
            self.zoom_to_measurement = 0;
            self.should_reset_bounds = true;
            self.commit_current_cycle();
            self.cycle_nav.step_back(); // Step to previous visible cycle
            if let Some(_index) = self.cycle_nav.current_index() {
                self.update_plots();
            }
        }

        if next_clicked {
            self.zoom_to_measurement = 0;
            self.should_reset_bounds = true;
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
            GasKey::from((&cycle.main_gas, &cycle.instrument.id.unwrap()))
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
                                    key,
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
                                    key,
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
                                    key,
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
                                    key,
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
                                    key,
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
                                    key,
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
                                    key,
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
                                    key,
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
                                    key,
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
                if !self.enabled_lin_cv.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_lin_cv.clone() {
                            let lin_cv_plot = init_attribute_plot(
                                "Lin cv".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = lin_cv_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Linear))
                                            .and_then(|model| model.model.cv())
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
                                    key,
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
                                    key,
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
                                    key,
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
                                    key,
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
                if !self.enabled_poly_cv.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_poly_cv.clone() {
                            let poly_cv_plot = init_attribute_plot(
                                "Poly cv".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = poly_cv_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::Poly))
                                            .and_then(|model| model.model.cv())
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
                                    key,
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
                                    key,
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
                                    key,
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
                                    key,
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
                if !self.enabled_roblin_cv.is_empty() {
                    ui.vertical(|ui| {
                        for key in &self.enabled_roblin_cv.clone() {
                            let roblin_cv_plot = init_attribute_plot(
                                "RobLin cv".to_owned(),
                                key,
                                self.flux_plot_w,
                                self.flux_plot_h,
                            );
                            let response = roblin_cv_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(key.clone(), FluxKind::RobLin))
                                            .and_then(|model| model.model.cv())
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
                let flux_unit = self.flux_unit;
                for gas_type in self.enabled_gases.clone() {
                    let flux_plot = init_attribute_plot(
                        format!("Best flux {}", flux_unit),
                        &gas_type,
                        self.flux_plot_w,
                        self.flux_plot_h,
                    );
                    let response2 = flux_plot.show(ui, |plot_ui| {
                        self.render_best_flux_plot(plot_ui, &gas_type, |cycle, gas| {
                            let umol_m2_s = cycle.best_flux_by_aic(gas).unwrap_or(f64::NAN);
                            flux_unit.from_umol_m2_s(umol_m2_s, gas.gas_type)
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

    pub fn date_picker(&mut self, ui: &mut egui::Ui) {
        let mut picker_start = self.start_date.date_naive();
        let mut picker_end = self.end_date.date_naive();
        let user_tz = self.selected_project.as_ref().unwrap().tz;

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
                    let naive = NaiveDateTime::from(picker_start);
                    let pick: DateTime<Tz> = user_tz.clone().from_local_datetime(&naive).unwrap();
                    self.start_date = pick;
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
                    let naive = NaiveDateTime::from(picker_end);
                    let pick: DateTime<Tz> = user_tz.clone().from_local_datetime(&naive).unwrap();
                    self.end_date = pick + TimeDelta::seconds(86399);
                }
            });
        });

        let start_before_end = self.start_date < self.end_date;
        let mut delta = TimeDelta::zero();
        let mut duration_str = String::new();

        if start_before_end {
            delta = self.end_date - self.start_date;

            if let Ok(duration) = delta.to_std() {
                let total_secs = duration.as_secs();

                let days = total_secs / 86_400;
                let hours = (total_secs % 86_400) / 3_600;
                let minutes = (total_secs % 3_600) / 60;
                let seconds = total_secs % 60;

                duration_str = if days > 0 {
                    format!("{}d {:02}h {:02}m {:02}s", days, hours, minutes, seconds)
                } else if hours > 0 {
                    format!("{:02}h {:02}m {:02}s", hours, minutes, seconds)
                } else if minutes > 0 {
                    format!("{:02}m {:02}s", minutes, seconds)
                } else {
                    format!("{:02}s", seconds)
                };
                ui.label(format!("From: {}", self.start_date));
                ui.label(format!("to: {}", self.end_date));

                ui.label(format!("Duration: {}", duration_str));

                // Buttons with full duration string
                if ui
                    .add_enabled(true, egui::Button::new(format!("Next ({})", duration_str)))
                    .clicked()
                {
                    self.start_date += delta;
                    self.end_date += delta;
                }

                if ui
                    .add_enabled(true, egui::Button::new(format!("Previous ({})", duration_str)))
                    .clicked()
                {
                    self.start_date -= delta;
                    self.end_date -= delta;
                }
            }
        }
    }

    pub fn log_display(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        if ui.button("Clear Log").clicked() {
            self.log_messages.clear();
        }
        ui.label("**Log Messages:**");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for message in &self.log_messages {
                ui.label(message.clone());
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
    pub fn handle_progress_messages(&mut self) {
        if let Some(mut receiver) = self.prog_receiver.take() {
            drain_progress_messages(self, &mut receiver);

            self.prog_receiver = Some(receiver);
        }
    }
    // pub fn handle_progress_messages(&mut self) {
    //     if let Some(receiver) = &mut self.progress_receiver {
    //         drain_progress_messages(self, receiver);
    //     }
    // }

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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
            let mut lin_cv_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_lin_cv_enabled(gas))).collect();
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                        ui.label("CV");
                        for (gas, mut is_enabled) in &mut lin_cv_gases {
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
                                if is_enabled {
                                    self.enabled_lin_cv.insert(gas.clone());
                                } else {
                                    self.enabled_lin_cv.remove(gas);
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
            let mut roblin_cv_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_roblin_cv_enabled(gas))).collect();
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
                                if is_enabled {
                                    self.enabled_roblin_rmse.insert(gas.clone());
                                } else {
                                    self.enabled_roblin_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("CV");
                        for (gas, mut is_enabled) in &mut roblin_cv_gases {
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
                                if is_enabled {
                                    self.enabled_roblin_cv.insert(gas.clone());
                                } else {
                                    self.enabled_roblin_cv.remove(gas);
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
            let mut poly_cv_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|gas| (gas.clone(), self.is_poly_cv_enabled(gas))).collect();
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
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
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
                                if is_enabled {
                                    self.enabled_poly_rmse.insert(gas.clone());
                                } else {
                                    self.enabled_poly_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("CV");
                        for (gas, mut is_enabled) in &mut poly_cv_gases {
                            if ui
                                .checkbox(
                                    &mut is_enabled,
                                    format!(
                                        "{} {}",
                                        gas.gas_type,
                                        cycle.instruments.get(&gas.id).unwrap().serial
                                    ),
                                )
                                .changed()
                            {
                                if is_enabled {
                                    self.enabled_poly_cv.insert(gas.clone());
                                } else {
                                    self.enabled_poly_cv.remove(gas);
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
    pub fn to_app_state(&self) -> AppState {
        AppState { start_date: self.start_date.to_utc(), end_date: self.end_date.to_utc() }
    }
}

impl ProcessEventSink for ValidationApp {
    fn on_query_event(&mut self, ev: &QueryEvent) {
        match ev {
            QueryEvent::InitStarted => {
                self.init_in_progress = true;
                self.recalc.calc_in_progress = true;
            },
            QueryEvent::InitEnded => {
                self.init_in_progress = false;
                self.recalc.calc_in_progress = false;
            },
            QueryEvent::QueryComplete => {
                // self.query_in_progress = false;
                self.log_messages.push_front(good_message("Finished queries."));
                self.recalc.query_in_progress = false;
            },
            QueryEvent::HeightFail(msg) => {
                self.log_messages.push_front(bad_message(msg));
            },
            QueryEvent::CyclesFail(msg) => {
                self.log_messages.push_front(bad_message(msg));
            },
            QueryEvent::DbFail(msg) => {
                self.log_messages.push_front(bad_message(msg));
            },
            QueryEvent::NoGasData(start_time) => {
                self.log_messages.push_front(bad_message(&format!(
                    "No gas data found for cycle at {}",
                    start_time
                )));
            },
            QueryEvent::NoGasDataDay(day) => {
                self.log_messages.push_front(bad_message(&format!(
                    "No gas data found for cycles at day {}",
                    day
                )));
            },
        }
    }

    fn on_progress_event(&mut self, ev: &ProgressEvent) {
        match ev {
            ProgressEvent::Rows(current, total) => {
                self.cycles_state = Some((*current, *total));
                self.cycles_progress += current;
                println!("Processed {} out of {} cycles", current, total);
            },
            ProgressEvent::Recalced(current, total) => {
                self.recalc.cycles_state = Some((*current, *total));
                self.recalc.cycles_progress += current;
                println!("Processed {} out of {} cycles", current, total);
            },
            ProgressEvent::CalculationStarted => {
                self.recalc.calc_enabled = false;
                self.recalc.calc_in_progress = true;
            },
            ProgressEvent::Day(date) => {
                self.log_messages.push_front(good_message(&format!("Loaded cycles from {}", date)));
            },
            ProgressEvent::NoGas(msg) => {
                self.log_messages.push_front(bad_message(&format!("Gas missing: {}", msg)));
            },
            ProgressEvent::Generic(msg) => {
                self.log_messages.push_front(good_message(&format!("{}", msg)));
            },
        }
    }

    fn on_read_event(&mut self, ev: &ReadEvent) {
        match ev {
            ReadEvent::File(filename) => {
                self.log_messages.push_front(good_message(&format!("Read file: {}", filename)));
            },
            ReadEvent::FileDetail(filename, detail) => {
                self.log_messages
                    .push_front(good_message(&format!("Read file: {} {}", filename, detail)));
            },
            ReadEvent::MeteoFail(filename, msg) => {
                self.log_messages.push_front(bad_message(&format!(
                    "Could not parse as meteo file: {}, {}",
                    filename, msg,
                )));
            },
            ReadEvent::HeightFail(filename, msg) => {
                self.log_messages.push_front(bad_message(&format!(
                    "Could not parse as height file: {}, {}",
                    filename, msg
                )));
            },
            ReadEvent::CycleFail(filename, msg) => {
                self.log_messages.push_front(bad_message(&format!(
                    "Could not parse as cycle file: {}, {}",
                    filename, msg
                )));
            },
            ReadEvent::GasFail(filename, msg) => {
                self.log_messages.push_front(bad_message(&format!(
                    "Could not parse as gas file: {}, {}",
                    filename, msg
                )));
            },
            ReadEvent::MetadataFail(filename, msg) => {
                self.log_messages.push_front(bad_message(&format!(
                    "Could not parse as chamber metadata file: {}, {}",
                    filename, msg
                )));
            },
            ReadEvent::FileRows(filename, rows) => {
                self.log_messages.push_front(good_message(&format!(
                    "Read file: {} with {} rows",
                    filename, rows
                )));
            },
            ReadEvent::RowFail(msg) => {
                self.log_messages.push_front(bad_message(&msg.to_owned()));
            },
            ReadEvent::FileFail(filename, e) => {
                self.log_messages.push_front(bad_message(&format!(
                    "Failed to read file {}, error: {}",
                    filename, e
                )));
            },
        }
    }

    fn on_insert_event(&mut self, ev: &InsertEvent) {
        match ev {
            InsertEvent::Ok(msg, rows) => {
                self.log_messages.push_front(good_message(&format!("{}{}", rows, msg)));
            },
            InsertEvent::OkSkip(rows, duplicates) => {
                if *duplicates == 0 {
                    self.log_messages.push_front(good_message(&format!(
                        "Inserted {} rows, skipped {} duplicates.",
                        rows, duplicates
                    )));
                } else {
                    self.log_messages.push_front(warn_message(&format!(
                        "Inserted {} rows, skipped {} duplicates.",
                        rows, duplicates
                    )));
                }
            },
            InsertEvent::CycleOkSkip(rows, skips) => {
                if skips == &0 {
                    self.log_messages
                        .push_front(good_message(&format!("Inserted {} cycles.", rows,)));
                } else {
                    self.log_messages.push_front(warn_message(&format!(
                        "Inserted {} cycles, skipped {} entries. Either something went wrong with the calculation or the cycles already exist in the db.",
                        rows, skips
                    )));
                }
            },
            InsertEvent::Fail(e) => {
                self.log_messages.push_front(bad_message(&format!("Failed to insert rows: {}", e)));
            },
        }
    }

    fn on_done(&mut self, res: &Result<(), String>) {
        match res {
            Ok(()) => {
                self.log_messages.push_front(good_message(&"All processing finished."));
            },
            Err(e) => {
                self.log_messages
                    .push_front(bad_message(&format!("Processing finished with error: {}", e)));
            },
        }

        self.cycles_progress = 0;
        self.init_in_progress = false;
        self.init_enabled = true;
        self.query_in_progress = false;
        self.recalc.calc_enabled = true;
        self.recalc.calc_in_progress = false;
        self.recalc.query_in_progress = false;
        self.recalc.cycles_progress = 0;
        self.recalc.cycles_state = None;
    }
}
pub fn is_inside_polygon(
    point: egui_plot::PlotPoint,
    start_x: f64,
    end_x: f64,
    min_y: f64,
    max_y: f64,
) -> bool {
    point.x >= start_x && point.x <= end_x
    // point.x >= start_x && point.x <= end_x && point.y >= min_y && point.y <= max_y
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

pub fn upload_gas_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    for path in &selected_paths {
        let mut instrument = match project.instrument.model {
            InstrumentType::LI7810 => Some(InstrumentConfig::li7810()),
            InstrumentType::LI7820 => Some(InstrumentConfig::li7820()),
        };
        if let Some(upload_type) = project.upload_from {
            instrument = match upload_type {
                InstrumentType::LI7810 => Some(InstrumentConfig::li7810()),
                InstrumentType::LI7820 => Some(InstrumentConfig::li7820()),
            };
        }
        if let Some(ref mut inst) = instrument {
            match inst.read_data_file(path) {
                Ok(data) => {
                    // inst.serial = Some(data.instrument.serial.clone());
                    if data.validate_lengths() {
                        let _rows = data.datetime.len();
                        let project_id = project.id.unwrap();

                        let file_name = match path.file_name().and_then(|n| n.to_str()) {
                            Some(name) => name,
                            None => {
                                eprintln!("Skipping path with invalid filename: {:?}", path);
                                // Optionally notify UI:
                                let _ =
                                    progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                                        path.to_string_lossy().to_string(),
                                        "Invalid file name (non-UTF8)".to_string(),
                                    )));
                                return (); // or `continue` if this is in a loop
                            },
                        };
                        let tx = match conn.transaction() {
                            Ok(tx) => tx,
                            Err(e) => {
                                eprintln!("Failed to start transaction: {}", e);
                                let _ = progress_sender.send(ProcessEvent::Insert(
                                    InsertEvent::Fail(format!(
                                        "Could not start transaction for '{}': {}",
                                        file_name, e
                                    )),
                                ));
                                continue;
                            },
                        };
                        let file_id = match get_or_insert_data_file(
                            &tx,
                            DataType::Gas,
                            file_name,
                            project_id,
                        ) {
                            Ok(id) => id,
                            Err(e) => {
                                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                                // Optionally notify UI
                                let _ =
                                    progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(
                                        format!("File '{}' skipped: {}", file_name, e),
                                    )));
                                continue; // or return if not inside a loop
                            },
                        };

                        match insert_measurements(&tx, &data, project, &file_id) {
                            Ok((count, duplicates)) => {
                                let _ = progress_sender.send(ProcessEvent::Insert(
                                    InsertEvent::OkSkip(count, duplicates),
                                ));
                                let _ = progress_sender.send(ProcessEvent::Read(
                                    ReadEvent::FileDetail(
                                        path.to_string_lossy().to_string(),
                                        format!("from {}", instrument.unwrap()),
                                    ),
                                ));
                                if let Err(e) = tx.commit() {
                                    eprintln!(
                                        "Failed to commit transaction for '{}': {}",
                                        file_name, e
                                    );
                                    let _ = progress_sender.send(ProcessEvent::Insert(
                                        InsertEvent::Fail(format!(
                                            "Commit failed for file '{}': {}",
                                            file_name, e
                                        )),
                                    ));
                                    // no commit = rollback
                                    continue;
                                }
                            },
                            Err(e) => {
                                println!("{}", e);
                                let _ = progress_sender.send(ProcessEvent::Insert(
                                    InsertEvent::Fail(format!("{}", e)),
                                ));
                            },
                        }
                    }
                },
                Err(e) => {
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                        path.to_str().unwrap().to_owned(),
                        e.to_string(),
                    )));
                },
            }
        }
        let _ = progress_sender.send(ProcessEvent::Query(QueryEvent::InitEnded));
    }
    let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
}
pub fn upload_cycle_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    let mut all_times = TimeData::new();

    for path in &selected_paths {
        if ensure_utf8(path).is_err() {
            let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                path.to_string_lossy().to_string(),
                "Invalid UTF-8, make sure your file is UTF-8 encoded.".to_owned(),
            )));
            continue;
        }
        let project_id = project.id.unwrap();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                // Optionally notify UI:
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                return (); // or `continue` if this is in a loop
            },
        };

        let file_id = match get_or_insert_data_file(conn, DataType::Cycle, file_name, project_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                // Optionally notify UI
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue; // or return if not inside a loop
            },
        };
        match try_all_formats(path, &tz, project, progress_sender.clone()) {
            //   Pass `path` directly
            Ok((res, _)) => {
                if res.validate_lengths() {
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::File(
                        path.to_string_lossy().to_string(),
                    )));
                    match insert_cycles(conn, &res, &project.id.unwrap(), &file_id) {
                        Ok((row_count, duplicates)) => {
                            let _ = progress_sender.send(ProcessEvent::Insert(
                                InsertEvent::OkSkip(row_count, duplicates),
                            ));
                        },
                        Err(e) => {
                            let _ = progress_sender
                                .send(ProcessEvent::Insert(InsertEvent::Fail(e.to_string())));
                        },
                    }
                } else {
                    let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::FileFail(
                        path.to_string_lossy().to_string(),
                        "Skipped, data vectors are not equal length, check your data file."
                            .to_owned(),
                    )));
                }
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::CycleFail(
                    path.to_string_lossy().to_string(),
                    e.to_string(),
                )));
            },
        }
    }
    let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
}

pub fn upload_meteo_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    let mut meteos = MeteoData::default();
    for path in &selected_paths {
        let project_id = project.id.unwrap();

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                // Optionally notify UI:
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                return (); // or `continue` if this is in a loop
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
        let file_id = match get_or_insert_data_file(&tx, DataType::Meteo, file_name, project_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                // Optionally notify UI
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue; // or return if not inside a loop
            },
        };
        match read_meteo_csv(path, tz) {
            //   Pass `path` directly
            Ok(res) => match insert_meteo_data(&tx, &file_id, &project.id.unwrap(), &res) {
                Ok(row_count) => {
                    let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Ok(
                        " of meteo data inserted.".to_owned(),
                        row_count as u64,
                    )));
                    if let Err(e) = tx.commit() {
                        eprintln!("Failed to commit transaction for '{}': {}", file_name, e);
                        let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(
                            format!("Commit failed for file '{}': {}", file_name, e),
                        )));
                        // no commit = rollback
                        continue;
                    }
                },
                Err(e) => {
                    let msg = format!("Failed to insert cycle data to db.Error {}", e);
                    let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(msg)));
                },
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::MeteoFail(
                    path.to_string_lossy().to_string(),
                    e.to_string(),
                )));
            },
        }
        let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
    }
}
pub fn upload_height_data_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    let mut heights = HeightData::default();
    for path in &selected_paths {
        let project_id = project.id.unwrap();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                // Optionally notify UI:
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                return (); // or `continue` if this is in a loop
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
        let file_id = match get_or_insert_data_file(&tx, DataType::Height, file_name, project_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Failed to insert/find data file '{}': {}", file_name, e);
                // Optionally notify UI
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue; // or return if not inside a loop
            },
        };
        match read_height_csv(path, tz) {
            Ok(res) => match insert_height_data(&tx, &file_id, &project.id.unwrap(), &res) {
                Ok(row_count) => {
                    let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Ok(
                        " of height data inserted.".to_owned(),
                        row_count,
                    )));
                    if let Err(e) = tx.commit() {
                        eprintln!("Failed to commit transaction for '{}': {}", file_name, e);
                        let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(
                            format!("Commit failed for file '{}': {}", file_name, e),
                        )));
                        // no commit = rollback
                        continue;
                    }
                },
                Err(e) => {
                    let msg = format!("Failed to insert cycle data to db.Error {}", e);
                    let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(msg)));
                },
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::HeightFail(
                    path.to_string_lossy().to_string(),
                    e.to_string(),
                )));
            },
        }
        let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
    }
}

pub fn upload_chamber_metadata_async(
    selected_paths: Vec<PathBuf>,
    conn: &mut Connection,
    project: &Project,
    tz: Tz,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) {
    for path in &selected_paths {
        let project_id = project.id.unwrap();

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                eprintln!("Skipping path with invalid filename: {:?}", path);
                // Optionally notify UI:
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::GasFail(
                    path.to_string_lossy().to_string(),
                    "Invalid file name (non-UTF8)".to_string(),
                )));
                return (); // or `continue` if this is in a loop
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
                // Optionally notify UI
                let _ = progress_sender.send(ProcessEvent::Insert(InsertEvent::Fail(format!(
                    "File '{}' skipped: {}",
                    file_name, e
                ))));
                continue; // or return if not inside a loop
            },
        };
        match read_chamber_metadata(path) {
            Ok(chambers) => match insert_chamber_metadata(&tx, &chambers, &project.id.unwrap()) {
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
            },
            Err(e) => {
                let _ = progress_sender.send(ProcessEvent::Read(ReadEvent::MetadataFail(
                    path.to_string_lossy().to_string(),
                    e.to_string(),
                )));
            },
        }
    }
    let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
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
pub fn drain_progress_messages<T: ProcessEventSink>(
    sink: &mut T,
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<ProcessEvent>,
) {
    loop {
        match receiver.try_recv() {
            Ok(msg) => match msg {
                ProcessEvent::Query(ev) => sink.on_query_event(&ev),
                ProcessEvent::Progress(ev) => sink.on_progress_event(&ev),
                ProcessEvent::Read(ev) => sink.on_read_event(&ev),
                ProcessEvent::Insert(ev) => sink.on_insert_event(&ev),
                ProcessEvent::Done(res) => sink.on_done(&res),
            },

            Err(TryRecvError::Empty) => {
                // nothing waiting right now -> we're done draining for this tick
                break;
            },

            Err(TryRecvError::Disconnected) => {
                // channel is closed, also done. you *could* choose to store a flag here.
                break;
            },
        }
    }
}

/// Inserts or fetches a data_file for a given project.
/// Returns the file's ID (existing or newly inserted).
pub fn get_or_insert_data_file(
    conn: &Connection,
    datatype: DataType,
    file_name: &str,
    project_id: i64,
) -> Result<i64> {
    // First, check if the file already exists for this project
    if let Ok(existing_id) = conn.query_row(
        "SELECT id FROM data_files WHERE file_name = ?1 AND project_link = ?2",
        params![file_name, project_id],
        |row| row.get::<_, i64>(0),
    ) {
        // Found existing entry
        return Ok(existing_id);
    }

    // If not found, insert it
    conn.execute(
        "INSERT INTO data_files (file_name, data_type, project_link) VALUES (?1, ?2, ?3)",
        params![file_name, datatype.type_str(), project_id],
    )?;

    // Return the new ID
    Ok(conn.last_insert_rowid())
}
