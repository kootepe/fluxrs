use super::download_app::DownloadApp;
use super::file_app::FileApp;
use super::manage_proj::ProjectApp;
use super::table_app::TableApp;
use super::validation_app::{AsyncCtx, ValidationApp};
use crate::appview::AppState;
use crate::keybinds::{self, Action, KeyBind, KeyBindings};
use crate::utils::{bad_message, good_message, warn_message};
use egui::{FontFamily, RichText, ScrollArea, Separator, WidgetInfo, WidgetType};
use fluxrs_core::datatype::DataType;
use fluxrs_core::processevent::{
    InsertEvent, ProcessEvent, ProcessEventSink, ProgressEvent, QueryEvent, ReadEvent,
};
use fluxrs_core::project::Project;
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::Path;
use tokio::sync::mpsc::error::TryRecvError;

pub enum AppEvent {
    SelectProject(Option<Project>),
}

#[derive(Default, PartialEq)]
struct EmptyPanel {}

impl EmptyPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui) {}
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
    switching_allowed: bool,
    pub log_messages: VecDeque<RichText>,
    app_state_loaded: bool,
    pub selected_project: Option<Project>,
    pub font_size: f32,
    live_panel: Panel,
    pub validation_panel: ValidationApp,
    table_panel: TableApp,
    dl_panel: DownloadApp,
    proj_panel: ProjectApp,
    file_panel: FileApp,
    empty_panel: EmptyPanel,
}

