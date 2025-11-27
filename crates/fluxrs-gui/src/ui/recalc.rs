use fluxrs_core::cycle::cycle::load_cycles;
use fluxrs_core::cycle_recalcer::{Datasets, Infra, Recalcer};
use fluxrs_core::data_formats::chamberdata::query_chamber_async;
use fluxrs_core::data_formats::heightdata::query_height_async;
use fluxrs_core::data_formats::meteodata::query_meteo_async;
use fluxrs_core::processevent::{
    InsertEvent, ProcessEvent, ProcessEventSink, ProgressEvent, QueryEvent, ReadEvent,
};
use fluxrs_core::project::Project;
use tokio::sync::mpsc::error::TryRecvError;

use crate::ui::manage_proj::project_ui::input_block_overlay;
use crate::utils::{bad_message, good_message, warn_message};

use chrono::{DateTime, TimeZone, Utc};
use eframe::egui::{Align2, Color32, Context, Frame, Ui, Window};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub struct RecalculateApp {
    pub calc_enabled: bool,
    pub calc_in_progress: bool,
    pub query_in_progress: bool,
    pub cycles_progress: usize,
    pub cycles_state: Option<(usize, usize)>,
}

impl Default for RecalculateApp {
    fn default() -> Self {
        Self::new()
    }
}
impl RecalculateApp {
    pub fn new() -> Self {
        Self {
            calc_enabled: true,
            calc_in_progress: false,
            query_in_progress: false,
            cycles_progress: 0,
            cycles_state: None,
        }
    }

    pub fn calculate(
        &self,
        runtime: &tokio::runtime::Runtime,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        project: Project,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) {
        let conn = match Connection::open("fluxrs.db") {
            Ok(conn) => conn,
            Err(e) => {
                let _ =
                    progress_sender.send(ProcessEvent::Query(QueryEvent::DbFail(e.to_string())));
                // log_messages.push_front(bad_message(&"Failed to open database."));
                return;
            },
        };
        let progsender = progress_sender.clone();

        let arc_conn = Arc::new(Mutex::new(conn));
        let proj = project.clone();

        // TODO: load_cycles queries chambers internally so they are getting set twice.
        runtime.spawn(async move {
            let cycle_result = load_cycles(
                arc_conn.clone(),
                start_date,
                end_date,
                proj.clone(),
                progress_sender.clone(),
            )
            .await;
            let meteo_result =
                query_meteo_async(arc_conn.clone(), start_date, end_date, proj.clone()).await;
            let height_result =
                query_height_async(arc_conn.clone(), start_date, end_date, proj.clone()).await;
            let chamber_result = query_chamber_async(arc_conn.clone(), proj.clone()).await;

            match (cycle_result, meteo_result, height_result, chamber_result) {
                (Ok(cycle_data), Ok(meteo_data), Ok(height_data), Ok(chamber_data)) => {
                    let _ = progress_sender.send(ProcessEvent::Query(QueryEvent::QueryComplete));
                    let _ = progsender
                        .send(ProcessEvent::Progress(ProgressEvent::Recalced(0, cycle_data.len())));
                    if !cycle_data.is_empty() {
                        let processor = Recalcer::new(
                            project.clone(),
                            Datasets {
                                meteo: meteo_data,
                                height: height_data,
                                chambers: chamber_data,
                            },
                            Infra { conn: arc_conn, progress: progress_sender },
                        );
                        processor.run_recalculating(cycle_data).await;
                    } else if cycle_data.is_empty() {
                        let msg = "No gas data or cycles found.";
                        let _ = progress_sender.send(ProcessEvent::Done(Err(msg.to_owned())));
                    }
                },
                e => {
                    eprintln!("Failed to query database: {:?}", e);
                    let msg = format!("Failed to query database: {:?}", e);
                    let _ = progress_sender.send(ProcessEvent::Done(Err(msg)));
                },
            }
        });
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        runtime: &tokio::runtime::Runtime,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        project: Project,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) {
        ui.vertical(|ui| {
            ui.label("Compare the current chamber measurementsa and meteo data of all calculated fluxes and recalculate if a new one is found.");
            ui.label("Does not change the adjusted calculation areas in any way.");


            if ui.add_enabled(self.calc_enabled && !self.calc_in_progress,egui::Button::new("Recalculate.")).clicked() {
                    self.calc_enabled = false;
                    self.query_in_progress = true;
                    self.calculate(runtime, start_date, end_date, project, progress_sender)

            }
            if self.calc_in_progress || !self.calc_enabled || self.query_in_progress {
                input_block_overlay(ctx, "blocker");

                Window::new("Manage project")
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 100.0))
                    .frame(
                        Frame::window(&ctx.style())
                            .fill(Color32::from_rgb(30, 30, 30))
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(16, 12)),
                    )
                    .show(ctx, |ui| {

                        ui.add(egui::Spinner::new());

                        if self.query_in_progress {
                            ui.label("Querying data, this can take a while for large time ranges.");
                        } else if self.calc_in_progress {
                            ui.label("Recalculating fluxes");

                        }

                        if let Some((_, total)) = self.cycles_state {

                            let total = total.max(1); // avoid division by zero
                            let fraction = (self.cycles_progress as f32 / total as f32).clamp(0.0, 1.0);
                            let pb =
                                egui::widgets::ProgressBar::new(fraction)
                                    .desired_width(200.)
                                    .corner_radius(1)
                                    .show_percentage()
                                    .text(format!("{}/{}", self.cycles_progress, total));
                            ui.add(pb);
                        }
                });
            }
    });
    }
    pub fn calculate_all(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        project: Project,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) {
        // 1970-01-01 to 2100-01-01 in UTC (wide and safe)
        let start = Utc.timestamp_opt(0, 0).unwrap();
        let end = Utc.timestamp_opt(4_102_444_800, 0).unwrap();
        self.calculate(runtime, start, end, project, progress_sender);
    }
}
impl ProcessEventSink for RecalculateApp {
    fn on_query_event(&mut self, ev: &QueryEvent) {
        match ev {
            QueryEvent::InitStarted => {
                self.calc_in_progress = true;
            },
            QueryEvent::InitEnded => {
                self.calc_in_progress = false;
            },
            QueryEvent::QueryComplete => {
                // self.query_in_progress = false;
                // self.log_messages.push_front(good_message("Finished queries."));
                self.query_in_progress = false;
            },
            QueryEvent::HeightFail(msg) => {
                // self.log_messages.push_front(bad_message(msg));
            },
            QueryEvent::CyclesFail(msg) => {
                // self.log_messages.push_front(bad_message(msg));
            },
            QueryEvent::DbFail(msg) => {
                // self.log_messages.push_front(bad_message(msg));
            },
            QueryEvent::NoGasData(start_time) => {
                // self.log_messages.push_front(bad_message(&format!(
                //     "No gas data found for cycle at {}",
                //     start_time
                // )));
            },
            QueryEvent::NoGasDataDay(day) => {
                // self.log_messages.push_front(bad_message(&format!(
                //     "No gas data found for cycles at day {}",
                //     day
                // )));
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
                self.cycles_state = Some((*current, *total));
                self.cycles_progress += current;
                // println!("Processed {} out of {} cycles", current, total);
            },
            ProgressEvent::CalculationStarted => {
                self.calc_enabled = false;
                self.calc_in_progress = true;
            },
            ProgressEvent::Day(date) => {
                // self.log_messages.push_front(good_message(&format!("Loaded cycles from {}", date)));
            },
            ProgressEvent::NoGas(msg) => {
                // self.log_messages.push_front(bad_message(&format!("Gas missing: {}", msg)));
            },
            ProgressEvent::Generic(msg) => {
                // self.log_messages.push_front(good_message(msg));
            },
        }
    }

