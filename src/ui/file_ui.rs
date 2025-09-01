use crate::instruments::InstrumentType;
use crate::mpsc;
use crate::validation_app::upload_chamber_metadata_async;
use crate::validation_app::upload_cycle_data_async;
use crate::validation_app::upload_gas_data_async;
use crate::validation_app::upload_height_data_async;
use crate::validation_app::upload_meteo_data_async;
use crate::validation_app::{DataType, ValidationApp};
use crate::Connection;
use crate::ProcessEvent;
use crate::Project;
use crate::QueryEvent;
use egui::{Context, Ui};
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
                                DataType::Cycle => {
                                    upload_cycle_data_async(
                                        path_list,
                                        &mut conn,
                                        &project_clone,
                                        progress_sender,
                                    );
                                },
                                DataType::Meteo => upload_meteo_data_async(
                                    path_list,
                                    &mut conn,
                                    &project_clone,
                                    progress_sender,
                                ),
                                DataType::Height => upload_height_data_async(
                                    path_list,
                                    &mut conn,
                                    &project_clone,
                                    progress_sender,
                                ),
                                DataType::Chamber => upload_chamber_metadata_async(
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
}
