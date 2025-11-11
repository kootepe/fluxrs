use crate::cycle::cycle::load_cycles;
use crate::processevent::ProcessEvent;
use crate::ui::validation_ui::ValidationApp;
use crate::utils::{bad_message, good_message, warn_message};
use eframe::egui::Context;
use egui::Color32;
use rusqlite::Connection;
use tokio::sync::mpsc;

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
                            self.log_messages.push_front(good_message(
                                &"Successfully loaded cycles.".to_string(),
                            ));
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
                let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
                self.progress_receiver = Some(progress_receiver);

                self.init_enabled = false;
                self.init_in_progress = true;

                self.runtime.spawn(async move {
                    let result = match Connection::open("fluxrs.db") {
                        Ok(conn) => load_cycles(
                            &conn,
                            &project,
                            start_date,
                            end_date,
                            progress_sender.clone(),
                        ),
                        Err(e) => {
                            let _ = progress_sender.send(ProcessEvent::Done(Err(e.to_string())));
                            Err(e)
                        },
                    };
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