    fn on_read_event(&mut self, ev: &ReadEvent) {
        match ev {
            ReadEvent::File(filename) => {
                // self.log_messages.push_front(good_message(&format!("Read file: {}", filename)));
            },
            ReadEvent::FileDetail(filename, detail) => {
                // self.log_messages
                //     .push_front(good_message(&format!("Read file: {} {}", filename, detail)));
            },
            ReadEvent::DataFail { .. } => {},
            ReadEvent::FileRows(filename, rows) => {
                // self.log_messages.push_front(good_message(&format!(
                //     "Read file: {} with {} rows",
                //     filename, rows
                // )));
            },
            ReadEvent::RowFail(msg) => {
                // self.log_messages.push_front(bad_message(&msg.to_owned()));
            },
            ReadEvent::FileFail(filename, e) => {
                // self.log_messages.push_front(bad_message(&format!(
                //     "Failed to read file {}, error: {}",
                //     filename, e
                // )));
            },
        }
    }

    fn on_insert_event(&mut self, ev: &InsertEvent) {
        match ev {
            InsertEvent::Ok(msg, rows) => {
                // self.log_messages.push_front(good_message(&format!("{}{}", rows, msg)));
            },
            InsertEvent::DataOkSkip { .. } => {},
            InsertEvent::Fail(e) => {
                // self.log_messages.push_front(bad_message(&format!("Failed to insert rows: {}", e)));
            },
        }
    }

    fn on_done(&mut self, res: &Result<(), String>) {
        match res {
            Ok(()) => {
                // self.log_messages.push_front(good_message("All processing finished."));
            },
            Err(e) => {
                // self.log_messages
                //     .push_front(bad_message(&format!("Processing finished with error: {}", e)));
            },
        }

        self.cycles_progress = 0;
        // self.init_in_progress = false;
        // self.init_enabled = true;
        self.query_in_progress = false;
        self.calc_enabled = true;
        self.calc_in_progress = false;
        self.query_in_progress = false;
        self.cycles_progress = 0;
        self.cycles_state = None;
    }
}
