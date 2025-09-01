use crate::cycle::load_cycles;
use crate::ui::validation_ui::ValidationApp;
use eframe::egui::Context;
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
}
