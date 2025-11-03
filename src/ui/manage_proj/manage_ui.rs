use crate::ui::manage_proj::project::Project;
use crate::ui::manage_proj::project_ui::{clicked_outside_window, ProjectApp};
use egui::{Align2, Area, Color32, Context, Frame, Id, Window};

pub struct ManageApp {
    pub open: bool,
    project: Project,
    delete_measurement: bool,
    delete_fluxes: bool,
    delete_meteo: bool,
    delete_height: bool,
    delete_chamber: bool,
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
            project: Project::default(),
            delete_measurement: false,
            delete_fluxes: false,
            delete_meteo: false,
            delete_height: false,
            delete_chamber: false,
        }
    }
}

impl ManageApp {
    fn close_manage_proj(&mut self) {
        self.open = false;
    }
    pub fn show_manage_proj_data(&mut self, ctx: &egui::Context, project: Project) {
        self.project = project;

        if !self.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.open = false;
            return;
        }

        let can_close = true;
        let wr = Window::new("Manage project")
            .open(&mut self.open)
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
                ui.horizontal(|ui| {
                    if ui.button("Delete measurement data").clicked() {
                        self.delete_measurement = true;
                    };
                    if ui.button("Delete calculated fluxes").clicked() {
                        self.delete_fluxes = true;
                    };
                    if ui.button("Delete meteo data").clicked() {
                        self.delete_meteo = true;
                    };
                    if ui.button("Delete height data").clicked() {
                        self.delete_height = true;
                    };
                    if ui.button("Delete chamber data").clicked() {
                        self.delete_chamber = true;
                    };
                });
            });

        if clicked_outside_window(ctx, wr.as_ref()) && can_close {
            self.close_manage_proj();
        }
    }
}
