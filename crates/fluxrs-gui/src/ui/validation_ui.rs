use crate::appview::AppState;
use crate::ui::enable_plots::EnabledPlots;
use crate::ui::plot_fits::EnableFit;
use crate::ui::plot_width::PlotAdjust;
use crate::ui::plotting_ui::{
    init_attribute_plot, init_gas_plot, init_lag_plot, init_residual_bars,
    init_standardized_residuals_plot,
};
use crate::ui::recalc::RecalculateApp;
use crate::ui::tz_picker::TimezonePickerState;
use crate::utils::{bad_message, good_message, warn_message};

use crate::keybinds::{Action, KeyBindings};
use fluxrs_core::cycle::cycle::{AppError, Cycle};
use fluxrs_core::cycle::gaskey::GasKey;
use fluxrs_core::cycle_navigator::CycleNavigator;
use fluxrs_core::data_formats::chamberdata::ChamberOrigin;
use fluxrs_core::data_formats::meteodata::MeteoSource;
use fluxrs_core::datatype::DataType;
use fluxrs_core::errorcode::ErrorCode;
use fluxrs_core::flux::{FluxKind, FluxUnit};
use fluxrs_core::gastype::GasType;
use fluxrs_core::instruments::instruments::Instrument;
use fluxrs_core::mode::Mode;
use fluxrs_core::processevent::{
    InsertEvent, ProcessEvent, ProcessEventSink, ProgressEvent, QueryEvent, ReadEvent,
};
use fluxrs_core::project::Project;
use fluxrs_core::types::FastMap;

use std::path::PathBuf;

use tokio::sync::mpsc;
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver, UnboundedSender};

use eframe::egui::{Color32, Context, Label, RichText, Stroke, TextWrapMode};
use egui_file::FileDialog;
use egui_plot::{LineStyle, MarkerShape, PlotPoints, Polygon, VLine};

use chrono::{DateTime, NaiveDateTime, TimeDelta, TimeZone};
use chrono_tz::{Tz, UTC};
use rusqlite::Result;
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::fmt;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

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

