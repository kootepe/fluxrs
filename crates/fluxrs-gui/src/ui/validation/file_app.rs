use crate::ui::tz_picker::timezone_combo;
use crate::ui::tz_picker::TimezonePickerState;
use chrono_tz::{Tz, UTC};
use egui::{Context, RichText, Ui};
use egui_file::FileDialog;
use fluxrs_core::data_formats::chamberdata::upload_chamber_metadata_async;
use fluxrs_core::data_formats::heightdata::upload_height_data_async;
use fluxrs_core::data_formats::meteodata::upload_meteo_data_async;
use fluxrs_core::data_formats::timedata::upload_cycle_data_async;
use fluxrs_core::datatype::DataType;
use fluxrs_core::instruments::instruments::upload_gas_data_async;
use fluxrs_core::instruments::instruments::InstrumentType;
use fluxrs_core::processevent::{ProcessEvent, QueryEvent};
use fluxrs_core::project::Project;
use rusqlite::Connection;
use tokio::sync::mpsc;

use std::borrow::Cow;
use std::collections::VecDeque;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

pub struct FileApp {
    pub opened_files: Option<Vec<PathBuf>>,
    pub open_file_dialog: Option<FileDialog>,
    pub initial_path: Option<PathBuf>,
    pub selected_data_type: Option<DataType>,

    pub tz_prompt_open: bool,
    pub tz_state: TimezonePickerState,
    pub tz_for_files: Option<Tz>,
}

impl Default for FileApp {
    fn default() -> Self {
        Self::new()
    }
}

impl FileApp {
    pub fn new() -> Self {
        Self {
            opened_files: None,
            open_file_dialog: None,
            initial_path: Some(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            selected_data_type: None,

            tz_prompt_open: false,
            tz_state: TimezonePickerState::default(),
            tz_for_files: None,
        }
    }
    pub fn file_ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        init_enabled: bool,
        init_in_progress: &mut bool,
        selected_project: &mut Option<Project>,
        log_messages: &mut VecDeque<RichText>,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
        runtime: &tokio::runtime::Runtime,
    ) {
        if selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }

        if *init_in_progress || !init_enabled {
            ui.add(egui::Spinner::new());
            ui.label("Reading files.");
        }

        let mut gas_btn_text = "Select Analyzer Files".to_owned();

        if let Some(project) = selected_project.as_mut() {
            project.upload_from = Some(project.upload_from.unwrap_or(project.instrument.model));
            let current_value = project.upload_from.unwrap();
            if self.tz_state.selected.is_none() {
                self.tz_state.selected = Some(project.tz)
            }

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

            gas_btn_text = format!("Select {} Files", &current_value);
        }

