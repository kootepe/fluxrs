use crate::keybinds::{Action, KeyBind, KeyBindings};
use crate::ui::project_ui::{Project, ProjectApp};
use crate::ui::table_ui::TableApp;
use crate::ui::validation_ui::ValidationApp;
use egui::{FontFamily, ScrollArea, Separator, WidgetInfo, WidgetType};

pub enum AppEvent {
    SelectProject(Option<Project>),
}

#[derive(Default, PartialEq)]
struct EmptyPanel {}

impl EmptyPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui) {}
}
#[derive(PartialEq, Eq)]
pub enum Panel {
    Validation,
    DataInit,
    DataLoad,
    FileInit,
    ProjInit,
    DataTable,
    DownloadData,
    Empty,
}
impl Default for Panel {
    fn default() -> Self {
        Self::Empty
    }
}
#[derive(Default)]
pub struct MainApp {
    pub show_settings: bool,
    live_panel: Panel,
    pub validation_panel: ValidationApp,
    table_panel: TableApp,
    proj_panel: ProjectApp,
    empty_panel: EmptyPanel,
}

impl MainApp {
    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.apply_font_size(ctx, self.validation_panel.font_size);
        if self.validation_panel.selected_project.is_none() {
            self.proj_panel.load_projects_from_db().unwrap();
            self.validation_panel.selected_project = self.proj_panel.project.clone();
            self.validation_panel.keybinds =
                KeyBindings::load_from_file("keybinds.json").unwrap_or_default();
        }
        if let Some(event) = self.proj_panel.update_project() {
            match event {
                AppEvent::SelectProject(proj) => {
                    if self.validation_panel.selected_project.clone().unwrap_or_default().name
                        != proj.clone().unwrap_or_default().name
                    {
                        self.validation_panel.selected_project = proj;
                        self.validation_panel.cycles = Vec::new();
                    }
                },
            }
        }
        // println!("{:?}", self.validation_panel.selected_project);
        for (_text_style, font_id) in ui.style_mut().text_styles.iter_mut() {
            // font_id.size = self.validation_panel.font_size;
            font_id.family = FontFamily::Monospace;
        }
        ui.horizontal_wrapped(|ui| {
            let container_response = ui.response();
            container_response
                .widget_info(|| WidgetInfo::labeled(WidgetType::RadioGroup, true, "Select panel"));

            let panel_switching_allowed = !self.validation_panel.init_in_progress;
            ui.ctx().clone().with_accessibility_parent(container_response.id, || {
                ui.add_enabled(panel_switching_allowed, |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::Validation,
                            "Validate measurements",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::DataLoad,
                            "Load measurements",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::DataInit,
                            "Initiate measurements",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::FileInit,
                            "Upload files to db",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::ProjInit,
                            "Initiate project",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::DataTable,
                            "View data in db",
                        );
                        ui.selectable_value(
                            &mut self.live_panel,
                            Panel::DownloadData,
                            "Download data",
                        );
                    })
                    .response
                });
            });
        });
        ui.separator();

        match self.live_panel {
            Panel::Validation => {
                self.validation_panel.ui(ui, ctx);
            },
            Panel::DataLoad => {
                self.validation_panel.load_ui(ui, ctx);
            },
            Panel::DataInit => {
                self.validation_panel.init_ui(ui, ctx);
            },
            Panel::FileInit => {
                self.validation_panel.file_ui(ui, ctx);
            },
            Panel::DataTable => {
                self.table_panel.table_ui(ui, ctx);
            },
            Panel::DownloadData => {
                self.validation_panel.dl_ui(ui, ctx);
            },
            Panel::ProjInit => {
                self.proj_panel.proj_ui(ui);
            },
            Panel::Empty => {
                self.empty_panel.ui(ui);
            },
        }
    }
    pub fn settings_ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("Settings panel").show(ctx, |ui| {
        ScrollArea::vertical().show(ui, |ui| {

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        self.validation_panel.render_measurement_plots(ui);
                        self.validation_panel.enable_floaters(ui);
                    });
                    self.validation_panel.render_lin_plot_selection(ui);

                    ui.add(Separator::default().vertical());
                    self.validation_panel.render_roblin_plot_selection(ui);
                    ui.add(Separator::default().vertical());
                    self.validation_panel.render_poly_plot_selection(ui);
                });
            });
            ui.group(|ui| {
                ui.label("Adjust hiding thresholds");
                ui.label("These are based on the main gas.");
                ui.label("Will not mark measurements as invalid, but allows hiding measurements in current view.");
                ui.label("Double click to reset");
                egui::Grid::new("thresholds_grid").min_col_width(100.).show(ui,|ui| {
                ui.label("RMSE");
                let rmse_adjuster = ui.add(
                    egui::DragValue::new(&mut self.validation_panel.rmse_thresh)
                        .speed(0.1)
                        .range(0.0..=100.)
                );
                if rmse_adjuster.changed() {
                    self.validation_panel.update_plots();
                }
                if rmse_adjuster.double_clicked() {
                    self.validation_panel.rmse_thresh = 25.;
                    self.validation_panel.update_plots();
                }
                ui.end_row();

                ui.label("r2");
                let r2_adjuster = ui.add(
                    egui::DragValue::new(&mut self.validation_panel.r2_thresh)
                        .speed(0.00001)
                        .range(0.0..=1.0)
                );
                if r2_adjuster.changed() {
                    self.validation_panel.update_plots()
                }
                if r2_adjuster.double_clicked() {
                    self.validation_panel.r2_thresh = 0.9;
                    self.validation_panel.update_plots();
                }
                ui.end_row();

                ui.label("p-value");
                let p_val_adjuster = ui.add(
                    egui::DragValue::new(&mut self.validation_panel.p_val_thresh)
                        .speed(0.0001)
                        .range(0.0..=1.0)
                );
                if p_val_adjuster.changed() {
                    self.validation_panel.update_plots()
                }
                if p_val_adjuster.double_clicked() {
                    self.validation_panel.p_val_thresh = 0.05;
                    self.validation_panel.update_plots();
                }
                        ui.end_row();

                ui.label("t0 concentration");
                let t0_adjuster= ui.add(
                    egui::DragValue::new(&mut self.validation_panel.t0_thresh)
                        .speed(1)
                        .range(0.0..=30000.0)
                );
                if t0_adjuster.changed() {
                    self.validation_panel.update_plots()
                }
                if t0_adjuster.double_clicked() {
                    self.validation_panel.t0_thresh= 30000.;
                    self.validation_panel.update_plots();
                }
            });
                        ui.end_row();
        });
            ui.separator();
            self.keybinding_settings_ui(ui);
                });
        });
    }

    fn keybinding_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("Keybinds");
            ui.label("Press rebind and hit key to set keybind");
            ui.label("Esc to cancel");
            egui::Grid::new("keybinds").show(ui, |ui| {
                for action in [
                    Action::NextCycle,
                    Action::PreviousCycle,
                    Action::ZoomToMeasurement,
                    Action::ResetCycle,
                    Action::SearchLagPrevious,
                    Action::SearchLag,
                    Action::IncrementLag,
                    Action::DecrementLag,
                    Action::IncrementDeadband,
                    Action::DecrementDeadband,
                    Action::IncrementCH4Deadband,
                    Action::DecrementCH4Deadband,
                    Action::IncrementCO2Deadband,
                    Action::DecrementCO2Deadband,
                    Action::ToggleValidity,
                    Action::ToggleCH4Validity,
                    Action::ToggleCO2Validity,
                    Action::ToggleH2OValidity,
                    Action::ToggleN2OValidity,
                    Action::ToggleBad,
                    Action::ToggleShowValids,
                    Action::ToggleShowInvalids,
                    Action::ToggleShowBad,
                    Action::ToggleShowSettings,
                    Action::ToggleShowLegend,
                    Action::ToggleShowDetails,
                    Action::TogglePlotWidthsWindow,
                    Action::ToggleShowResiduals,
                    Action::ToggleShowStandResiduals,
                ] {
                    let mut rebind_text = "Rebind";
                    if let Some(pending) = self.validation_panel.awaiting_rebind {
                        if pending == action {
                            rebind_text = "Press key to rebind";
                        }
                    }
                    ui.label(format!("{}:", action));
                    if let Some(key) = self.validation_panel.keybinds.key_for(action) {
                        ui.label(format!("{}", key));
                    } else {
                        ui.label("Unbound");
                    }

                    if ui.button(rebind_text).clicked() {
                        self.validation_panel.awaiting_rebind = Some(action);
                    }
                    if self.validation_panel.keybinds.key_for(action).is_some()
                        && ui.button("Unbind").clicked()
                    {
                        self.validation_panel.keybinds.remove(&action);
                        self.validation_panel.keybinds.save_to_file("keybinds.json").ok();
                        self.validation_panel.awaiting_rebind = None;
                    }
                    ui.end_row();
                }
            });
        });

        if let Some(action) = self.validation_panel.awaiting_rebind {
            // if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            //     self.validation_panel.awaiting_rebind = None; // cancel
            // } else {
            if let Some((key, modifiers)) = ui.input(|i| {
                i.raw.events.iter().find_map(|event| {
                    if let egui::Event::Key { key, pressed: true, .. } = event {
                        if *key != egui::Key::Escape {
                            Some((*key, i.modifiers))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            }) {
                let keybind = KeyBind {
                    key,
                    ctrl: modifiers.ctrl,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                };
                self.validation_panel.keybinds.set(action, keybind);
                self.validation_panel.keybinds.save_to_file("keybinds.json").ok();
                self.validation_panel.awaiting_rebind = None;
            }
        }
    }
    fn apply_font_size(&self, ctx: &egui::Context, font_size: f32) {
        use egui::{FontId, TextStyle};

        let mut style = (*ctx.style()).clone();

        // Update font sizes for the main text styles
        style.text_styles = [
            (TextStyle::Heading, FontId::monospace(font_size + 6.0)),
            (TextStyle::Body, FontId::monospace(font_size)),
            (TextStyle::Monospace, FontId::monospace(font_size)),
            (TextStyle::Button, FontId::monospace(font_size)),
            (TextStyle::Small, FontId::monospace(font_size - 2.0)),
        ]
        .into();

        ctx.set_style(style);
    }
}