impl MainApp {
    pub fn new() -> Self {
        Self { switching_allowed: true, font_size: 14., ..Default::default() }
    }
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        async_ctx: &mut AsyncCtx,
        keybinds: &KeyBindings,
    ) {
        self.apply_font_size(ctx, self.font_size);
        for (_text_style, font_id) in ui.style_mut().text_styles.iter_mut() {
            font_id.family = FontFamily::Monospace;
        }

        self.handle_progress_messages(async_ctx);

        // NOTE: this block would be better in MainApp or FluxApp
        // project should exist when app is in validation panel, it doesnt need to be option
        if self.validation_panel.selected_project.is_none() {
            self.proj_panel.load_projects_from_db().unwrap();
            self.selected_project = self.proj_panel.project.clone();

            if self.selected_project.is_some() {
                // set the preselected timezone to show in dropdown
                self.file_panel.tz_state.query =
                    self.selected_project.clone().unwrap().tz.to_string();
                self.file_panel.tz_state.selected = Some(self.selected_project.clone().unwrap().tz);
                self.file_panel.tz_for_files = Some(self.selected_project.clone().unwrap().tz);

                let user_tz = self.selected_project.clone().unwrap_or_default().tz;
                if !self.app_state_loaded {
                    // move this block into a function in ValidationApp
                    if let Ok(app) = load_app_state(Path::new("app_state.json")) {
                        println!("Reload app state");
                        self.validation_panel.start_date = app.start_date.with_timezone(&user_tz);
                        self.validation_panel.end_date = app.end_date.with_timezone(&user_tz);
                        // prevent constantly reloading the app state file
                        self.app_state_loaded = true;
                    }
                }
            }
        }

        if let Some(event) = self.proj_panel.update_project() {
            match event {
                AppEvent::SelectProject(proj) => {
                    if proj.is_some() {
                        if self.validation_panel.selected_project.clone().unwrap_or_default().name
                            != proj.clone().unwrap_or_default().name
                        {
                            self.validation_panel.selected_project = proj;
                            self.validation_panel.cycles = Vec::new();

                            self.validation_panel.tz_state.query = self
                                .validation_panel
                                .selected_project
                                .clone()
                                .unwrap()
                                .tz
                                .to_string();
                            self.validation_panel.tz_state.selected =
                                Some(self.validation_panel.selected_project.clone().unwrap().tz);
                            self.validation_panel.tz_for_files =
                                Some(self.validation_panel.selected_project.clone().unwrap().tz);
                        }
                    } else {
                        self.validation_panel.selected_project = None
                    }
                },
            }
        }
        ui.horizontal_wrapped(|ui| {
            let container_response = ui.response();
            container_response
                .widget_info(|| WidgetInfo::labeled(WidgetType::RadioGroup, true, "Select panel"));

            ui.ctx().clone().with_accessibility_parent(container_response.id, || {
                ui.add_enabled(self.switching_allowed, |ui: &mut egui::Ui| {
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

        let project = self.validation_panel.selected_project.clone();
        let log_msgs = &mut self.log_messages;
        match self.live_panel {
            Panel::Validation => {
                self.validation_panel.ui(ui, ctx, async_ctx, keybinds);
            },
            Panel::DataLoad => {
                self.validation_panel.load_ui(ui, ctx, async_ctx, log_msgs);
                self.log_display(ui);
            },
            Panel::DataInit => {
                self.validation_panel.init_ui(ui, ctx, async_ctx);
                self.log_display(ui);
            },
            Panel::FileInit => {
                self.file_panel.ui(ui, ctx, async_ctx, &project, log_msgs);
                self.log_display(ui);
            },
            Panel::DataTable => {
                self.table_panel.ui(ui, ctx, project);
            },
            Panel::DownloadData => {
                self.dl_panel.ui(ui, ctx, async_ctx, project);
            },
            Panel::ProjInit => {
                self.proj_panel.ui(ui, ctx, async_ctx);
            },
            Panel::Empty => {
                self.empty_panel.ui(ui);
            },
        }
        // self.handle_progress_messages(async_ctx);
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
    pub fn settings_ui(
        &mut self,
        ctx: &egui::Context,
        async_ctx: &AsyncCtx,
        keybinds: &mut KeyBindings,
    ) {
        egui::SidePanel::right("Settings panel").show(ctx, |ui| {
        ScrollArea::vertical().show(ui, |ui| {

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        self.validation_panel.render_measurement_plots(ui);
                        self.validation_panel.enable_floaters(ui);
                    });

                    self.validation_panel.render_lin_plot_selection(ui);
                    ui.add(Separator::default().vertical());
                    self.validation_panel.render_exp_plot_selection(ui);
                    ui.add(Separator::default().vertical());
                    self.validation_panel.render_roblin_plot_selection(ui);
                    ui.add(Separator::default().vertical());
                    self.validation_panel.render_poly_plot_selection(ui);
                });
            });
            ui.group(|ui| {
                ui.label("Adjust plot point size");
                let pt_size = ui.add(
                    egui::DragValue::new(&mut self.validation_panel.plot_point_size)
                        .speed(0.01)
                        .range(1.0..=10.)
                );

                if pt_size.double_clicked() {
                    self.validation_panel.plot_point_size = 3.;
                }
                });

            ui.group(|ui| {
                ui.label("Adjust hiding thresholds");
                ui.label("These are based on the main gas.");
                ui.label("Will not mark measurements as invalid in the data, but allows hiding measurements in current view.");
                ui.label("Double click to reset");
                egui::Grid::new("thresholds_grid").min_col_width(100.).show(ui,|ui| {
                ui.label("RMSE");
                let rmse_adjuster = ui.add(
                    egui::DragValue::new(&mut self.validation_panel.rmse_thresh)
                        .speed(0.1)
                        .range(0.0..=100.)
                );
                if rmse_adjuster.changed() {
                    self.validation_panel.update_plots(&async_ctx);
                }
                if rmse_adjuster.double_clicked() {
                    self.validation_panel.rmse_thresh = 25.;
                    self.validation_panel.update_plots(&async_ctx);
                }
                ui.end_row();

                ui.label("r2");
                let r2_adjuster = ui.add(
                    egui::DragValue::new(&mut self.validation_panel.r2_thresh)
                        .speed(0.00001)
                        .range(0.0..=1.0)
                );
                if r2_adjuster.changed() {
                    self.validation_panel.update_plots(&async_ctx)
                }
                if r2_adjuster.double_clicked() {
                    self.validation_panel.r2_thresh = 0.9;
                    self.validation_panel.update_plots(&async_ctx);
                }
                ui.end_row();

                ui.label("p-value");
                let p_val_adjuster = ui.add(
                    egui::DragValue::new(&mut self.validation_panel.p_val_thresh)
                        .speed(0.0001)
                        .range(0.0..=1.0)
                );
                if p_val_adjuster.changed() {
                    self.validation_panel.update_plots(&async_ctx)
                }
                if p_val_adjuster.double_clicked() {
                    self.validation_panel.p_val_thresh = 0.05;
                    self.validation_panel.update_plots(&async_ctx);
                }
                        ui.end_row();

                ui.label("t0 concentration");
                let t0_adjuster= ui.add(
                    egui::DragValue::new(&mut self.validation_panel.t0_thresh)
                        .speed(1)
                        .range(0.0..=30000.0)
                );
                if t0_adjuster.changed() {
                    self.validation_panel.update_plots(&async_ctx)
                }
                if t0_adjuster.double_clicked() {
                    self.validation_panel.t0_thresh= 30000.;
                    self.validation_panel.update_plots(&async_ctx);
                }
            });
                        ui.end_row();
        });
            ui.separator();
            self.keybinding_settings_ui(ui, keybinds);
                });
        });
    }

    fn keybinding_settings_ui(&mut self, ui: &mut egui::Ui, keybinds: &mut KeyBindings) {
        ui.group(|ui| {
            ui.label("Keybinds");
            ui.label("Press rebind and hit key to set keybind");
            ui.label("Esc to cancel");
            egui::Grid::new("keybinds").show(ui, |ui| {
                for action in [
                    Action::NextCycle,
                    Action::PreviousCycle,
                    Action::ZoomToMeasurement,
                    Action::ResetCycle,
                    Action::SearchLagPrevious,
                    Action::SearchLag,
                    Action::IncrementLag,
                    Action::DecrementLag,
                    Action::IncrementDeadband,
                    Action::DecrementDeadband,
                    Action::IncrementCH4Deadband,
                    Action::DecrementCH4Deadband,
                    Action::IncrementCO2Deadband,
                    Action::DecrementCO2Deadband,
                    Action::ToggleValidity,
                    Action::ToggleCH4Validity,
                    Action::ToggleCO2Validity,
                    Action::ToggleH2OValidity,
                    Action::ToggleN2OValidity,
                    Action::ToggleBad,
                    Action::ToggleShowValids,
                    Action::ToggleShowInvalids,
                    Action::ToggleShowBad,
                    Action::ToggleShowSettings,
                    Action::ToggleShowLegend,
                    Action::ToggleShowDetails,
                    Action::TogglePlotWidthsWindow,
                    Action::ToggleShowResiduals,
                    Action::ToggleShowStandResiduals,
                    Action::ToggleShowLag,
                ] {
                    let mut rebind_text = "Rebind";
                    if let Some(pending) = keybinds.awaiting_rebind {
                        if pending == action {
                            rebind_text = "Press key to rebind";
                        }
                    }
                    ui.label(format!("{}:", action));
                    if let Some(key) = keybinds.key_for(action) {
                        ui.label(format!("{}", key));
                    } else {
                        ui.label("Unbound");
                    }

                    if ui.button(rebind_text).clicked() {
                        keybinds.awaiting_rebind = Some(action);
                    }
                    if keybinds.key_for(action).is_some() && ui.button("Unbind").clicked() {
                        keybinds.remove(&action);
                        keybinds.save_to_file("keybinds.json").ok();
                        keybinds.awaiting_rebind = None;
                    }
                    ui.end_row();
                }
            });
        });

        if let Some(action) = keybinds.awaiting_rebind {
            // if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            //     awaiting_rebind = None; // cancel
            // } else {
            if let Some((key, modifiers)) = ui.input(|i| {
                i.raw.events.iter().find_map(|event| {
                    if let egui::Event::Key { key, pressed: true, .. } = event {
                        if *key != egui::Key::Escape {
                            Some((*key, i.modifiers))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            }) {
                let keybind = KeyBind {
                    key,
                    ctrl: modifiers.ctrl,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                };
                keybinds.set(action, keybind);
                keybinds.save_to_file("keybinds.json").ok();
                keybinds.awaiting_rebind = None;
            }
        }
    }
    fn apply_font_size(&self, ctx: &egui::Context, font_size: f32) {
        use egui::{FontId, TextStyle};

        let mut style = (*ctx.style()).clone();

        // Update font sizes for the main text styles
        style.text_styles = [
            (TextStyle::Heading, FontId::monospace(font_size + 6.0)),
            (TextStyle::Body, FontId::monospace(font_size)),
            (TextStyle::Monospace, FontId::monospace(font_size)),
            (TextStyle::Button, FontId::monospace(font_size)),
            (TextStyle::Small, FontId::monospace(font_size - 2.0)),
        ]
        .into();

        ctx.set_style(style);
    }

    pub fn handle_progress_messages(&mut self, async_ctx: &mut AsyncCtx) {
        if let Some(mut receiver) = async_ctx.prog_receiver.take() {
            drain_progress_messages(self, &mut receiver);

            async_ctx.prog_receiver = Some(receiver);
        }
    }
}

pub fn load_app_state(path: &Path) -> Result<AppState, Box<dyn std::error::Error>> {
    let data = std::fs::read_to_string(path)?;
    let state: AppState = serde_json::from_str(&data)?;
    Ok(state)
}
pub fn save_app_state(app: &ValidationApp, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let state = app.to_app_state();
    let json = serde_json::to_string_pretty(&state)?;
    let mut file = fs::File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}
impl ProcessEventSink for MainApp {
    fn on_query_event(&mut self, ev: &QueryEvent) {
        match ev {
            QueryEvent::InitStarted => {
                println!("No switching allowed");
                self.switching_allowed = false;
                self.validation_panel.init_in_progress = true;
                self.validation_panel.recalc.calc_in_progress = true;
            },
            QueryEvent::InitEnded => {
                self.switching_allowed = true;
                self.validation_panel.init_in_progress = false;
                self.validation_panel.recalc.calc_in_progress = false;
            },
            QueryEvent::QueryComplete => {
                self.validation_panel.query_in_progress = false;
                self.log_messages.push_front(good_message("Finished queries."));
                self.validation_panel.recalc.query_in_progress = false;
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
            ProgressEvent::DisableUI => {
                self.switching_allowed = false;
                self.file_panel.reading_in_progress = true;
                self.validation_panel.init_enabled = false;
                self.validation_panel.recalc.calc_enabled = false;
            },
            ProgressEvent::EnableUI => {
                self.switching_allowed = true;
                self.file_panel.reading_in_progress = false;
                self.validation_panel.init_enabled = true;
                self.validation_panel.recalc.calc_enabled = true;
            },
            ProgressEvent::Rows(current, total) => {
                self.validation_panel.cycles_state = Some((*current, *total));
                self.validation_panel.cycles_progress += current;
                println!("Processed {} out of {} cycles", current, total);
            },
            ProgressEvent::Recalced(current, total) => {
                self.validation_panel.recalc.cycles_state = Some((*current, *total));
                self.validation_panel.recalc.cycles_progress += current;
                println!("Processed {} out of {} cycles", current, total);
            },
            ProgressEvent::CalculationStarted => {
                self.validation_panel.recalc.calc_enabled = false;
                self.validation_panel.recalc.calc_in_progress = true;
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
                self.switching_allowed = true;
                self.validation_panel.cycles_progress = 0;
                self.validation_panel.init_in_progress = false;
                self.validation_panel.init_enabled = true;
                self.validation_panel.query_in_progress = false;
                self.validation_panel.recalc.calc_enabled = true;
                self.validation_panel.recalc.calc_in_progress = false;
                self.validation_panel.recalc.query_in_progress = false;
                self.validation_panel.recalc.cycles_progress = 0;
                self.validation_panel.recalc.cycles_state = None;
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

        println!("Reset app state");
        self.switching_allowed = true;
        self.validation_panel.cycles_progress = 0;
        self.validation_panel.init_in_progress = false;
        self.validation_panel.init_enabled = true;
        self.validation_panel.query_in_progress = false;
        self.validation_panel.recalc.calc_enabled = true;
        self.validation_panel.recalc.calc_in_progress = false;
        self.validation_panel.recalc.query_in_progress = false;
        self.validation_panel.recalc.cycles_progress = 0;
        self.validation_panel.recalc.cycles_state = None;
    }
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
