use crate::instruments::instruments::InstrumentType;
use crate::mpsc;
use crate::ui::tz_picker::timezone_combo;
use crate::ui::validation_ui::upload_chamber_metadata_async;
use crate::ui::validation_ui::upload_cycle_data_async;
use crate::ui::validation_ui::upload_gas_data_async;
use crate::ui::validation_ui::upload_height_data_async;
use crate::ui::validation_ui::upload_meteo_data_async;
use crate::ui::validation_ui::{DataType, ValidationApp};
use crate::Connection;
use crate::ProcessEvent;
use crate::Project;
use crate::QueryEvent;
use chrono_tz::{Tz, UTC};
use egui::{Align2, Context, Frame, Id, RichText, Ui};
use egui_file::FileDialog;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

impl ValidationApp {
    pub fn file_ui(&mut self, ui: &mut Ui, ctx: &Context) {
        self.handle_progress_messages();
        self.show_timezone_prompt(ctx);
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        if self.init_in_progress || !self.init_enabled {
            ui.add(egui::Spinner::new());
            ui.label("Reading files.");
        }
        let mut gas_btn_text = "Select Analyzer Files".to_owned();
        if let Some(project) = self.selected_project.as_mut() {
            let current_value = project.upload_from.unwrap_or(project.instrument.model); // fallback display value

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

        let btns_disabled = self.init_enabled && !self.init_in_progress;
        ui.add_enabled(btns_disabled, |ui: &mut egui::Ui| {
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

        // Handle file selection
        self.handle_file_selection(ctx);

        self.log_display(ui);
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
    pub fn handle_file_selection(&mut self, ctx: &Context) {
        if let Some(dialog) = &mut self.open_file_dialog {
            dialog.show(ctx);

            match dialog.state() {
                egui_file::State::Selected => {
                    let selected_paths: Vec<PathBuf> =
                        dialog.selection().into_iter().map(|p: &Path| p.to_path_buf()).collect();

                    if selected_paths.is_empty() {
                    } else {
                        self.opened_files = Some(selected_paths.clone());
                        // open the timezone prompt next frame
                        self.tz_prompt_open = true;

                        self.tz_state.focus_search_once = true;
                    }

                    self.open_file_dialog = None; //   Close the dialog
                },
                egui_file::State::Cancelled | egui_file::State::Closed => {
                    self.log_messages.push_front("File selection cancelled.".into());
                    self.open_file_dialog = None;
                },
                _ => {}, // Do nothing if still open
            }
        }
    }

    pub fn show_timezone_prompt(&mut self, ctx: &egui::Context) {
        if !self.tz_prompt_open {
            return;
        }

        egui::Area::new(Id::from("tz_prompt_layer"))
        .fixed_pos(ctx.screen_rect().center()) // center-ish
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Window::new("Choose timezone for the selected files")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Type to search and pick the timezone used to interpret these files.");

                    // Searchable ComboBox
                    if let Some(tz) = timezone_combo(ui, "file_tz_combo_v031", &mut self.tz_state) {
                        self.tz_for_files = Some(tz);
                    }

                    // Current selection preview
                    if let Some(tz) = self.tz_for_files {
                        ui.label(format!("Selected timezone: {}", tz));
                    } else {
                        ui.label("No timezone selected (will default to UTC).");
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let tz = self.tz_for_files.unwrap_or(UTC);

                            // set up progress only when we're actually starting work
                            // let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
                            // self.progress_receiver = Some(progress_receiver);
                            let progress_sender = self.prog_sender.clone();

                            let arc_msgs = Arc::new(Mutex::new(self.log_messages.clone()));

                            if let Some(paths) = self.opened_files.clone() {
                                // OPTION A: if your processor can accept tz explicitly, pass it:
                                // self.process_files_async(paths, self.selected_data_type.clone(), self.get_project(), tz, arc_msgs, progress_sender, &self.runtime);

                                // OPTION B: stash tz onto your Project or app state, then call as-is:
                                // if let Some(mut project) = self.get_project_mut() {
                                //     project.timezone = Some(tz.to_string());
                                // }
                                self.process_files_async(
                                    paths,
                                    self.selected_data_type.clone(),
                                    self.get_project(),
                                    tz,
                                    arc_msgs,
                                    progress_sender,
                                    &self.runtime,
                                );
                            }

                            self.tz_prompt_open = false;
                            // keep `opened_files` if you want “re-run” ability; otherwise clear it
                            self.opened_files = None;
                        }

                        if ui.button("Cancel").clicked() {
                            self.tz_prompt_open = false;
                            self.opened_files = None;
                        }
                    });
                });
        });
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
                        logs.push_front(format!("Failed to connect to database: {}", e).into());
                    },
                })
                .await;
            if let Err(e) = join_result {
                let mut logs = log_messages_clone.lock().unwrap();

                let _ =
                    sender_clone.send(ProcessEvent::Done(Err("Thread join failure".to_owned())));
                logs.push_front(format!("Join error: {}", e).into());
            }
        });
    }
}
