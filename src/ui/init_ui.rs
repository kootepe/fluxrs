use crate::data_formats::chamberdata::query_chamber_async;
use crate::data_formats::gasdata::query_gas_async;
use crate::data_formats::heightdata::query_height_async;
use crate::data_formats::meteodata::query_meteo_async;
use crate::data_formats::timedata::query_cycles_async;
use crate::processevent::{ProcessEvent, QueryEvent};
use crate::ui::validation_ui::{render_recalculate_ui, Datasets, Infra, Processor, ValidationApp};
use eframe::egui::Context;
use egui::Color32;
use rusqlite::Connection;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc;

impl ValidationApp {
    pub fn init_ui(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
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
        let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
        let sender_clone = progress_sender.clone();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                self.date_picker(ui);
                // Date navigation buttons

                let start_after_end = self.start_date < self.end_date;
                // Trigger processing with selected date range
                if ui
                    .add_enabled(
                        self.init_enabled && !self.init_in_progress && start_after_end,
                        egui::Button::new("Initiate measurements").fill(Color32::DARK_GREEN),
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
                        let height_result = query_height_async(
                            arc_conn.clone(),
                            start_date,
                            end_date,
                            project.clone(),
                        )
                        .await;
                        let chamber_result =
                            query_chamber_async(arc_conn.clone(), project.clone()).await;

                        match (
                            cycles_result,
                            gas_result,
                            meteo_result,
                            height_result,
                            chamber_result,
                        ) {
                            (
                                Ok(times),
                                Ok(gas_data),
                                Ok(meteo_data),
                                Ok(height_data),
                                Ok(chamber_data),
                            ) => {
                                let _ = progress_sender
                                    .send(ProcessEvent::Query(QueryEvent::QueryComplete));
                                if !times.start_time.is_empty() && !gas_data.is_empty() {
                                    let processor = Processor::new(
                                        project.clone(),
                                        Datasets {
                                            gas: Arc::new(gas_data),
                                            meteo: meteo_data,
                                            height: height_data,
                                            chambers: chamber_data,
                                        },
                                        Infra { conn: arc_conn, progress: progress_sender },
                                    );
                                    processor.run_processing_dynamic(times).await;
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
                if !start_after_end {
                    ui.label("Start date can't be later then end date");
                }
            });

            ui.separator();
            render_recalculate_ui(
                ui,
                &self.runtime,
                self.start_date,
                self.end_date,
                self.get_project().clone(),
                sender_clone,
            );
        });
        // Handle messages from background processing

        // Display log messages
        self.log_display(ui);
    }
}