        let btns_enabled = init_enabled && !*init_in_progress;
        ui.add_enabled(btns_enabled, |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                if ui.button(&gas_btn_text).clicked() {
                    self.selected_data_type = Some(DataType::Gas);
                    self.open_file_dialog(&gas_btn_text);
                }
                if ui.button("Select Cycle Files").clicked() {
                    self.selected_data_type = Some(DataType::Cycle);
                    self.open_file_dialog("Select Cycle Files");
                }
                if ui.button("Select Meteo Files").clicked() {
                    self.selected_data_type = Some(DataType::Meteo);
                    self.open_file_dialog("Select Meteo Files");
                }
                if ui.button("Select Height Files").clicked() {
                    self.selected_data_type = Some(DataType::Height);
                    self.open_file_dialog("Select Height Files");
                }
                if ui.button("Select Chamber Metadata File").clicked() {
                    self.selected_data_type = Some(DataType::Chamber);
                    self.open_file_dialog("Select Chamber Metadata File");
                }
            })
            .response
        });

        self.handle_file_selection(ctx, log_messages, selected_project);
        self.start_processing_if_ready(
            selected_project,
            log_messages,
            init_in_progress,
            progress_sender,
            runtime,
        );
        self.show_timezone_prompt(ctx);
    }

    pub fn open_file_dialog(&mut self, title: &str) {
        let mut dialog = FileDialog::open_file(self.initial_path.clone())
            .title(title)
            .open_button_text(Cow::from("Upload"))
            .multi_select(true)
            .show_rename(false)
            .show_new_folder(false);

        dialog.open();
        self.open_file_dialog = Some(dialog);
    }

    pub fn handle_file_selection(
        &mut self,
        ctx: &Context,
        log_messages: &mut VecDeque<RichText>,
        project: &mut Option<Project>,
    ) {
        if let Some(dialog) = &mut self.open_file_dialog {
            dialog.show(ctx);

            match dialog.state() {
                egui_file::State::Selected => {
                    let selected_paths: Vec<PathBuf> =
                        dialog.selection().into_iter().map(|p: &Path| p.to_path_buf()).collect();

                    if !selected_paths.is_empty() {
                        self.opened_files = Some(selected_paths.clone());

                        // Only open the timezone prompt if we actually need it
                        if !self.current_gas_instrument_has_tz(project) {
                            // non-gas OR gas instrument without its own TZ
                            self.tz_prompt_open = true;
                            self.tz_state.focus_search_once = true;
                        } else {
                            // gas instrument *with* TZ info in the file → no prompt
                            self.tz_prompt_open = false;
                            // Optional: clear any previous manual TZ
                            // self.tz_for_files = None;
                        }
                    }

                    self.open_file_dialog = None; // Close the dialog
                },
                egui_file::State::Cancelled | egui_file::State::Closed => {
                    if let Some(dt) = self.selected_data_type {
                        log_messages.push_front(format!("{dt:?} file selection cancelled.").into());
                    }
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
        tz: Tz,
        log_messages: Arc<Mutex<VecDeque<RichText>>>,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
        runtime: &tokio::runtime::Runtime,
    ) {
        let log_messages_clone = Arc::clone(&log_messages);
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
                                        tz,
                                        progress_sender,
                                    )
                                },
                                DataType::Cycle => {
                                    let _ = progress_sender
                                        .send(ProcessEvent::Query(QueryEvent::InitStarted));
                                    upload_cycle_data_async(
                                        path_list,
                                        &mut conn,
                                        &project_clone,
                                        tz,
                                        progress_sender,
                                    );
                                },
                                DataType::Meteo => {
                                    let _ = progress_sender
                                        .send(ProcessEvent::Query(QueryEvent::InitStarted));
                                    upload_meteo_data_async(
                                        path_list,
                                        &mut conn,
                                        &project_clone,
                                        tz,
                                        progress_sender,
                                    )
                                },
                                DataType::Height => {
                                    let _ = progress_sender
                                        .send(ProcessEvent::Query(QueryEvent::InitStarted));
                                    upload_height_data_async(
                                        path_list,
                                        &mut conn,
                                        &project_clone,
                                        tz,
                                        progress_sender,
                                    )
                                },
                                DataType::Chamber => {
                                    let _ = progress_sender
                                        .send(ProcessEvent::Query(QueryEvent::InitStarted));
                                    upload_chamber_metadata_async(
                                        path_list,
                                        &mut conn,
                                        &project_clone,
                                        tz,
                                        progress_sender,
                                    )
                                },
                            }
                        }
                    },
                    Err(e) => {
                        let mut logs = log_messages.lock().unwrap();
                        logs.push_front(format!("Failed to connect to database: {e}").into());
                    },
                })
                .await;

            if let Err(e) = join_result {
                let mut logs = log_messages_clone.lock().unwrap();
                let _ =
                    sender_clone.send(ProcessEvent::Done(Err("Thread join failure".to_owned())));
                logs.push_front(format!("Join error: {e}").into());
            }
        });
    }

    // Check that current datatype is gas and then check if the current
    // instrument has has_tz as true. If both are true, returns true and tz_prompt will not prompt.
    fn current_gas_instrument_has_tz(&self, project: &mut Option<Project>) -> bool {
        let is_gas = self.selected_data_type == Some(DataType::Gas);
        if !is_gas {
            return false;
        }

        project
            .as_ref()
            .and_then(|p| p.upload_from)
            .map(|instrument| instrument.get_config().has_tz)
            .unwrap_or(false)
    }

    fn start_processing_if_ready(
        &mut self,
        selected_project: &Option<Project>,
        log_messages: &mut VecDeque<RichText>,
        init_in_progress: &mut bool,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
        runtime: &tokio::runtime::Runtime,
    ) {
        if *init_in_progress {
            return;
        }

        if self.tz_prompt_open {
            return;
        }

        let Some(paths) = self.opened_files.clone() else {
            return;
        };

        let Some(project) = selected_project.clone() else {
            return;
        };

        let has_instrument_tz =
            FileApp::project_gas_instrument_has_tz(selected_project, self.selected_data_type);

        let tz = if has_instrument_tz {
            self.tz_for_files.unwrap_or(UTC)
        } else {
            match self.tz_for_files {
                Some(t) => t,
                None => {
                    self.tz_prompt_open = true;
                    return;
                },
            }
        };

        let arc_msgs = Arc::new(Mutex::new(log_messages.clone()));

        *init_in_progress = true;

        self.process_files_async(
            paths,
            self.selected_data_type,
            &project,
            tz,
            arc_msgs,
            progress_sender,
            runtime,
        );

        self.opened_files = None;
    }
    fn project_gas_instrument_has_tz(
        selected_project: &Option<Project>,
        dt: Option<DataType>,
    ) -> bool {
        if dt != Some(DataType::Gas) {
            return false;
        }

        selected_project
            .as_ref()
            .and_then(|p| p.upload_from)
            .map(|instrument| instrument.get_config().has_tz)
            .unwrap_or(false)
    }

    pub fn show_timezone_prompt(&mut self, ctx: &egui::Context) {
        if !self.tz_prompt_open {
            return;
        }

        egui::Area::new(egui::Id::from("tz_prompt_layer"))
            .fixed_pos(ctx.screen_rect().center()) // center-ish
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Window::new("Choose timezone for the selected files")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label("Type to search and pick the timezone used to interpret these files.");

                        // Searchable ComboBox
                        if let Some(tz) = timezone_combo(ui, "file_tz_combo", &mut self.tz_state) {
                            self.tz_for_files = Some(tz);
                        }

                        // Current selection preview
                        if let Some(tz) = self.tz_for_files {
                            ui.label(format!("Selected timezone: {tz}"));
                        } else {
                            ui.label("No timezone selected (will default to UTC).");
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("OK").clicked() {
                                // Lock in a TZ (even if defaulted), close dialog.
                                let tz = self.tz_for_files.unwrap_or(UTC);
                                self.tz_for_files = Some(tz);
                                self.tz_prompt_open = false;
                                // ❌ no processing here
                            }

                            if ui.button("Cancel").clicked() {
                                self.tz_prompt_open = false;
                                self.opened_files = None;
                            }
                        });
                    });
            });
    }
}
