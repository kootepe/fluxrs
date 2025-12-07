use crate::ui::main_app::DateRange;
use crate::ui::AsyncCtx;
use crate::ui::ValidationApp;
use crate::utils::{bad_message, good_message, warn_message};
use chrono::{DateTime, NaiveDateTime, TimeDelta, TimeZone};
use chrono_tz::{Tz, UTC};
use eframe::egui::Context;
use egui::{Color32, RichText};
use fluxrs_core::cycle::cycle::{load_cycles_sync, AppError, Cycle};
use fluxrs_core::processevent::{ProcessEvent, QueryEvent};
use fluxrs_core::project::Project;
use rusqlite::Connection;
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

type LoadResult = Arc<Mutex<Option<Result<Vec<Cycle>, AppError>>>>;

pub struct LoadAsyncState {
    pub result: LoadResult,
    pub done_sender: Sender<()>,
    pub done_receiver: Receiver<()>,
}

impl LoadAsyncState {
    pub fn new() -> Self {
        let (done_sender, done_receiver) = std::sync::mpsc::channel();
        let result = Arc::new(Mutex::new(None));
        Self { done_sender, done_receiver, result }
    }
}

impl Default for LoadAsyncState {
    fn default() -> Self {
        Self::new()
    }
}
pub struct LoadApp {
    pub load_in_progress: bool,
    load_enabled: bool,
    load_state: LoadAsyncState,
}

impl Default for LoadApp {
    fn default() -> Self {
        Self::new()
    }
}
impl LoadApp {
    pub fn new() -> Self {
        Self { load_in_progress: false, load_enabled: true, load_state: LoadAsyncState::default() }
    }
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &Context,
        async_ctx: &mut AsyncCtx,
        validation_app: &mut ValidationApp,
        project: &Project,
        date_range: &mut DateRange,
        log_msgs: &mut VecDeque<RichText>,
    ) {
        if self.load_state.done_receiver.try_recv().is_ok() {
            self.load_in_progress = false;
            self.load_enabled = true;

            if let Ok(mut result_lock) = self.load_state.result.lock() {
                if let Some(result) = result_lock.take() {
                    match result {
                        Ok(cycles) => {
                            validation_app.cycles = cycles;
                            log_msgs.push_front(good_message("Successfully loaded cycles."));
                        },
                        Err(e) => {
                            log_msgs.push_front(bad_message(&format!("Error: {}", e)));
                        },
                    }
                }
            }
            validation_app.update_plots(async_ctx);
        }

        if self.load_in_progress || !self.load_enabled {
            ui.add(egui::Spinner::new());
            ui.label("Loading fluxes from db...");
            // return; // optionally stop drawing the rest of the UI while loading
        } else {
            self.date_picker(ui, project, date_range);

            let start_after_end = date_range.start < date_range.end;
            if ui
                .add_enabled(
                    self.load_enabled && !self.load_in_progress && start_after_end,
                    egui::Button::new("Load measurements").fill(Color32::DARK_GREEN),
                )
                .clicked()
            {
                validation_app.commit_all_dirty_cycles(async_ctx);
                let sender = self.load_state.done_sender.clone();
                let result_slot = self.load_state.result.clone();
                let start_date = date_range.start;
                let end_date = date_range.end;
                let project = project.clone();
                let progress_sender = async_ctx.prog_sender.clone();

                self.load_enabled = false;
                self.load_in_progress = true;
                let _ = async_ctx.prog_sender.send(ProcessEvent::Query(QueryEvent::InitStarted));

                // TODO: Use AppError for clearer error messages.
                async_ctx.runtime.spawn(async move {
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
                    //
                    if let Ok(mut slot) = result_slot.lock() {
                        *slot = Some(result);
                    }
                    let _ = sender.send(());
                    let _ = progress_sender.send(ProcessEvent::Done(Ok(())));
                });
            }
            if !start_after_end {
                ui.label("Start date can't be later then end date");
            }
        }
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
                    let pick: DateTime<Tz> = user_tz.from_local_datetime(&naive).unwrap();
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
                    let pick: DateTime<Tz> = user_tz.from_local_datetime(&naive).unwrap();
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
