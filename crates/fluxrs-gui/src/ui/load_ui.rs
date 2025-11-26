use crate::ui::validation_ui::ValidationApp;
use crate::utils::{bad_message, good_message, warn_message};
use eframe::egui::Context;
use egui::Color32;
use fluxrs_core::cycle::cycle::{load_cycles_sync, AppError, Cycle};
use fluxrs_core::processevent::ProcessEvent;
use rusqlite::Connection;

impl ValidationApp {
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
                            self.log_messages
                                .push_front(good_message("Successfully loaded cycles."));
                        },
                        Err(e) => {
                            self.log_messages.push_front(bad_message(&format!("Error: {}", e)));
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

            let start_after_end = self.start_date < self.end_date;
            if ui
                .add_enabled(
                    self.init_enabled && !self.init_in_progress && start_after_end,
                    egui::Button::new("Load measurements").fill(Color32::DARK_GREEN),
                )
                .clicked()
            {
                self.commit_all_dirty_cycles();
                let sender = self.task_done_sender.clone();
                let result_slot = self.load_result.clone();
                let start_date = self.start_date;
                let end_date = self.end_date;
                let project = self.get_project().clone();
                let progress_sender = self.prog_sender.clone();

                self.init_enabled = false;
                self.init_in_progress = true;

                // TODO: Use AppError for clearer error messages.
                self.runtime.spawn(async move {
                    let result: Result<Vec<Cycle>, AppError> = match Connection::open("fluxrs.db") {
                        Ok(conn) => load_cycles_sync(
                            &conn,
                            &project,
                            start_date.timestamp(),
                            end_date.timestamp(),
                            progress_sender.clone(),
                        ),

                        Err(e) => {
                            // db open failed
                            let _ = progress_sender.send(ProcessEvent::Done(Err(e.to_string())));
                            Err(AppError::from(e)) // <-- convert to AppError
                        },
                    };

                    // Optional: send a nicer message for specific errors
                    if let Err(ref err) = result {
                        match err {
                            AppError::NoRows(msg) => {
                                let _ = progress_sender.send(ProcessEvent::Done(Err(msg.clone())));
                            },
                            _ => {
                                let _ =
                                    progress_sender.send(ProcessEvent::Done(Err(err.to_string())));
                            },
                        }
                    }
                    if let Ok(mut slot) = result_slot.lock() {
                        *slot = Some(result);
                    }
                    let _ = sender.send(());
                });
            }
            if !start_after_end {
                ui.label("Start date can't be later then end date");
            }
        }
        self.log_display(ui);
    }
}
