use crate::ui::manage_proj::datepickerstate::DateRangePickerState;
use crate::ui::manage_proj::project::Project;
use crate::ui::recalc::RecalculateApp;
use crate::ui::validation_ui::DataType;
use chrono::NaiveDateTime;
use egui::{Align2, Color32, Context, Frame, Ui, Window};
use egui::{RichText, ScrollArea, WidgetInfo, WidgetType};
use rusqlite::{params, Connection};
use std::collections::HashSet;

use crate::cycle::cycle::{load_cycles, Cycle};
use crate::data_formats::chamberdata::read_chamber_metadata;
use crate::data_formats::heightdata::query_height;
use crate::processevent::ProcessEvent;
use crate::ui::recalcer::{Datasets, Infra, Recalcer};
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone)]
struct DataFileRow {
    id: i64,
    file_name: String,
    data_type: String,
    uploaded_at: Option<NaiveDateTime>,
    project_link: i64,
}

pub struct DeleteMeasurementApp {
    project: Project,
    date_range: DateRangePickerState,
    reload_requested: bool,
    selected_ids: HashSet<i64>,
    files: Vec<DataFileRow>,
    last_error: Option<String>,
    datatype: DataType,
}

impl DeleteMeasurementApp {
    fn new(project: Project, datatype: DataType) -> Self {
        let mut app = Self {
            project,
            date_range: DateRangePickerState::default(),
            files: Vec::new(),
            selected_ids: HashSet::new(),
            reload_requested: true,
            last_error: None,
            datatype,
        };
        app.reload_files(); // initial load
        app
    }

    fn ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        project: Project,
        datatype: DataType,
        recalc: &RecalculateApp,
        runtime: &tokio::runtime::Runtime,
        progress_sender: UnboundedSender<ProcessEvent>,
    ) {
        self.project = project;

        if self.datatype != datatype {
            self.datatype = datatype;
            self.reload_requested = true;
        }

        if self.reload_requested {
            self.reload_files();
            self.reload_requested = false;
        }
        // ui.vertical_centered(|ui| {
        //     ui.heading("Delete fluxes");
        //     ui.separator();
        // });
        // self.date_range.date_picker(ui, ctx);
        ui.heading("Data Files");

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.reload_requested = true;
            }
            if ui.button("Delete selected").clicked() && !self.selected_ids.is_empty() {
                match self.delete_selected() {
                    Ok(n) if n > 0 => {
                        match self.datatype {
                            DataType::Meteo | DataType::Height | DataType::Chamber => {
                                recalc.calculate_all(
                                    runtime,
                                    self.project.clone(),
                                    progress_sender.clone(),
                                );
                            },
                            _ => {
                                // No recalculation for Gas or Cycle deletions
                            },
                        }
                    },
                    Ok(_) => { /* nothing deleted */ },
                    Err(err) => {
                        self.last_error = Some(err);
                    },
                }
            }

            ui.label(format!("Total files: {}", self.files.len()));
        });

        if let Some(err) = &self.last_error {
            ui.colored_label(egui::Color32::RED, err);
        }

        ui.separator();

        let cols = ["", "ID", "File name", "Type", "Upload date"];
        ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("data_table")
                .striped(true)
                .num_columns(cols.len())
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    // Header row
                    for col in cols {
                        ui.label(RichText::new(col).strong());
                    }
                    ui.end_row();

                    // Data rows
                    for file in &self.files {
                        // 1) checkbox column
                        let mut checked = self.selected_ids.contains(&file.id);
                        if ui.checkbox(&mut checked, "").changed() {
                            if checked {
                                self.selected_ids.insert(file.id);
                            } else {
                                self.selected_ids.remove(&file.id);
                            }
                        }

                        // 2) ID
                        ui.label(file.id.to_string());

                        // 3) File name
                        ui.label(&file.file_name);

                        // 4) Typd
                        ui.label(&file.data_type);

                        // 5) Upload date
                        let uploaded = file
                            .uploaded_at
                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "-".to_string());
                        ui.label(uploaded);
                        ui.end_row();
                    }
                });
        });
    }

    fn reload_files(&mut self) {
        self.files.clear();
        self.selected_ids.clear();
        self.last_error = None;

        let conn = Connection::open("fluxrs.db").expect("Failed to open database");
        let mut stmt = match conn.prepare(
            "
            SELECT id, file_name, data_type, uploaded_at, project_link
            FROM data_files
            WHERE data_type = ?1
            ORDER BY uploaded_at DESC, id DESC
            ",
        ) {
            Ok(stmt) => stmt,
            Err(e) => {
                self.last_error = Some(format!("Failed to prepare query: {e}"));
                return;
            },
        };

        let rows = stmt.query_map([self.datatype.type_str()], |row| {
            Ok(DataFileRow {
                id: row.get(0)?,
                file_name: row.get(1)?,
                data_type: row.get(2)?,
                uploaded_at: row.get(3).ok(), // TIMESTAMP -> Option<NaiveDateTime>
                project_link: row.get(4)?,
            })
        });

        match rows {
            Ok(iter) => {
                for row in iter {
                    match row {
                        Ok(file) => self.files.push(file),
                        Err(e) => {
                            self.last_error = Some(format!("Error reading row: {e}"));
                            break;
                        },
                    }
                }
            },
            Err(e) => {
                self.last_error = Some(format!("Query error: {e}"));
            },
        }
    }
    fn delete_selected(&mut self) -> Result<usize, String> {
        self.last_error = None;

        let mut conn =
            Connection::open("fluxrs.db").map_err(|e| format!("Failed to open database: {e}"))?;
        let tx = conn.transaction().map_err(|e| format!("Could not start transaction: {e}"))?;

        // Avoid borrowing `self` across the loop by copying IDs out first
        let ids: Vec<i64> = self.selected_ids.iter().copied().collect();

        let mut deleted = 0usize;
        for id in ids {
            if let Err(e) = tx.execute("DELETE FROM data_files WHERE id = ?", params![id]) {
                let _ = tx.rollback(); // best-effort cleanup
                return Err(format!("Failed to delete id {id}: {e}"));
            }
            deleted += 1;
        }

        tx.commit().map_err(|e| format!("Failed to commit deletion: {e}"))?;

        self.reload_files();
        Ok(deleted)
    }
}

