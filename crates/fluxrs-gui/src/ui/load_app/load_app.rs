use crate::ui::date_picker;
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
            date_picker(ui, project, date_range);

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
}
