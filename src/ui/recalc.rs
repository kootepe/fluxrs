use crate::cycle::cycle::load_cycles;
use crate::data_formats::chamberdata::query_chamber_async;
use crate::data_formats::heightdata::query_height_async;
use crate::data_formats::meteodata::query_meteo_async;
use crate::processevent::{ProcessEvent, ProgressEvent, QueryEvent};
use crate::ui::manage_proj::project_ui::input_block_overlay;
use crate::Project;

use crate::ui::recalcer::{Datasets, Infra, Recalcer};
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
        &self,
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