#[derive(PartialEq, Eq)]
enum ManagePanel {
    DeleteCycle,
    DeleteMeasurement,
    DeleteMeteo,
    DeleteHeight,
    DeleteChamber,
    DeleteFlux,
    Empty,
}
impl Default for ManagePanel {
    fn default() -> Self {
        Self::Empty
    }
}

pub struct ManageApp {
    pub open: bool,
    project: Project,
    live_panel: ManagePanel,
    del_measurement: DeleteMeasurementApp,
    del_meteo: DeleteMeasurementApp,
    del_height: DeleteMeasurementApp,
    del_meta: DeleteMeasurementApp,
    recalc_app: RecalculateApp,
}

impl Default for ManageApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ManageApp {
    pub fn new() -> Self {
        Self {
            open: false,
            live_panel: ManagePanel::default(),
            project: Project::default(),
            del_measurement: DeleteMeasurementApp::new(Project::default(), DataType::Gas),
            del_meteo: DeleteMeasurementApp::new(Project::default(), DataType::Meteo),
            del_height: DeleteMeasurementApp::new(Project::default(), DataType::Height),
            del_meta: DeleteMeasurementApp::new(Project::default(), DataType::Chamber),
            recalc_app: RecalculateApp::default(),
        }
    }
}

impl ManageApp {
    fn close_manage_proj(&mut self) {
        self.open = false;
        self.live_panel = ManagePanel::default();
    }
    pub fn show_manage_proj_data(
        &mut self,
        ctx: &egui::Context,
        project: Project,
        runtime: &tokio::runtime::Runtime,
        progress_sender: UnboundedSender<ProcessEvent>,
    ) {
        self.project = project;

        if !self.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.close_manage_proj();
            return;
        }

        let mut open = self.open;
        Window::new("Manage project")
            .open(&mut open)
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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if ui.button("Close").clicked() {
                        self.close_manage_proj();
                    };
                });
                ui.horizontal_wrapped(|ui| {
                    let container_response = ui.response();
                    container_response.widget_info(|| {
                        WidgetInfo::labeled(WidgetType::RadioGroup, true, "Select panel")
                    });

                    // let panel_switching_allowed = !self.validation_panel.init_in_progress;
                    let panel_switching_allowed = true;
                    ui.ctx().clone().with_accessibility_parent(container_response.id, || {
                        ui.add_enabled(panel_switching_allowed, |ui: &mut egui::Ui| {
                            ui.horizontal(|ui| {
                                ui.selectable_value(
                                    &mut self.live_panel,
                                    ManagePanel::DeleteCycle,
                                    "Delete chamber cycles",
                                );
                                ui.selectable_value(
                                    &mut self.live_panel,
                                    ManagePanel::DeleteMeasurement,
                                    "Delete gas measurements",
                                );
                                ui.selectable_value(
                                    &mut self.live_panel,
                                    ManagePanel::DeleteMeteo,
                                    "Delete meteo data",
                                );
                                ui.selectable_value(
                                    &mut self.live_panel,
                                    ManagePanel::DeleteHeight,
                                    "Delete height data",
                                );
                                ui.selectable_value(
                                    &mut self.live_panel,
                                    ManagePanel::DeleteChamber,
                                    "Delete chamber metadata",
                                );
                            })
                            .response
                        });
                    });
                });
                ui.separator();
                let project_clone = self.project.clone();
                match self.live_panel {
                    ManagePanel::DeleteMeasurement => {
                        self.del_measurement.ui(
                            ui,
                            ctx,
                            project_clone,
                            DataType::Gas,
                            &self.recalc_app,
                            runtime,
                            progress_sender.clone(),
                        );
                    },
                    ManagePanel::DeleteCycle => {
                        self.del_measurement.ui(
                            ui,
                            ctx,
                            project_clone,
                            DataType::Cycle,
                            &self.recalc_app,
                            runtime,
                            progress_sender.clone(),
                        );
                    },
                    ManagePanel::DeleteMeteo => {
                        self.del_measurement.ui(
                            ui,
                            ctx,
                            project_clone,
                            DataType::Meteo,
                            &self.recalc_app,
                            runtime,
                            progress_sender.clone(),
                        );
                    },
                    ManagePanel::DeleteHeight => {
                        self.del_measurement.ui(
                            ui,
                            ctx,
                            project_clone,
                            DataType::Height,
                            &self.recalc_app,
                            runtime,
                            progress_sender.clone(),
                        );
                    },
                    ManagePanel::DeleteChamber => {
                        self.del_measurement.ui(
                            ui,
                            ctx,
                            project_clone,
                            DataType::Chamber,
                            &self.recalc_app,
                            runtime,
                            progress_sender.clone(),
                        );
                    },
                    ManagePanel::Empty => {},
                    ManagePanel::DeleteFlux => {},
                }
            });
    }
}