type LoadResult = Arc<Mutex<Option<Result<Vec<Cycle>, AppError>>>>;
pub type ProgReceiver = UnboundedReceiver<ProcessEvent>;
pub type ProgSender = UnboundedSender<ProcessEvent>;

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
    pub plot_enabler: EnabledPlots,

    pub p_val_thresh: f32,
    pub rmse_thresh: f32,
    pub r2_thresh: f32,
    pub t0_thresh: f32,
    pub cycles: Vec<Cycle>,
    pub cycle_nav: CycleNavigator,
    pub plot_w: PlotAdjust,
    pub font_size: f32,
    pub dirty_cycles: HashSet<usize>,
    pub zoom_to_measurement: u8,
    pub should_reset_bounds: bool,
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
    pub selected_project: Option<Project>,
    pub show_fits: EnableFit,
    pub calc_area_color: Color32,
    pub calc_area_adjust_color: Color32,
    pub calc_area_stroke_color: Color32,
    pub selected_model: FluxKind,
    pub keybinds: KeyBindings,
    pub awaiting_rebind: Option<Action>,
    pub show_cycle_details: bool,
    pub show_residuals: bool,
    pub show_standardized_residuals: bool,
    pub show_lag_plot: bool,
    pub show_legend: bool,
    pub show_plot_widths: bool,
    pub toggled_gas: Option<GasKey>,
    pub dragging: Adjuster,
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
            plot_enabler: EnabledPlots::default(),

            p_val_thresh: 0.05,
            rmse_thresh: 25.,
            r2_thresh: 0.98,
            t0_thresh: 50000.,
            cycles: Vec::new(),
            cycle_nav: CycleNavigator::new(),
            font_size: 14.,
            plot_w: PlotAdjust::new(),
            zoom_to_measurement: 0,
            should_reset_bounds: false,
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
            selected_project: None,
            show_fits: EnableFit::new(),
            keep_calc_constant_deadband: true,
            calc_area_color: Color32::BLACK,
            calc_area_adjust_color: Color32::BLACK,
            calc_area_stroke_color: Color32::BLACK,
            selected_model: FluxKind::Linear,
            keybinds: KeyBindings::default(),
            awaiting_rebind: None,
            show_residuals: false,
            show_standardized_residuals: false,
            show_lag_plot: true,
            show_legend: true,
            show_cycle_details: true,
            show_plot_widths: true,
            toggled_gas: None,
            dragging: Adjuster::default(),
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
            self.show_cycle_details(ctx)
        }
        if self.show_plot_widths {
            egui::Window::new("Adjust plot widths").show(ctx, |ui| {
            ui.label("Drag boxes right/left or down/up to adjust plot sizes.");
            ui.label("Unfinished, flux plot dimensions also adjust all plots that are not gas or lag plot");
            egui::Grid::new("plots").show(ui, |ui| {
                ui.label("Lag plot width: ");
                ui.add(egui::DragValue::new(&mut self.plot_w.lag_plot_w).speed(1.).range(150.0..=1920.0));
                ui.label("Flux plot width:");
                ui.add(egui::DragValue::new(&mut self.plot_w.flux_plot_w).speed(1.).range(150.0..=1920.0));
                ui.label("Gas plot width:");
                ui.add(egui::DragValue::new(&mut self.plot_w.gas_plot_w).speed(1.).range(150.0..=1920.0));
                ui.end_row();
                ui.label("Lag plot height:");
                ui.add(egui::DragValue::new(&mut self.plot_w.lag_plot_h).speed(1.).range(150.0..=1920.0));
                ui.label("Flux plot height:");
                ui.add(egui::DragValue::new(&mut self.plot_w.flux_plot_h).speed(1.).range(150.0..=1920.0));
                ui.label("Gas plot height:");
                ui.add(egui::DragValue::new(&mut self.plot_w.gas_plot_h).speed(1.).range(150.0..=1920.0));
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
        let mut show_exp_model = true;
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
            });
            ui.vertical(|ui| {
                show_linear_model =
                    ui.checkbox(&mut self.show_fits.show_linfit, "Show linear model").clicked();
                show_poly_model = ui
                    .checkbox(&mut self.show_fits.show_polyfit, "Show polynomial model")
                    .clicked();
                show_roblin_model = ui
                    .checkbox(&mut self.show_fits.show_roblinfit, "Show robust linear model")
                    .clicked();
                show_exp_model = ui
                    .checkbox(&mut self.show_fits.show_expfit, "Show exponential model")
                    .clicked();
            });

            ui.vertical(|ui| {
                if let Some(current_cycle) = self.cycle_nav.current_cycle(&self.cycles) {
                    // Group keys by their label
                    let mut label_map: BTreeMap<i64, Vec<_>> = BTreeMap::new();

                    for key in &current_cycle.gases {
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
                                        .any(|((g, _s), record)| g == key && record.is_valid);

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
                                        self.toggled_gas = Some(*key);
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
                if self.keybinds.action_triggered(Action::ToggleShowInvalids, i) {
                    self.show_invalids = !self.show_invalids;
                    show_invalids_clicked = true;
                }
                if self.keybinds.action_triggered(Action::ToggleShowValids, i) {
                    self.show_valids = !self.show_valids;
                    show_valids_clicked = true;
                }
                if self.keybinds.action_triggered(Action::ToggleShowBad, i) {
                    self.show_bad = !self.show_bad;
                    show_bad = true;
                }
                if self.keybinds.action_triggered(Action::ToggleShowLegend, i) {
                    self.show_legend = !self.show_legend;
                }
                if self.keybinds.action_triggered(Action::ToggleValidity, i) {
                    toggle_valid = true;
                }
                if self.keybinds.action_triggered(Action::NextCycle, i) {
                    next_clicked = true;
                }
                if self.keybinds.action_triggered(Action::PreviousCycle, i) {
                    prev_clicked = true;
                }
                if self.keybinds.action_triggered(Action::ToggleBad, i) {
                    mark_bad = true;
                }
                if self.keybinds.action_triggered(Action::TogglePlotWidthsWindow, i) {
                    self.show_plot_widths = !self.show_plot_widths;
                }
                if self.keybinds.action_triggered(Action::ZoomToMeasurement, i) {
                    if self.zoom_to_measurement == 2 {
                        self.zoom_to_measurement = 0
                    } else {
                        self.zoom_to_measurement += 1;
                    }
                }
                if self.keybinds.action_triggered(Action::ResetCycle, i) {
                    reset_cycle = true;
                }
                if self.keybinds.action_triggered(Action::ToggleShowDetails, i) {
                    self.show_cycle_details = !self.show_cycle_details
                }
                if self.keybinds.action_triggered(Action::ToggleShowResiduals, i) {
                    self.show_residuals = !self.show_residuals
                }
                if self.keybinds.action_triggered(Action::ToggleShowLag, i) {
                    self.show_lag_plot = !self.show_lag_plot
                }
                if self.keybinds.action_triggered(Action::ToggleShowStandResiduals, i) {
                    self.show_standardized_residuals = !self.show_standardized_residuals
                }

                if self.keybinds.action_triggered(Action::ToggleCH4Validity, i) {
                    if let Some(current_cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles)
                    {
                        for ((g, _), record) in current_cycle.fluxes.iter_mut() {
                            if g.gas_type == GasType::CH4 {
                                record.is_valid = !record.is_valid;
                            }
                        }
                        self.mark_dirty();
                    }
                }
                if self.keybinds.action_triggered(Action::IncrementDeadband, i) {
                    self.mark_dirty();
                    if self.keep_calc_constant_deadband {
                        self.increment_deadband_constant_calc(1.);
                    } else {
                        self.increment_deadband(1.);
                    }
                    self.update_plots();
                }
                if self.keybinds.action_triggered(Action::DecrementDeadband, i) {
                    self.mark_dirty();
                    if self.keep_calc_constant_deadband {
                        self.increment_deadband_constant_calc(-1.);
                    } else {
                        self.increment_deadband(-1.);
                    }
                    self.update_plots();
                }
                if self.keybinds.action_triggered(Action::DecrementLag, i) {
                    self.mark_dirty();
                    let delta = -1.0;

                    match self.zoom_to_measurement {
                        0 | 1 => {
                            self.increment_open_lag(delta);
                        },
                        2 => {
                            self.increment_close_lag(delta);
                        },
                        _ => {},
                    }

                    if self.mode_pearsons() {
                        self.set_all_calc_range_to_best_r();
                    }
                    self.update_plots();
                }

                // BUG: calc area doesnt stick to deadband when incrementing
                if self.keybinds.action_triggered(Action::IncrementLag, i) {
                    self.mark_dirty();
                    let delta = 1.0;

                    match self.zoom_to_measurement {
                        0 | 1 => {
                            self.increment_open_lag(delta);
                        },
                        2 => {
                            self.increment_close_lag(delta);
                        },
                        _ => {},
                    }

                    if self.mode_pearsons() {
                        self.set_all_calc_range_to_best_r();
                    }

                    self.update_plots();
                }

                if self.keybinds.action_triggered(Action::SearchLag, i) {
                    self.mark_dirty();
                    if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                        cycle.search_new_open_lag(&GasKey::from((
                            &cycle.main_gas,
                            &cycle.main_instrument.id.unwrap(),
                        )));
                        self.update_plots();
                    }
                }
                if self.keybinds.action_triggered(Action::SearchLagPrevious, i) {
                    if let Some(current_visible_idx) = self.cycle_nav.current_index() {
                        if current_visible_idx > 0 {
                            let chamber_id = self.cycles[current_visible_idx].chamber_id.clone();
                            let (before, after) = self.cycles.split_at_mut(current_visible_idx);
                            let current_cycle = &mut after[0];

                            // find previous cycle which is valid and has the same chamber id
                            if let Some(previous_cycle) = before
                                .iter()
                                .rev()
                                .find(|cycle| cycle.chamber_id == chamber_id && cycle.is_valid)
                            {
                                // 1) reuse previous cycle's lag, not its absolute adjusted open time
                                let prev_lag = previous_cycle.get_open_lag(); // f64

                                // 2) compute where that lag would be in the current cycle
                                let current_start = current_cycle.timing.get_start_ts() as f64;
                                let current_open_offset = current_cycle.get_open_offset() as f64;
                                let target = current_start + current_open_offset + prev_lag;

                                let Some(main_gas) =
                                    self.selected_project.as_ref().unwrap().main_gas
                                else {
                                    eprintln!("No main gas selected!");
                                    return;
                                };

                                current_cycle.get_peak_near_timestamp(
                                    &GasKey::from((
                                        &main_gas,
                                        &current_cycle.main_instrument.id.unwrap(),
                                    )),
                                    target as i64,
                                );
                                self.mark_dirty();
                                self.update_plots();
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
                cycle.increment_end_lag_reload(120.);
                self.update_plots();
            }
        }
        if remove_from_end {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.increment_end_lag_reload(-120.);
                self.update_plots();
            }
        }
        if add_to_start {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.increment_start_lag_reload(-120.);
                self.update_plots();
            }
        }
        if remove_from_start {
            self.mark_dirty();
            if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                cycle.increment_start_lag_reload(120.);
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
            GasKey::from((&cycle.main_gas, &cycle.main_instrument.id.unwrap()))
        } else {
            return;
        };

        if self.plot_enabler.gases.is_empty() {
            self.plot_enabler.gases.insert(main_key);
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
            let mut instruments = FastMap::default();
            if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
                instruments = cycle.instruments.clone();
            }
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    if self.zoom_to_measurement == 2 {
                        self.should_reset_bounds = true;
                    }
                    let keys: Vec<_> = self.plot_enabler.gases.iter().copied().collect();
                    for key in &keys {
                        if self.plot_enabler.is_gas_enabled(&key) {
                            let gas_plot = init_gas_plot(
                                &key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.selected_project.as_ref().unwrap().tz,
                                self.get_start(),
                                self.get_end(),
                                self.plot_w.gas_plot_w,
                                self.plot_w.gas_plot_h,
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
                if !self.plot_enabler.lin_fluxes.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.lin_fluxes.iter().copied().collect();
                        for key in &keys {
                            let flux_plot = init_attribute_plot(
                                "flux".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let flux_unit = self.flux_unit;
                            let fluxkind = FluxKind::Linear;
                            let response = flux_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        flux_value_for_plot(cycle, key, fluxkind, flux_unit)
                                    },
                                    &format!(
                                        "Flux ({} {})",
                                        fluxkind.label(),
                                        flux_unit.to_owned()
                                    ),
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
                if !self.plot_enabler.poly_fluxes.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.poly_fluxes.iter().copied().collect();
                        for key in &keys {
                            let poly_flux_plot = init_attribute_plot(
                                "Poly Flux".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let flux_unit = self.flux_unit;
                            let fluxkind = FluxKind::Poly;
                            let response = poly_flux_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        flux_value_for_plot(cycle, key, fluxkind, flux_unit)
                                    },
                                    &format!(
                                        "Flux ({} {})",
                                        fluxkind.label(),
                                        flux_unit.to_owned()
                                    ),
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
                if !self.plot_enabler.roblin_fluxes.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> =
                            self.plot_enabler.roblin_fluxes.iter().copied().collect();
                        for key in &keys {
                            let roblin_flux_plot = init_attribute_plot(
                                "RobLin Flux".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let flux_unit = self.flux_unit;
                            let fluxkind = FluxKind::RobLin;
                            let response = roblin_flux_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        flux_value_for_plot(cycle, key, fluxkind, flux_unit)
                                    },
                                    &format!(
                                        "Flux ({} {})",
                                        fluxkind.label(),
                                        flux_unit.to_owned()
                                    ),
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
                if !self.plot_enabler.exp_fluxes.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.exp_fluxes.iter().copied().collect();
                        for key in &keys {
                            let exp_flux_plot = init_attribute_plot(
                                "Exponential Flux".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let flux_unit = self.flux_unit;
                            let fluxkind = FluxKind::Exponential;
                            let response = exp_flux_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        flux_value_for_plot(cycle, key, fluxkind, flux_unit)
                                    },
                                    &format!(
                                        "Flux ({} {})",
                                        fluxkind.label(),
                                        flux_unit.to_owned()
                                    ),
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
                if !self.plot_enabler.lin_p_val.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.lin_p_val.iter().copied().collect();
                        for key in &keys {
                            let lin_p_val_plot = init_attribute_plot(
                                "Linear p-value".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = lin_p_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Linear))
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
                if !self.plot_enabler.measurement_rs.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> =
                            self.plot_enabler.measurement_rs.iter().copied().collect();
                        for key in &keys {
                            let measurement_r_plot = init_attribute_plot(
                                "Measurement r2".to_owned(),
                                &key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.measurement_r_plot_w,
                                self.plot_w.measurement_r_plot_h,
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
                if !self.plot_enabler.calc_r.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.calc_r.iter().copied().collect();
                        for key in &keys {
                            let selected_model = self.selected_model;
                            let calc_r_plot = init_attribute_plot(
                                format!("{} r2", selected_model),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.calc_r_plot_w,
                                self.plot_w.calc_r_plot_h,
                            );
                            let response = calc_r_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, selected_model))
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
                if !self.plot_enabler.conc_t0.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.conc_t0.iter().copied().collect();
                        for key in &keys {
                            let conc_plot = init_attribute_plot(
                                "Concentration t0".to_owned(),
                                &key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.conc_t0_plot_w,
                                self.plot_w.conc_t0_plot_h,
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
                if !self.plot_enabler.lin_adj_r2.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.lin_adj_r2.iter().copied().collect();
                        for key in &keys {
                            let adj_r2_val_plot = init_attribute_plot(
                                "Lin adjusted r2".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = adj_r2_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Linear))
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
                if !self.plot_enabler.lin_sigma.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.lin_sigma.iter().copied().collect();
                        for key in &keys {
                            let sigma_plot = init_attribute_plot(
                                "Lin sigma".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = sigma_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Linear))
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
                if !self.plot_enabler.lin_aic.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.lin_aic.iter().copied().collect();
                        for key in &keys {
                            let lin_aic_plot = init_attribute_plot(
                                "Lin AIC".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = lin_aic_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Linear))
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
                if !self.plot_enabler.lin_rmse.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.lin_rmse.iter().copied().collect();
                        for key in &keys {
                            let lin_rmse_plot = init_attribute_plot(
                                "Lin RMSE".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = lin_rmse_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Linear))
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
                if !self.plot_enabler.lin_cv.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.lin_cv.iter().copied().collect();
                        for key in &keys {
                            let lin_cv_plot = init_attribute_plot(
                                "Lin cv".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = lin_cv_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Linear))
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
                if !self.plot_enabler.poly_adj_r2.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.poly_adj_r2.iter().copied().collect();
                        for key in &keys {
                            let adj_r2_val_plot = init_attribute_plot(
                                "Poly adjusted r2".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = adj_r2_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Poly))
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
                if !self.plot_enabler.poly_sigma.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.poly_sigma.iter().copied().collect();
                        for key in &keys {
                            let sigma_plot = init_attribute_plot(
                                "Poly sigma".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = sigma_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Poly))
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
                if !self.plot_enabler.poly_aic.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.poly_aic.iter().copied().collect();
                        for key in &keys {
                            let poly_aic_plot = init_attribute_plot(
                                "Poly AIC".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = poly_aic_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Poly))
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
                if !self.plot_enabler.poly_rmse.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.poly_rmse.iter().copied().collect();
                        for key in &keys {
                            let poly_rmse_plot = init_attribute_plot(
                                "Poly RMSE".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = poly_rmse_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Poly))
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
                if !self.plot_enabler.poly_cv.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.poly_cv.iter().copied().collect();
                        for key in &keys {
                            let poly_cv_plot = init_attribute_plot(
                                "Poly cv".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = poly_cv_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Poly))
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
                if !self.plot_enabler.roblin_adj_r2.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> =
                            self.plot_enabler.roblin_adj_r2.iter().copied().collect();
                        for key in &keys {
                            let adj_r2_val_plot = init_attribute_plot(
                                "Roblin Adjusted r2".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = adj_r2_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::RobLin))
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
                if !self.plot_enabler.roblin_sigma.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.roblin_sigma.iter().copied().collect();
                        for key in &keys {
                            let sigma_plot = init_attribute_plot(
                                "RobLin sigma".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = sigma_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::RobLin))
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
                if !self.plot_enabler.roblin_aic.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.roblin_aic.iter().copied().collect();
                        for key in &keys {
                            let roblin_aic_plot = init_attribute_plot(
                                "RobLin AIC".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = roblin_aic_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::RobLin))
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
                if !self.plot_enabler.roblin_rmse.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.roblin_rmse.iter().copied().collect();
                        for key in &keys {
                            let roblin_rmse_plot = init_attribute_plot(
                                "RobLin RMSE".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = roblin_rmse_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::RobLin))
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
                if !self.plot_enabler.roblin_cv.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.roblin_cv.iter().copied().collect();
                        for key in &keys {
                            let roblin_cv_plot = init_attribute_plot(
                                "RobLin cv".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = roblin_cv_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::RobLin))
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
                if !self.plot_enabler.exp_adj_r2.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.exp_adj_r2.iter().copied().collect();
                        for key in &keys {
                            let adj_r2_val_plot = init_attribute_plot(
                                "Exp adjusted r2".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = adj_r2_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Exponential))
                                            .and_then(|model| model.model.adj_r2())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Exponential.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.plot_enabler.exp_sigma.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.exp_sigma.iter().copied().collect();
                        for key in &keys {
                            let sigma_plot = init_attribute_plot(
                                "Exp sigma".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = sigma_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Exponential))
                                            .and_then(|model| model.model.sigma())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Exponential.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.plot_enabler.exp_aic.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.exp_aic.iter().copied().collect();
                        for key in &keys {
                            let exp_aic_plot = init_attribute_plot(
                                "Exp AIC".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = exp_aic_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Exponential))
                                            .and_then(|model| model.model.aic())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Exponential.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.plot_enabler.exp_rmse.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.exp_rmse.iter().copied().collect();
                        for key in &keys {
                            let exp_rmse_plot = init_attribute_plot(
                                "Exp RMSE".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = exp_rmse_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Exponential))
                                            .and_then(|model| model.model.rmse())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Exponential.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.plot_enabler.exp_cv.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.exp_cv.iter().copied().collect();
                        for key in &keys {
                            let exp_cv_plot = init_attribute_plot(
                                "Exp cv".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = exp_cv_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Exponential))
                                            .and_then(|model| model.model.cv())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Exponential.label()),
                                    None,
                                );
                            });
                            if response.response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                            }
                        }
                    });
                }
                if !self.plot_enabler.exp_p_val.is_empty() {
                    ui.vertical(|ui| {
                        let keys: Vec<_> = self.plot_enabler.exp_p_val.iter().copied().collect();
                        for key in &keys {
                            let exp_p_val_plot = init_attribute_plot(
                                "Exponential p-value".to_owned(),
                                key,
                                instruments.get(&key.id).unwrap().clone(),
                                self.plot_w.flux_plot_w,
                                self.plot_w.flux_plot_h,
                            );
                            let response = exp_p_val_plot.show(ui, |plot_ui| {
                                self.render_attribute_plot(
                                    plot_ui,
                                    key,
                                    move |cycle, key| {
                                        cycle
                                            .fluxes
                                            .get(&(*key, FluxKind::Exponential))
                                            .and_then(|model| model.model.p_value())
                                            .unwrap_or(0.0)
                                    },
                                    &format!("Flux ({})", FluxKind::Exponential.label()),
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
            });
        });
        let mut main_instrument = Instrument::default();
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            main_instrument = cycle.instruments.get(&main_key.id).unwrap().clone();
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            ui.horizontal(|ui| {
                if self.show_lag_plot {
                    let lag_plot = init_lag_plot(
                        &main_key,
                        main_instrument.clone(),
                        self.plot_w.lag_plot_w,
                        self.plot_w.lag_plot_h,
                    );
                    let response = lag_plot.show(ui, |plot_ui| {
                        self.render_lag_plot(plot_ui);
                    });
                    if response.response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                    }
                }
                let flux_unit = self.flux_unit;
                let keys: Vec<_> = self.plot_enabler.gases.iter().copied().collect();
                for key in &keys {
                    let flux_plot = init_attribute_plot(
                        format!("Best flux {}", flux_unit),
                        key,
                        main_instrument.clone(),
                        self.plot_w.flux_plot_w,
                        self.plot_w.flux_plot_h,
                    );
                    let response2 = flux_plot.show(ui, |plot_ui| {
                        self.render_best_flux_plot(plot_ui, key, |cycle, gas| {
                            let umol_m2_s = cycle.best_flux_by_aic(gas).unwrap_or(f64::NAN);
                            flux_unit.from_umol_m2_s(umol_m2_s, gas.gas_type)
                        });
                    });
                    if response2.response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                    }
                }
            });
        });
        let mut instruments = FastMap::default();
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            instruments = cycle.instruments.clone();
        }
        if self.show_residuals {
            egui::Window::new("Residual bar plots").show(ctx, |ui| {
                ui.vertical(|ui| {
                    for model in FluxKind::all() {
                        ui.horizontal(|ui| {
                            for gas in &self.plot_enabler.gases {
                                // NOTE: get rid of clone
                                let instrument = instruments.get(&gas.id).unwrap().clone();
                                let residual_bars =
                                    init_residual_bars(gas, instrument, *model, 250., 145.);
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
                    for model in FluxKind::all() {
                        ui.horizontal(|ui| {
                            for gas in &self.plot_enabler.gases {
                                // NOTE: get rid of clone
                                let instrument = instruments.get(&gas.id).unwrap().clone();
                                let residual_plot_stand = init_standardized_residuals_plot(
                                    gas, instrument, *model, 250., 145.,
                                );
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

        if start_before_end {
            let delta = self.end_date - self.start_date;

            if let Ok(duration) = delta.to_std() {
                let total_secs = duration.as_secs();

                let days = total_secs / 86_400;
                let hours = (total_secs % 86_400) / 3_600;
                let minutes = (total_secs % 3_600) / 60;
                let seconds = total_secs % 60;

                let duration_str = if days > 0 {
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

            let main_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|key| (*key, self.plot_enabler.is_gas_enabled(key))).collect();

            let measurement_r_gases: Vec<(GasKey, bool)> = gases
                .iter()
                .map(|key| (*key, self.plot_enabler.is_measurement_r_enabled(key)))
                .collect();

            let conc_t0_gases: Vec<(GasKey, bool)> =
                gases.iter().map(|key| (*key, self.plot_enabler.is_conc_t0_enabled(key))).collect();

            let min_width = 100.;
            ui.vertical(|ui| {
                ui.label("General measurement plots");
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.label("Enable gases");
                    ui.vertical(|ui| {
                        for (gas, mut is_enabled) in &main_gases {
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
                                    self.plot_enabler.gases.insert(*gas);
                                } else {
                                    self.plot_enabler.gases.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Cycle r2");
                        for (gas, mut is_enabled) in &measurement_r_gases {
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
                                    self.plot_enabler.measurement_rs.insert(*gas);
                                } else {
                                    self.plot_enabler.measurement_rs.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("t0 concentration");
                        for (gas, mut is_enabled) in &conc_t0_gases {
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
                                    self.plot_enabler.conc_t0.insert(*gas);
                                } else {
                                    self.plot_enabler.conc_t0.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("t0 concentration");
                        for (gas, mut is_enabled) in &conc_t0_gases {
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
                                    self.plot_enabler.conc_t0.insert(*gas);
                                } else {
                                    self.plot_enabler.conc_t0.remove(gas);
                                }
                            }
                        }
                    });
                });
            });
            ui.group(|ui| {
                ui.checkbox(&mut self.show_lag_plot, "Show lag plot");
            });
        } else {
            ui.colored_label(Color32::ORANGE, "LOAD DATA TO SHOW ALL SETTINGS!");
        }
    }
    pub fn render_lin_plot_selection(&mut self, ui: &mut egui::Ui) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let gases = cycle.gases.clone(); // Clone gases early!

            let lin_flux_gases = self.plot_enabler.get_lin_flux_gases(&gases);
            let lin_p_val_gases = self.plot_enabler.get_lin_p_val_gases(&gases);
            let lin_adj_r2_gases = self.plot_enabler.get_lin_adj_r2_gases(&gases);
            let lin_sigma_gases = self.plot_enabler.get_lin_sigma_gases(&gases);
            let lin_rmse_gases = self.plot_enabler.get_lin_rmse_gases(&gases);
            let lin_cv_gases = self.plot_enabler.get_lin_cv_gases(&gases);
            let lin_aic_gases = self.plot_enabler.get_lin_aic_gases(&gases);

            let min_width = 150.;
            ui.vertical(|ui| {
                ui.add(Label::new("Linear model plots").wrap_mode(TextWrapMode::Truncate));
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Flux");
                        for (gas, mut is_enabled) in &lin_flux_gases {
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
                                    self.plot_enabler.lin_fluxes.insert(*gas);
                                } else {
                                    self.plot_enabler.lin_fluxes.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Adjusted r2");
                        for (gas, mut is_enabled) in &lin_adj_r2_gases {
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
                                    self.plot_enabler.lin_adj_r2.insert(*gas);
                                } else {
                                    self.plot_enabler.lin_adj_r2.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Sigma");
                        for (gas, mut is_enabled) in &lin_sigma_gases {
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
                                    self.plot_enabler.lin_sigma.insert(*gas);
                                } else {
                                    self.plot_enabler.lin_sigma.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("AIC");
                        for (gas, mut is_enabled) in &lin_aic_gases {
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
                                    self.plot_enabler.lin_aic.insert(*gas);
                                } else {
                                    self.plot_enabler.lin_aic.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("RMSE");
                        for (gas, mut is_enabled) in &lin_rmse_gases {
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
                                    self.plot_enabler.lin_rmse.insert(*gas);
                                } else {
                                    self.plot_enabler.lin_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("CV");
                        for (gas, mut is_enabled) in &lin_cv_gases {
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
                                    self.plot_enabler.lin_cv.insert(*gas);
                                } else {
                                    self.plot_enabler.lin_cv.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("p-value");
                        for (gas, mut is_enabled) in &lin_p_val_gases {
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
                                    self.plot_enabler.lin_p_val.insert(*gas);
                                } else {
                                    self.plot_enabler.lin_p_val.remove(gas);
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

            let roblin_flux_gases = self.plot_enabler.get_roblin_flux_gases(&gases);
            let roblin_adj_r2_gases = self.plot_enabler.get_roblin_adj_r2_gases(&gases);
            let roblin_sigma_gases = self.plot_enabler.get_roblin_sigma_gases(&gases);
            let roblin_rmse_gases = self.plot_enabler.get_roblin_rmse_gases(&gases);
            let roblin_cv_gases = self.plot_enabler.get_roblin_cv_gases(&gases);
            let roblin_aic_gases = self.plot_enabler.get_roblin_aic_gases(&gases);

            let min_width = 150.;
            ui.vertical(|ui| {
                ui.add(Label::new("RobLin model plots").wrap_mode(TextWrapMode::Truncate));
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Flux");
                        for (gas, mut is_enabled) in &roblin_flux_gases {
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
                                    self.plot_enabler.roblin_fluxes.insert(*gas);
                                } else {
                                    self.plot_enabler.roblin_fluxes.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Adjusted r2");
                        for (gas, mut is_enabled) in &roblin_adj_r2_gases {
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
                                    self.plot_enabler.roblin_adj_r2.insert(*gas);
                                } else {
                                    self.plot_enabler.roblin_adj_r2.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Sigma");
                        for (gas, mut is_enabled) in &roblin_sigma_gases {
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
                                    self.plot_enabler.roblin_sigma.insert(*gas);
                                } else {
                                    self.plot_enabler.roblin_sigma.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("AIC");
                        for (gas, mut is_enabled) in &roblin_aic_gases {
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
                                    self.plot_enabler.roblin_aic.insert(*gas);
                                } else {
                                    self.plot_enabler.roblin_aic.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("RMSE");
                        for (gas, mut is_enabled) in &roblin_rmse_gases {
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
                                    self.plot_enabler.roblin_rmse.insert(*gas);
                                } else {
                                    self.plot_enabler.roblin_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("CV");
                        for (gas, mut is_enabled) in &roblin_cv_gases {
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
                                    self.plot_enabler.roblin_cv.insert(*gas);
                                } else {
                                    self.plot_enabler.roblin_cv.remove(gas);
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

            let poly_flux_gases = self.plot_enabler.get_poly_flux_gases(&gases);
            let poly_adj_r2_gases = self.plot_enabler.get_poly_adj_r2_gases(&gases);
            let poly_sigma_gases = self.plot_enabler.get_poly_sigma_gases(&gases);
            let poly_rmse_gases = self.plot_enabler.get_poly_rmse_gases(&gases);
            let poly_cv_gases = self.plot_enabler.get_poly_cv_gases(&gases);
            let poly_aic_gases = self.plot_enabler.get_poly_aic_gases(&gases);

            let min_width = 150.;
            ui.vertical(|ui| {
                ui.group(|ui| {
                    ui.add(Label::new("Poly model plots").wrap_mode(TextWrapMode::Truncate));
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Flux");
                        for (gas, mut is_enabled) in &poly_flux_gases {
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
                                    self.plot_enabler.poly_fluxes.insert(*gas);
                                } else {
                                    self.plot_enabler.poly_fluxes.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Adjusted r2");
                        for (gas, mut is_enabled) in &poly_adj_r2_gases {
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
                                    self.plot_enabler.poly_adj_r2.insert(*gas);
                                } else {
                                    self.plot_enabler.poly_adj_r2.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("Sigma");
                        for (gas, mut is_enabled) in &poly_sigma_gases {
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
                                    self.plot_enabler.poly_sigma.insert(*gas);
                                } else {
                                    self.plot_enabler.poly_sigma.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("AIC");
                        for (gas, mut is_enabled) in &poly_aic_gases {
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
                                    self.plot_enabler.poly_aic.insert(*gas);
                                } else {
                                    self.plot_enabler.poly_aic.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("RMSE");
                        for (gas, mut is_enabled) in &poly_rmse_gases {
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
                                    self.plot_enabler.poly_rmse.insert(*gas);
                                } else {
                                    self.plot_enabler.poly_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });
                ui.group(|ui| {
                    ui.set_min_width(min_width); // Enforce group width here
                    ui.vertical(|ui| {
                        ui.label("CV");
                        for (gas, mut is_enabled) in &poly_cv_gases {
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
                                    self.plot_enabler.poly_cv.insert(*gas);
                                } else {
                                    self.plot_enabler.poly_cv.remove(gas);
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
    pub fn render_exp_plot_selection(&mut self, ui: &mut egui::Ui) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let gases = cycle.gases.clone(); // Clone gases early!

            let exp_flux_gases = self.plot_enabler.get_exp_flux_gases(&gases);
            let exp_p_val_gases = self.plot_enabler.get_exp_p_val_gases(&gases);
            let exp_adj_r2_gases = self.plot_enabler.get_exp_adj_r2_gases(&gases);
            let exp_sigma_gases = self.plot_enabler.get_exp_sigma_gases(&gases);
            let exp_rmse_gases = self.plot_enabler.get_exp_rmse_gases(&gases);
            let exp_cv_gases = self.plot_enabler.get_exp_cv_gases(&gases);
            let exp_aic_gases = self.plot_enabler.get_exp_aic_gases(&gases);

            let min_width = 150.0;
            ui.vertical(|ui| {
                ui.add(Label::new("Exponential model plots").wrap_mode(TextWrapMode::Truncate));

                ui.group(|ui| {
                    ui.set_min_width(min_width);
                    ui.vertical(|ui| {
                        ui.label("Flux");
                        for (gas, mut is_enabled) in &exp_flux_gases {
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
                                    self.plot_enabler.exp_fluxes.insert(*gas);
                                } else {
                                    self.plot_enabler.exp_fluxes.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width);
                    ui.vertical(|ui| {
                        ui.label("Adjusted r2");
                        for (gas, mut is_enabled) in &exp_adj_r2_gases {
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
                                    self.plot_enabler.exp_adj_r2.insert(*gas);
                                } else {
                                    self.plot_enabler.exp_adj_r2.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width);
                    ui.vertical(|ui| {
                        ui.label("Sigma");
                        for (gas, mut is_enabled) in &exp_sigma_gases {
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
                                    self.plot_enabler.exp_sigma.insert(*gas);
                                } else {
                                    self.plot_enabler.exp_sigma.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width);
                    ui.vertical(|ui| {
                        ui.label("AIC");
                        for (gas, mut is_enabled) in &exp_aic_gases {
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
                                    self.plot_enabler.exp_aic.insert(*gas);
                                } else {
                                    self.plot_enabler.exp_aic.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width);
                    ui.vertical(|ui| {
                        ui.label("RMSE");
                        for (gas, mut is_enabled) in &exp_rmse_gases {
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
                                    self.plot_enabler.exp_rmse.insert(*gas);
                                } else {
                                    self.plot_enabler.exp_rmse.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width);
                    ui.vertical(|ui| {
                        ui.label("CV");
                        for (gas, mut is_enabled) in &exp_cv_gases {
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
                                    self.plot_enabler.exp_cv.insert(*gas);
                                } else {
                                    self.plot_enabler.exp_cv.remove(gas);
                                }
                            }
                        }
                    });
                });

                ui.group(|ui| {
                    ui.set_min_width(min_width);
                    ui.vertical(|ui| {
                        ui.label("p-value");
                        for (gas, mut is_enabled) in &exp_p_val_gases {
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
                                    self.plot_enabler.exp_p_val.insert(*gas);
                                } else {
                                    self.plot_enabler.exp_p_val.remove(gas);
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
    pub fn show_cycle_details(&self, ctx: &Context) {
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
                        ui.label("Main instrument:");
                        ui.label(format!("{}", cycle.main_instrument.model));
                        ui.end_row();
                        ui.label("Serial:");
                        ui.label(&cycle.main_instrument.serial);
                        ui.end_row();
                        ui.label("Chamber:");
                        ui.label(cycle.chamber_id.to_string());
                        ui.end_row();
                        ui.label("Start Time:");
                        ui.label(
                            DateTime::from_timestamp(cycle.get_start() as i64, 0)
                                .unwrap()
                                .with_timezone(&self.selected_project.as_ref().unwrap().tz)
                                .to_string(),
                        );
                        ui.end_row();
                        ui.label("Epoch:");
                        ui.label(cycle.get_start_ts().to_string());
                        ui.end_row();
                        ui.label("Epoch End:");
                        ui.label((cycle.get_start_ts() + cycle.get_end_offset()).to_string());
                        ui.end_row();
                        ui.label("First TS:");
                        if let Some(first_val) =
                            cycle.get_dt_v(&cycle.main_instrument.id.unwrap()).first()
                        {
                            ui.label(format!("{}", first_val.to_owned()));
                        } else {
                            ui.label("None");
                        }
                        ui.end_row();
                        ui.label("Last TS:");
                        if let Some(last_val) =
                            cycle.get_dt_v(&cycle.main_instrument.id.unwrap()).last()
                        {
                            ui.label(format!("{}", last_val.to_owned()));
                        } else {
                            ui.label("None");
                        }
                        ui.end_row();
                        ui.label("Close Offset:");
                        ui.label(cycle.get_close_offset().to_string());
                        ui.end_row();
                        ui.label("Close lag:");
                        ui.label(cycle.get_close_lag().to_string());
                        ui.end_row();
                        ui.label("Open Offset:");
                        ui.label(cycle.get_open_offset().to_string());
                        ui.end_row();
                        ui.label("Open lag:");
                        ui.label(cycle.get_open_lag().to_string());
                        ui.end_row();
                        ui.label("End Offset:");
                        ui.label(cycle.get_end_offset().to_string());
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
                        ui.label("Measurement r²:");
                        ui.label(
                            match cycle.measurement_r2.get(
                                &(GasKey::from((&main_gas, &cycle.main_instrument.id.unwrap()))),
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

                egui::Grid::new("cycle_details_grid").striped(true).show(ui, |ui| {
                    ui.label("Chamber height:");
                    ui.label(format!("{:.2}", cycle.chamber_height));
                    ui.end_row();
                    ui.label("Chamber volume:");
                    ui.label(format!("{:.2} cm3", cycle.chamber.volume_m3() * 1e+6));
                    ui.end_row();
                    ui.label("Chamber area:");
                    ui.label(format!("{:.2} cm2", cycle.chamber.area_m2() * 1e+4));
                    ui.end_row();
                    ui.label("Chamber dimensions:");
                    let cham_txt = format!("{}", cycle.chamber);
                    if cycle.chamber.origin != ChamberOrigin::Raw {
                        ui.colored_label(Color32::ORANGE, cham_txt);
                    } else {
                        ui.label(cham_txt);
                    }
                    ui.end_row();

                    ui.label("Air temperature");
                    let temp_text = format!("{}", cycle.air_temperature);
                    if cycle.air_temperature.source != MeteoSource::Raw {
                        ui.colored_label(Color32::ORANGE, temp_text);
                    } else {
                        ui.label(temp_text);
                    }
                    ui.end_row();

                    ui.label("Air pressure");
                    let press_text = format!("{}", cycle.air_pressure);
                    if cycle.air_pressure.source != MeteoSource::Raw {
                        ui.colored_label(Color32::ORANGE, press_text);
                    } else {
                        ui.label(press_text);
                    }
                    ui.end_row();
                });
                ui.separator();

                for model in FluxKind::all() {
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
                            ui.label("c0");
                            ui.end_row();

                            for gas in &self.plot_enabler.gases {
                                let flux = if let Some(raw_flux) = cycle.get_flux(gas, *model) {
                                    let converted_flux =
                                        self.flux_unit.from_umol_m2_s(raw_flux, gas.gas_type);

                                    format!("{:.6}", converted_flux)
                                } else {
                                    "N/A".to_string()
                                };
                                // NOTE: Add pearsons r2
                                let r2 = cycle
                                    .get_r2(gas, *model)
                                    .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                let cv = cycle
                                    .get_cv(gas, *model)
                                    .map_or("N/A".to_string(), |v| format!("{:.6}", v * 100.));
                                let sigma = cycle
                                    .get_sigma(gas, *model)
                                    .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                let rmse = cycle
                                    .get_rmse(gas, *model)
                                    .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                let aic = cycle
                                    .get_aic(gas, *model)
                                    .map_or("N/A".to_string(), |v| format!("{:.6}", v));
                                let c0 = cycle
                                    .t0_concentration
                                    .get(gas)
                                    .map_or("N/A".to_string(), |v| format!("{:.6}", v));

                                ui.label(format!("{}", gas.gas_type));
                                ui.label(flux);
                                ui.label(r2);
                                ui.label(cv);
                                ui.label(sigma);
                                ui.label(rmse);
                                ui.label(aic);
                                ui.label(c0);
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
                self.log_messages.push_front(good_message(msg));
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
            ReadEvent::DataFail { kind, file, reason } => {
                let what = match kind {
                    DataType::Meteo => "meteo",
                    DataType::Gas => "gas",
                    DataType::Height => "height",
                    DataType::Cycle => "cycle",
                    DataType::Chamber => "chamber metadata",
                };
                let msg = format!("Could not parse as {} file: {}, {}", what, file, reason);
                self.log_messages.push_front(bad_message(&msg));
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
            InsertEvent::DataOkSkip { kind, inserts, skips } => {
                let what = match kind {
                    DataType::Meteo => "meteo",
                    DataType::Gas => "gas",
                    DataType::Height => "height",
                    DataType::Cycle => "cycle",
                    DataType::Chamber => "chamber metadata",
                };
                if *skips == 0 {
                    self.log_messages.push_front(good_message(&format!(
                        "Inserted rows of {} {} data.",
                        inserts, what
                    )));
                } else {
                    self.log_messages.push_front(warn_message(&format!(
                        "Inserted rows of {} {} data, skipped {} duplicates.",
                        inserts, what, skips
                    )));
                }
            },
            InsertEvent::Fail(e) => {
                self.log_messages.push_front(bad_message(&format!("Failed to insert rows: {}", e)));
                self.cycles_progress = 0;
                self.init_in_progress = false;
                self.init_enabled = true;
                self.query_in_progress = false;
                self.recalc.calc_enabled = true;
                self.recalc.calc_in_progress = false;
                self.recalc.query_in_progress = false;
                self.recalc.cycles_progress = 0;
                self.recalc.cycles_state = None;
            },
        }
    }

    fn on_done(&mut self, res: &Result<(), String>) {
        match res {
            Ok(()) => {
                self.log_messages.push_front(good_message("All processing finished."));
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
    _min_y: f64,
    _max_y: f64,
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
fn flux_value_for_plot(cycle: &Cycle, key: &GasKey, model: FluxKind, flux_unit: FluxUnit) -> f64 {
    let flux_umol_m2_s =
        cycle.fluxes.get(&(*key, model)).and_then(|record| record.model.flux()).unwrap_or(0.0);

    flux_unit.from_umol_m2_s(flux_umol_m2_s, key.gas_type)
}
