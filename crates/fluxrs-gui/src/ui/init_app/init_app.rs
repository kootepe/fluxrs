use crate::ui::main_app::DateRange;
use crate::ui::recalc::RecalculateApp;
use crate::ui::AsyncCtx;
use eframe::egui::Context;
use egui::{Color32, RichText};
use fluxrs_core::cycle_processor::{Datasets, Infra, Processor};
use fluxrs_core::data_formats::chamberdata::query_chamber_async;
use fluxrs_core::data_formats::gasdata::{query_gas_async, QueryError};
use fluxrs_core::data_formats::heightdata::query_height_async;
use fluxrs_core::data_formats::meteodata::query_meteo_async;
use fluxrs_core::data_formats::timedata::query_cycles_async;
use fluxrs_core::processevent::{ProcessEvent, ProgressEvent, QueryEvent};
use fluxrs_core::project::Project;
use rusqlite::Connection;
use std::sync::Arc;
use std::sync::Mutex;

use chrono::{DateTime, NaiveDateTime, TimeDelta, TimeZone};
use chrono_tz::{Tz, UTC};

#[derive(Default)]
pub struct InitApp {
    pub recalc: RecalculateApp,
    pub init_in_progress: bool,
    pub init_enabled: bool,
    pub cycles_progress: usize,
    pub cycles_state: Option<(usize, usize)>,
    pub query_in_progress: bool,
}

impl InitApp {
    pub fn new() -> Self {
        Self { init_enabled: true, ..Default::default() }
    }
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        async_ctx: &mut AsyncCtx,
        date_range: &mut DateRange,
        project: &Project,
    ) {
        // Show spinner if processing is ongoing
        if self.init_in_progress || !self.init_enabled {
            ui.add(egui::Spinner::new());
            if self.query_in_progress {
                ui.label("Querying data, this can take a while for large time ranges.");
            } else if let Some((_, total)) = self.cycles_state {
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
            return;
        }

        // Main UI layout
        let sender = async_ctx.prog_sender.clone();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                self.date_picker(ui, project, date_range);
                // Date navigation buttons

                let sender_clone = sender.clone();
                let start_after_end = date_range.start < date_range.end;
                // Trigger processing with selected date range
                if ui
                    .add_enabled(
                        self.init_enabled && !self.init_in_progress && start_after_end,
                        egui::Button::new("Initiate measurements").fill(Color32::DARK_GREEN),
                    )
                    .clicked()
                {
                    self.init_enabled = false;
                    self.init_in_progress = true;
                    self.query_in_progress = true;
                    let _ = async_ctx
                        .prog_sender
                        .send(ProcessEvent::Progress(ProgressEvent::DisableUI));

                    let start_date = date_range.start;
                    let end_date = date_range.end;
                    let project = project.clone();

                    let conn = match Connection::open("fluxrs.db") {
                        Ok(conn) => conn,
                        Err(e) => {
                            eprintln!("Failed to open database: {}", e);
                            return;
                        },
                    };
                    let arc_conn = Arc::new(Mutex::new(conn));

                    async_ctx.runtime.spawn(async move {
                        let start = start_date.to_utc();
                        let end = end_date.to_utc();
                        let cycles_result =
                            query_cycles_async(arc_conn.clone(), start, end, project.clone()).await;
                        let gas_result =
                            query_gas_async(arc_conn.clone(), start, end, project.clone()).await;
                        let meteo_result =
                            query_meteo_async(arc_conn.clone(), start, end, project.clone()).await;
                        let height_result =
                            query_height_async(arc_conn.clone(), start, end, project.clone()).await;
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
                                let _ = sender_clone
                                    .clone()
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
                                        Infra { conn: arc_conn, progress: sender_clone.clone() },
                                    );
                                    processor.run_processing_dynamic(times).await;
                                } else {
                                    // let _ = progress_sender.send(ProcessEvent::Query(
                                    //     QueryEvent::NoGasData("No data available".into()),
                                    // ));
                                    let _ = sender_clone.clone().send(ProcessEvent::Done(Err(
                                        "No data available.".to_owned(),
                                    )));
                                }
                            },
                            (_, Err(err), _, _, _) => {
                                let _ = sender_clone
                                    .clone()
                                    .send(ProcessEvent::Done(Err(err.to_string())));
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
            self.recalc.ui(
                ui,
                ctx,
                &async_ctx.runtime,
                date_range.start.to_utc(),
                date_range.end.to_utc(),
                project,
                sender.clone(),
            );
        });

        // Display log messages
    }
    pub fn date_picker(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        date_range: &mut DateRange,
    ) {
        let mut picker_start = date_range.start.date_naive();
        let mut picker_end = date_range.end.date_naive();
        let user_tz = &project.tz;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                // Start Date Picker
                ui.label("Pick start date:");
                if ui
                    .add(
                        egui_extras::DatePickerButton::new(&mut picker_start)
                            .highlight_weekends(false)
                            .id_salt("start_date"),
                    )
                    .changed()
                {
                    let naive = NaiveDateTime::from(picker_start);
                    let pick: DateTime<Tz> = user_tz.clone().from_local_datetime(&naive).unwrap();
                    date_range.start = pick;
                }
            });

            ui.vertical(|ui| {
                ui.label("Pick end date:");
                if ui
                    .add(
                        egui_extras::DatePickerButton::new(&mut picker_end)
                            .highlight_weekends(false)
                            .id_salt("end_date"),
                    )
                    .changed()
                {
                    let naive = NaiveDateTime::from(picker_end);
                    let pick: DateTime<Tz> = user_tz.clone().from_local_datetime(&naive).unwrap();
                    date_range.end = pick + TimeDelta::seconds(86399);
                }
            });
        });

        let start_before_end = date_range.start < date_range.end;

        if start_before_end {
            let delta = date_range.end - date_range.start;

            if let Ok(duration) = delta.to_std() {
                let total_secs = duration.as_secs();

                let days = total_secs / 86_400;
                let hours = (total_secs % 86_400) / 3_600;
                let minutes = (total_secs % 3_600) / 60;
                let seconds = total_secs % 60;

                let duration_str = if days > 0 {
                    format!("{}d {:02}h {:02}m {:02}s", days, hours, minutes, seconds)
                } else if hours > 0 {
                    format!("{:02}h {:02}m {:02}s", hours, minutes, seconds)
                } else if minutes > 0 {
                    format!("{:02}m {:02}s", minutes, seconds)
                } else {
                    format!("{:02}s", seconds)
                };
                ui.label(format!("From: {}", date_range.start));
                ui.label(format!("to: {}", date_range.end));

                ui.label(format!("Duration: {}", duration_str));

                // Buttons with full duration string
                if ui
                    .add_enabled(true, egui::Button::new(format!("Next ({})", duration_str)))
                    .clicked()
                {
                    date_range.start += delta;
                    date_range.end += delta;
                }

                if ui
                    .add_enabled(true, egui::Button::new(format!("Previous ({})", duration_str)))
                    .clicked()
                {
                    date_range.start -= delta;
                    date_range.end -= delta;
                }
            }
        }
    }
}
